use std::{
    collections::HashMap,
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

pub const META_FILENAME: &str = "meta.json";
pub const INTERNAL_BIN_FILENAME: &str = "internal.bin";
pub const BUNDLE_FILE_NAME: &str = "bundle.tar.zstd";

pub fn unzip_tarball(bundle_path: &PathBuf, unpack_dir: &PathBuf) -> anyhow::Result<()> {
    let tar_file = File::open(bundle_path)?;
    let zstd_decoder = zstd::Decoder::new(tar_file)?;
    let mut archive = tar::Archive::new(zstd_decoder);
    archive.unpack(unpack_dir)?;
    Ok(())
}

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

        // Add the internal binary file if it exists.
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
        let bundle_temp_file = bundle_temp_dir.path().join(BUNDLE_FILE_NAME);
        self.make_tarball(&bundle_temp_file)?;
        Ok((bundle_temp_file, bundle_temp_dir))
    }
}

/// Reads and decompresses a .tar.zstd file from an input stream into multiple specified files.
pub async fn extract_files_from_tarball<R: AsyncBufRead>(
    input: R,
    file_names: &[&str],
) -> anyhow::Result<HashMap<String, Vec<u8>>> {
    let zstd_decoder = ZstdDecoder::new(Box::pin(input));
    let archive = Archive::new(zstd_decoder);
    let mut entries = archive.entries()?;

    let mut extracted_files = HashMap::new();
    let file_names_set: std::collections::HashSet<&str> = file_names.iter().cloned().collect();

    while let Some(entry) = entries.next().await {
        let mut owned_entry = entry?;
        let path_str = owned_entry.path()?.to_str().unwrap_or_default().to_owned();

        if file_names_set.contains(path_str.as_str()) {
            let mut file_bytes = Vec::new();
            owned_entry.read_to_end(&mut file_bytes).await?;
            extracted_files.insert(path_str, file_bytes);

            if extracted_files.len() == file_names.len() {
                break;
            }
        }
    }

    let missing_files: Vec<&str> = file_names
        .iter()
        .filter(|&name| !extracted_files.contains_key(*name))
        .cloned()
        .collect();

    if !missing_files.is_empty() {
        return Err(anyhow::anyhow!(
            "Files not found in tarball: {:?}",
            missing_files
        ));
    }

    Ok(extracted_files)
}

pub fn parse_meta(meta_bytes: Vec<u8>) -> anyhow::Result<VersionedBundle> {
    if let Ok(message) = serde_json::from_slice(&meta_bytes) {
        return Ok(VersionedBundle::V0_7_7(message));
    }

    if let Ok(message) = serde_json::from_slice(&meta_bytes) {
        return Ok(VersionedBundle::V0_7_6(message));
    }

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

/// Reads and decompresses a .tar.zstd file from an input stream into just a `meta.json` file.
/// This assumes that the `meta.json` file will be the first entry in the tarball.
pub async fn parse_meta_from_tarball<R: AsyncBufRead>(input: R) -> anyhow::Result<VersionedBundle> {
    let zstd_decoder = ZstdDecoder::new(Box::pin(input));
    let archive = Archive::new(zstd_decoder);

    // Again, note that the below method specifically is only looking at the first entry in the tarball.
    if let Some(first_entry) = archive.entries()?.next().await {
        let mut owned_first_entry = first_entry?;
        let path_str = owned_first_entry
            .path()?
            .to_str()
            .unwrap_or_default()
            .to_owned();

        if path_str == META_FILENAME {
            let mut meta_bytes = Vec::new();
            owned_first_entry.read_to_end(&mut meta_bytes).await?;

            return parse_meta(meta_bytes);
        }
    }
    Err(anyhow::anyhow!("No meta.json file found in the tarball"))
}

/// Reads and decompresses a .tar.zstd file from an input stream into just the internal bin file.
pub async fn parse_internal_bin_from_tarball<R: AsyncBufRead>(
    input: R,
) -> anyhow::Result<TestResult> {
    let extracted_files = extract_files_from_tarball(input, &[INTERNAL_BIN_FILENAME]).await?;
    if let Some(internal_bin_bytes) = extracted_files.get(INTERNAL_BIN_FILENAME) {
        let test_result: TestResult =
            TestResult::decode(internal_bin_bytes.as_slice()).map_err(|err| {
                anyhow::anyhow!("Failed to decode {}: {}", INTERNAL_BIN_FILENAME, err)
            })?;
        return Ok(test_result);
    }

    Err(anyhow::anyhow!(
        "No {} file found in the tarball",
        INTERNAL_BIN_FILENAME
    ))
}

pub async fn parse_internal_bin_and_meta_from_tarball<R: AsyncBufRead>(
    input: R,
) -> anyhow::Result<(TestResult, VersionedBundle)> {
    let extracted_files =
        extract_files_from_tarball(input, &[META_FILENAME, INTERNAL_BIN_FILENAME]).await?;

    let internal_bin_bytes = extracted_files
        .get(INTERNAL_BIN_FILENAME)
        .ok_or_else(|| anyhow::anyhow!("No {} file found in the tarball", INTERNAL_BIN_FILENAME))?;

    let test_result: TestResult = TestResult::decode(internal_bin_bytes.as_slice())
        .map_err(|err| anyhow::anyhow!("Failed to decode {}: {}", INTERNAL_BIN_FILENAME, err))?;

    let meta_bytes = extracted_files
        .get(META_FILENAME)
        .ok_or_else(|| anyhow::anyhow!("No {} file found in the tarball", META_FILENAME))?;

    let versioned_bundle = parse_meta(meta_bytes.to_vec())?;

    Ok((test_result, versioned_bundle))
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        env,
        time::{SystemTime, UNIX_EPOCH},
    };

    use context::repo::BundleRepo;
    use tempfile::tempdir;

    use super::*;
    use crate::bundle_meta::{
        BundleMeta, BundleMetaBaseProps, BundleMetaDebugProps, BundleMetaJunitProps, META_VERSION,
    };
    use crate::files::{FileSet, FileSetType};
    use crate::Test;

    #[tokio::test]
    pub async fn test_bundle_meta_is_first_entry() {
        let mut repo = BundleRepo::default();
        let upload_time_epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_else(|_| std::time::Duration::from_secs(0))
            .as_secs();
        repo.repo.owner = "org".to_string();
        repo.repo.name = "repo".to_string();
        let mut envs: HashMap<String, String> = HashMap::new();
        envs.insert("key".to_string(), "value".to_string());
        let meta = BundleMeta {
            junit_props: BundleMetaJunitProps::default(),
            bundle_upload_id_v2: String::with_capacity(0),
            debug_props: BundleMetaDebugProps {
                command_line: String::with_capacity(0),
            },
            variant: Some("variant".to_string()),
            base_props: BundleMetaBaseProps {
                version: META_VERSION.to_string(),
                org: "org".to_string(),
                repo: repo.clone(),
                cli_version: "0.0.1".to_string(),
                bundle_upload_id: "00".to_string(),
                tags: vec![],
                file_sets: vec![FileSet::new(
                    FileSetType::Junit,
                    vec![],
                    "test*.xml".to_string(),
                    None,
                )],
                upload_time_epoch,
                test_command: Some("exit 1".to_string()),
                quarantined_tests: vec![Test::new(
                    None,
                    "name".to_string(),
                    "parent_name".to_string(),
                    Some("class_name".to_string()),
                    None,
                    "org".to_string(),
                    &repo.repo,
                    None,
                )],
                os_info: Some(env::consts::OS.to_string()),
                codeowners: None,
                envs,
            },
            internal_bundled_file: None,
        };
        let bundler_util = BundlerUtil::new(meta, None);
        let temp_dir = tempdir().unwrap();
        let bundle_path = temp_dir.path().join(BUNDLE_FILE_NAME);

        assert!(bundler_util.make_tarball(&bundle_path).is_ok());
        assert!(bundle_path.exists());

        let tarball_file = async_std::fs::File::open(&bundle_path).await.unwrap();
        let reader = async_std::io::BufReader::new(tarball_file);

        let parsed_meta = parse_meta_from_tarball(reader).await;
        assert!(parsed_meta.is_ok());
        match parsed_meta.unwrap() {
            VersionedBundle::V0_7_7(meta) => {
                assert_eq!(meta.base_props.version, META_VERSION.to_string());
                assert_eq!(meta.variant, Some("variant".to_string()));
                assert_eq!(meta.base_props.org, "org");
                assert_eq!(meta.base_props.repo.repo.name, "repo");
                assert_eq!(meta.base_props.repo.repo.owner, "org");
                assert_eq!(meta.base_props.cli_version, "0.0.1");
                assert_eq!(meta.base_props.bundle_upload_id, "00");
                assert_eq!(meta.base_props.file_sets.len(), 1);
                assert_eq!(meta.base_props.upload_time_epoch, upload_time_epoch);
                assert_eq!(meta.base_props.test_command, Some("exit 1".to_string()));
                assert_eq!(meta.base_props.quarantined_tests.len(), 1);
                assert_eq!(meta.base_props.os_info, Some(env::consts::OS.to_string()));
                assert!(meta.base_props.codeowners.is_none());
                assert!(meta.internal_bundled_file.is_none());
                assert!(meta.base_props.envs.contains_key("key"));
            }
            _ => panic!("Expected V0_7_7 versioned bundle"),
        }
    }
}
