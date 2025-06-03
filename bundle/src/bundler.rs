use std::{
    fs::File,
    io::{Seek, Write},
    path::PathBuf,
};

use async_compression::futures::bufread::ZstdDecoder;
use async_std::{io::ReadExt, stream::StreamExt};
use async_tar_wasm::Archive;
use codeowners::CodeOwners;
use context::bazel_bep::common::BepParseResult;
use futures_io::AsyncBufRead;
use prost::Message;
use proto::test_context::test_run::TestResult;
use tempfile::TempDir;
#[cfg(feature = "wasm")]
use tsify_next::Tsify;
#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

use crate::bundle_meta::{BundleMeta, VersionedBundle};

/// Utility type for packing files into tarball.
///
#[cfg_attr(feature = "wasm", derive(Tsify))]
pub struct BundlerUtil {
    meta: BundleMeta,
    bep_result: Option<BepParseResult>,
}

const META_FILENAME: &str = "meta.json";
const INTERNAL_BIN_FILENAME: &str = "internal.bin";

impl BundlerUtil {
    const ZSTD_COMPRESSION_LEVEL: i32 = 15; // This gives roughly 10x compression for text, 22 gives 11x.

    pub fn new(meta: BundleMeta, bep_result: Option<BepParseResult>) -> Self {
        Self { meta, bep_result }
    }

    /// Writes compressed tarball to disk.
    ///
    pub fn make_tarball(&self, bundle_path: &PathBuf) -> anyhow::Result<()> {
        let mut total_bytes_in: u64 = 0;

        let tar_file = File::create(bundle_path)?;
        let zstd_encoder = zstd::Encoder::new(tar_file, Self::ZSTD_COMPRESSION_LEVEL)?;
        let mut tar = tar::Builder::new(zstd_encoder);

        // Serialize meta and add it to the tarball.
        {
            let meta_json_bytes = serde_json::to_vec(&self.meta)?;
            total_bytes_in += meta_json_bytes.len() as u64;
            let mut meta_temp = tempfile::tempfile()?;
            meta_temp.write_all(&meta_json_bytes)?;
            meta_temp.seek(std::io::SeekFrom::Start(0))?;
            tar.append_file(META_FILENAME, &mut meta_temp)?;
        }

        // Add all files to the tarball.
        self.meta
            .base_props
            .file_sets
            .iter()
            .try_for_each(|file_set| {
                file_set.files.iter().try_for_each(|bundled_file| {
                    let path = std::path::Path::new(&bundled_file.original_path);
                    let mut file = File::open(path)?;
                    tar.append_file(&bundled_file.path, &mut file)?;
                    total_bytes_in += std::fs::metadata(path)?.len();
                    Ok::<(), anyhow::Error>(())
                })?;
                Ok::<(), anyhow::Error>(())
            })?;
        if let Some(bundled_file) = self.meta.internal_bundled_file.as_ref() {
            let path = std::path::Path::new(&bundled_file.original_path);
            let mut file = File::open(path)?;
            tar.append_file(&bundled_file.path, &mut file)?;
            total_bytes_in += std::fs::metadata(path)?.len();
        }

        if let Some(CodeOwners { ref path, .. }) = self.meta.base_props.codeowners {
            let mut file = File::open(path)?;
            tar.append_file("CODEOWNERS", &mut file)?;
            total_bytes_in += std::fs::metadata(path)?.len();
        }

        if let Some(bep_result) = self.bep_result.as_ref() {
            let mut bep_events_file =
                bep_result
                    .bep_test_events
                    .iter()
                    .fold(tempfile::tempfile()?, |f, event| {
                        if let Err(e) = serde_json::to_writer(&f, event) {
                            tracing::error!("Failed to write BEP event: {}", e);
                        }
                        f
                    });
            bep_events_file.flush()?;
            bep_events_file.seek(std::io::SeekFrom::Start(0))?;
            tar.append_file("bazel_bep.json", &mut bep_events_file)?;
            total_bytes_in += bep_events_file.seek(std::io::SeekFrom::End(0))?;
        }

        // Flush to disk.
        tar.into_inner()?.finish()?;

        let total_bytes_out = std::fs::metadata(bundle_path)?.len();
        let size_reduction = 1.0 - total_bytes_out as f64 / total_bytes_in as f64;

        tracing::info!(
            "Total bytes in: {}, total bytes out: {} (size reduction: {:.2}%)",
            total_bytes_in,
            total_bytes_out,
            size_reduction * 100.0,
        );

        Ok(())
    }

    pub fn make_tarball_in_temp_dir(&self) -> anyhow::Result<(PathBuf, TempDir)> {
        let bundle_temp_dir = tempfile::tempdir()?;
        let bundle_temp_file = bundle_temp_dir.path().join("bundle.tar.zstd");
        self.make_tarball(&bundle_temp_file)?;
        Ok((bundle_temp_file, bundle_temp_dir))
    }
}

async fn parse_file_from_tarball<R: AsyncBufRead>(
    input: R,
    filename: &str,
) -> anyhow::Result<Vec<u8>> {
    let zstd_decoder = ZstdDecoder::new(Box::pin(input));
    let archive = Archive::new(zstd_decoder);

    if let Some(first_entry) = archive.entries()?.next().await {
        let mut owned_first_entry = first_entry?;
        let path_str = owned_first_entry
            .path()?
            .to_str()
            .unwrap_or_default()
            .to_owned();

        if path_str == META_FILENAME {
            let mut file_bytes = Vec::new();
            owned_first_entry.read_to_end(&mut file_bytes).await?;

            return Ok(file_bytes);
        }
    }
    Err(anyhow::anyhow!(format!(
        "No file at {} found in the tarball",
        filename
    )))
}

/// Reads and decompresses a .tar.zstd file from an input stream into just a `meta.json` file
///
pub async fn parse_meta_from_tarball<R: AsyncBufRead>(input: R) -> anyhow::Result<VersionedBundle> {
    let meta_bytes = parse_file_from_tarball(input, META_FILENAME).await?;
    parse_meta(meta_bytes)
}

pub fn parse_meta(meta_bytes: Vec<u8>) -> anyhow::Result<VersionedBundle> {
    if let Ok(message) = serde_json::from_slice(&meta_bytes) {
        return Ok(VersionedBundle::V0_6_3(message));
    }

    if let Ok(message) = serde_json::from_slice(&meta_bytes) {
        return Ok(VersionedBundle::V0_6_2(message));
    }

    if let Ok(message) = serde_json::from_slice(&meta_bytes) {
        return Ok(VersionedBundle::V0_5_34(message));
    }

    let base_bundle = serde_json::from_slice(&meta_bytes)?;
    Ok(VersionedBundle::V0_5_29(base_bundle))
}

/// Reads and decompresses a .tar.zstd file from an input stream into just the test results from it
///
pub async fn parse_internal_bin_from_tarball<R: AsyncBufRead>(
    input: R,
) -> anyhow::Result<TestResult> {
    let meta_bytes = parse_file_from_tarball(input, INTERNAL_BIN_FILENAME).await?;
    if let Ok(test_result) = TestResult::decode(meta_bytes.as_slice()) {
        return Ok(test_result);
    }

    Err(anyhow::anyhow!(
        "Failed to decode internal.bin from tarball"
    ))
}
