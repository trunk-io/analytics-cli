use std::{
    fs::File,
    io::{Seek, Write},
    path::PathBuf,
    pin::Pin,
};

use async_compression::futures::bufread::ZstdDecoder;
use async_std::{io::ReadExt, stream::StreamExt};
use async_tar_wasm::{Archive, Entries, Entry};
use codeowners::CodeOwners;
use context::bazel_bep::common::BepParseResult;
use futures_io::AsyncBufRead;
use prost::Message;
use proto::test_context::test_run::TestReport;
use tempfile::TempDir;
#[cfg(feature = "wasm")]
use tsify_next::Tsify;
#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

use crate::bundle_meta::{BundleMeta, VersionedBundle};
use crate::files::FileSetType;
use crate::traces::{DiscoveredTrace, trace_archive_name};

/// Utility type for packing files into tarball.
///
#[cfg_attr(feature = "wasm", derive(Tsify))]
pub struct BundlerUtil<'a> {
    meta: &'a BundleMeta,
    bep_result: Option<BepParseResult>,
    /// Playwright traces discovered from JUnit `[[ATTACHMENT|...]]` references.
    /// Each trace is packed at `traces/<identity_hash>.zip`. Multiple
    /// `DiscoveredTrace` entries with the same identity hash collapse to a
    /// single tarball entry — the first non-empty source wins.
    traces: Vec<DiscoveredTrace>,
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

impl<'a> BundlerUtil<'a> {
    const ZSTD_COMPRESSION_LEVEL: i32 = 15; // This gives roughly 10x compression for text, 22 gives 11x.

    pub fn new(meta: &'a BundleMeta, bep_result: Option<BepParseResult>) -> Self {
        Self {
            meta,
            bep_result,
            traces: Vec::new(),
        }
    }

    /// Attaches Playwright traces discovered from JUnit attachments to be
    /// packed alongside `internal.bin`. Repeated calls replace the previous
    /// list rather than append.
    pub fn with_traces(mut self, traces: Vec<DiscoveredTrace>) -> Self {
        self.traces = traces;
        self
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

        // Add the internal binary file if it exists.
        if let Some(bundled_file) = self.meta.internal_bundled_file.as_ref() {
            let path = std::path::Path::new(&bundled_file.original_path);
            let mut file = File::open(path)?;
            tar.append_file(&bundled_file.path, &mut file)?;
            total_bytes_in += std::fs::metadata(path)?.len();
        }

        // Add all files to the tarball.
        // Skip Internal file_sets if internal_bundled_file is set, to avoid adding the same file twice.
        // If internal_bundled_file is None, we still add Internal file_sets as a fallback.
        let has_internal_bundled_file = self.meta.internal_bundled_file.is_some();
        self.meta
            .base_props
            .file_sets
            .iter()
            .filter(|file_set| {
                !has_internal_bundled_file || file_set.file_set_type != FileSetType::Internal
            })
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

        // Pack discovered Playwright traces. Each is best-effort: a missing
        // or unreadable trace is logged and skipped so a single bad file
        // can't take down the whole upload. Duplicate identity hashes
        // collapse to one tarball entry.
        let mut packed_trace_hashes: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        for trace in &self.traces {
            if !packed_trace_hashes.insert(trace.identity_hash.clone()) {
                continue;
            }
            let archive_name = trace_archive_name(&trace.identity_hash);
            let mut file = match File::open(&trace.source_path) {
                Ok(f) => f,
                Err(e) => {
                    tracing::warn!(
                        "Skipping Playwright trace at {:?}: {}",
                        trace.source_path,
                        e
                    );
                    continue;
                }
            };
            if let Err(e) = tar.append_file(&archive_name, &mut file) {
                tracing::warn!(
                    "Failed to append Playwright trace {:?} as {}: {}",
                    trace.source_path,
                    archive_name,
                    e
                );
                continue;
            }
            total_bytes_in += std::fs::metadata(&trace.source_path)
                .map(|m| m.len())
                .unwrap_or(0);
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

pub fn parse_meta(meta_bytes: Vec<u8>) -> anyhow::Result<VersionedBundle> {
    if let Ok(message) = serde_json::from_slice(&meta_bytes) {
        return Ok(VersionedBundle::V0_7_8(message));
    }

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

pub fn bin_parse(bin: &Vec<u8>) -> anyhow::Result<TestReport> {
    if let Ok(test_report) = proto::test_context::test_run::TestReport::decode(bin.as_slice()) {
        Ok(test_report)
    } else {
        let test_result = proto::test_context::test_run::TestResult::decode(bin.as_slice())
            .map_err(|err| {
                anyhow::anyhow!("Failed to decode {}: {}", INTERNAL_BIN_FILENAME, err)
            })?;
        Ok(TestReport {
            test_results: vec![test_result],
            ..Default::default()
        })
    }
}

fn archive_entries<R: AsyncBufRead>(
    archive: R,
) -> anyhow::Result<Entries<ZstdDecoder<Pin<Box<R>>>>> {
    let zstd_decoder = ZstdDecoder::new(Box::pin(archive));
    let archive = Archive::new(zstd_decoder);
    Ok(archive.entries()?)
}

async fn parse_meta_entry<R: AsyncBufRead>(
    entry: &mut Entry<Archive<ZstdDecoder<Pin<Box<R>>>>>,
) -> anyhow::Result<Option<VersionedBundle>> {
    if let Some(path_str) = entry.path()?.to_str()
        && path_str == META_FILENAME
    {
        let mut meta_bytes = Vec::new();
        entry.read_to_end(&mut meta_bytes).await?;
        Ok(Some(parse_meta(meta_bytes)?))
    } else {
        Ok(None)
    }
}

async fn parse_meta_from_first_entry<R: AsyncBufRead>(
    entries: &mut Entries<ZstdDecoder<Pin<Box<R>>>>,
) -> anyhow::Result<VersionedBundle> {
    let Some(first_entry) = entries.next().await else {
        return Err(anyhow::anyhow!("No entries found in the tarball"));
    };
    let Some(meta) = parse_meta_entry(&mut first_entry?).await? else {
        return Err(anyhow::anyhow!("No meta.json file found in the tarball"));
    };
    Ok(meta)
}

async fn parse_internal_bin_entry<R: AsyncBufRead>(
    entry: &mut Entry<Archive<ZstdDecoder<Pin<Box<R>>>>>,
    filename: Option<&str>,
) -> anyhow::Result<Option<TestReport>> {
    if let Some(path_str) = entry.path()?.to_str()
        && (Some(path_str) == filename || path_str == INTERNAL_BIN_FILENAME)
    {
        let mut internal_bin_bytes = Vec::new();
        entry.read_to_end(&mut internal_bin_bytes).await?;
        Ok(Some(bin_parse(&internal_bin_bytes)?))
    } else {
        Ok(None)
    }
}

/// Reads and decompresses a .tar.zstd file from an input stream into just a `meta.json` file.
/// This assumes that the `meta.json` file will be the first entry in the tarball.
pub async fn parse_meta_from_tarball<R: AsyncBufRead>(input: R) -> anyhow::Result<VersionedBundle> {
    let mut entries = archive_entries(input)?;
    parse_meta_from_first_entry(&mut entries).await
}

/// Reads and decompresses a .tar.zstd file from an input stream into just the internal bin file.
pub async fn parse_internal_bin_from_tarball<R: AsyncBufRead>(
    input: R,
) -> anyhow::Result<TestReport> {
    parse_internal_bin_and_meta_from_tarball(input)
        .await
        .map(|(internal_bin, _)| internal_bin)
}

pub async fn parse_internal_bin_and_meta_from_tarball<R: AsyncBufRead>(
    input: R,
) -> anyhow::Result<(TestReport, VersionedBundle)> {
    let mut entries = archive_entries(input)?;
    let meta = parse_meta_from_first_entry(&mut entries).await?;
    let bundled_file = meta.internal_bundled_file();
    let internal_bin_filename = bundled_file
        .map(|bf| bf.path.as_str())
        .unwrap_or(INTERNAL_BIN_FILENAME);
    while let Some(entry) = entries.next().await {
        if let Some(internal_bin) =
            parse_internal_bin_entry(&mut entry?, Some(internal_bin_filename)).await?
        {
            return Ok((internal_bin, meta));
        };
    }
    Err(anyhow::anyhow!("No internal.bin file found in the tarball"))
}

/// Synchronously decompresses a zstd-compressed tarball from an in-memory buffer
/// and extracts the meta.json and internal.bin contents.
///
/// Callers should run this on a blocking thread pool.
pub fn parse_internal_bin_and_meta_from_bytes(
    bytes: &[u8],
) -> anyhow::Result<(TestReport, VersionedBundle)> {
    let zstd_decoder = zstd::Decoder::new(std::io::Cursor::new(bytes))?;
    let mut archive = tar::Archive::new(zstd_decoder);

    let mut meta: Option<VersionedBundle> = None;
    let mut internal_bin: Option<TestReport> = None;

    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.to_path_buf();
        let path_str = path.to_str().unwrap_or("");

        if path_str == META_FILENAME {
            let mut meta_bytes = Vec::new();
            std::io::Read::read_to_end(&mut entry, &mut meta_bytes)?;
            meta = Some(parse_meta(meta_bytes)?);
        } else if meta.is_some() {
            let expected_filename = meta
                .as_ref()
                .and_then(|m| m.internal_bundled_file())
                .map(|bf| bf.path.as_str())
                .unwrap_or(INTERNAL_BIN_FILENAME);

            if path_str == expected_filename || path_str == INTERNAL_BIN_FILENAME {
                let mut bin_bytes = Vec::new();
                std::io::Read::read_to_end(&mut entry, &mut bin_bytes)?;
                internal_bin = Some(bin_parse(&bin_bytes)?);
                break;
            }
        }
    }

    match (internal_bin, meta) {
        (Some(bin), Some(m)) => Ok((bin, m)),
        (_, None) => Err(anyhow::anyhow!("No meta.json file found in the tarball")),
        (None, _) => Err(anyhow::anyhow!("No internal.bin file found in the tarball")),
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        env,
        time::{SystemTime, UNIX_EPOCH},
    };

    use async_std::{
        fs,
        io::{self, ReadExt},
        stream::StreamExt,
    };
    use context::repo::BundleRepo;
    use proto::test_context::test_run::{TestReport, UploaderMetadata};
    use tempfile::{TempDir, tempdir};

    use super::{archive_entries, parse_internal_bin_entry};
    use crate::{
        BundledFile, Test, VersionedBundle,
        bundle_meta::{
            BundleMeta, BundleMetaBaseProps, BundleMetaDebugProps, BundleMetaJunitProps,
            META_VERSION,
        },
        bundler::{
            BUNDLE_FILE_NAME, BundlerUtil, INTERNAL_BIN_FILENAME,
            parse_internal_bin_and_meta_from_bytes, parse_meta_from_tarball,
        },
        files::{FileSet, FileSetType},
        parse_internal_bin_and_meta_from_tarball,
    };

    fn create_internal_bundled_file(
        temp_dir: &TempDir,
        bin_path: Option<String>,
    ) -> (BundledFile, TestReport) {
        let bin_path = bin_path.unwrap_or(INTERNAL_BIN_FILENAME.to_string());
        let full_bin_path = temp_dir.path().join(bin_path.clone());

        let test_report = TestReport {
            test_results: Vec::new(),
            uploader_metadata: Some(UploaderMetadata {
                version: "v1".to_string(),
                origin: "A test".to_string(),
                upload_time: None,
                variant: "A variant".to_string(),
            }),
        };
        let mut buf = Vec::new();
        prost::Message::encode(&test_report, &mut buf).unwrap();
        std::fs::write(&full_bin_path, buf).unwrap();

        let internal_bundled_file = BundledFile {
            original_path: full_bin_path.to_str().unwrap().to_string(),
            original_path_rel: None,
            path: bin_path,
            ..Default::default()
        };
        (internal_bundled_file, test_report)
    }

    fn create_bundle_meta(internal_bundled_file: Option<BundledFile>) -> BundleMeta {
        let mut repo = BundleRepo::default();
        let upload_time_epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_else(|_| std::time::Duration::from_secs(0))
            .as_secs();
        repo.repo.owner = "org".to_string();
        repo.repo.name = "repo".to_string();
        let mut envs: HashMap<String, String> = HashMap::new();
        envs.insert("key".to_string(), "value".to_string());
        BundleMeta {
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
                    "".to_string(),
                )],
                os_info: Some(env::consts::OS.to_string()),
                codeowners: None,
                envs,
                use_uncloned_repo: None,
            },
            internal_bundled_file,
            failed_tests: vec![Test::new(
                None,
                "name".to_string(),
                "parent_name".to_string(),
                Some("class_name".to_string()),
                None,
                "org".to_string(),
                &repo.repo,
                None,
                "".to_string(),
            )],
        }
    }

    #[tokio::test]
    pub async fn test_bundle_meta_is_first_entry() {
        let temp_dir = tempdir().unwrap();
        let bundle_path = temp_dir.path().join(BUNDLE_FILE_NAME);
        let meta = create_bundle_meta(None);
        BundlerUtil::new(&meta, None)
            .make_tarball(&bundle_path)
            .unwrap();

        let parsed_meta = parse_meta_from_tarball(io::BufReader::new(
            fs::File::open(&bundle_path).await.unwrap(),
        ))
        .await
        .unwrap();
        assert_eq!(parsed_meta, VersionedBundle::V0_7_8(meta));
    }

    #[tokio::test]
    pub async fn test_internal_bin_is_second_entry() {
        let temp_dir = tempdir().unwrap();
        let bundle_path = temp_dir.path().join(BUNDLE_FILE_NAME);
        let (internal_bundled_file, test_report) = create_internal_bundled_file(&temp_dir, None);
        let meta = create_bundle_meta(Some(internal_bundled_file));
        BundlerUtil::new(&meta, None)
            .make_tarball(&bundle_path)
            .unwrap();

        let entries = archive_entries(io::BufReader::new(
            fs::File::open(&bundle_path).await.unwrap(),
        ))
        .unwrap();

        let mut entry = entries.skip(1).next().await.unwrap().unwrap();

        let internal_bin = parse_internal_bin_entry(&mut entry, None)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(internal_bin, test_report);
    }

    #[tokio::test]
    pub async fn test_internal_bin_is_backwards_compatible_with_last_entry() {
        let temp_dir = tempdir().unwrap();
        let bundled_file_path = temp_dir.path().join("some_junit_file.xml");
        std::fs::write(&bundled_file_path, "").unwrap();
        let bundle_path = temp_dir.path().join(BUNDLE_FILE_NAME);
        let (internal_bundled_file, test_report) = create_internal_bundled_file(&temp_dir, None);
        let mut meta = create_bundle_meta(Some(internal_bundled_file));
        meta.base_props.file_sets.push(FileSet::new(
            FileSetType::Junit,
            vec![BundledFile {
                original_path: bundled_file_path.to_str().unwrap().to_string(),
                original_path_rel: None,
                path: "internal/0".to_string(),
                ..Default::default()
            }],
            "internal/0".to_string(),
            None,
        ));
        BundlerUtil::new(&meta, None)
            .make_tarball(&bundle_path)
            .unwrap();

        let mut entries = archive_entries(io::BufReader::new(
            fs::File::open(&bundle_path).await.unwrap(),
        ))
        .unwrap();

        let meta_entry = entries.next().await.unwrap().unwrap();
        let internal_bin_entry = entries.next().await.unwrap().unwrap();
        let bundled_file_entry = entries.next().await.unwrap().unwrap();
        assert!(entries.next().await.is_none());
        let entries_with_internal_bin_last = [meta_entry, bundled_file_entry, internal_bin_entry];

        let new_bundle_path = temp_dir.path().join("reordered_bundle.tar.zstd");

        {
            let tar_file = std::fs::File::create(&new_bundle_path).unwrap();
            let zstd_encoder = zstd::Encoder::new(tar_file, 15).unwrap();
            let mut tar_builder = tar::Builder::new(zstd_encoder);

            for mut entry in entries_with_internal_bin_last {
                let path = entry.path().unwrap().to_path_buf();
                let mut content = Vec::new();
                ReadExt::read_to_end(&mut entry, &mut content)
                    .await
                    .unwrap();
                tar_builder
                    .append_data(&mut tar::Header::new_gnu(), path, &content[..])
                    .unwrap();
            }

            tar_builder.into_inner().unwrap().finish().unwrap();
        }

        let internal_bin_and_meta = parse_internal_bin_and_meta_from_tarball(io::BufReader::new(
            fs::File::open(&bundle_path).await.unwrap(),
        ))
        .await
        .unwrap();
        assert_eq!(
            internal_bin_and_meta,
            (test_report, VersionedBundle::V0_7_8(meta))
        );
    }

    #[tokio::test]
    pub async fn test_nondefault_internal_bin_path() {
        let temp_dir = tempdir().unwrap();
        let bundle_path = temp_dir.path().join(BUNDLE_FILE_NAME);
        let (internal_bundled_file, test_report) =
            create_internal_bundled_file(&temp_dir, Some("new_bin_file.bin".to_string()));
        let meta = create_bundle_meta(Some(internal_bundled_file));
        BundlerUtil::new(&meta, None)
            .make_tarball(&bundle_path)
            .unwrap();

        let internal_bin_and_meta = parse_internal_bin_and_meta_from_tarball(io::BufReader::new(
            fs::File::open(&bundle_path).await.unwrap(),
        ))
        .await
        .unwrap();
        assert_eq!(
            internal_bin_and_meta,
            (test_report, VersionedBundle::V0_7_8(meta))
        );
    }

    #[test]
    pub fn test_no_duplicate_internal_file() {
        const INTERNAL_FILE_PATH: &str = "internal/0";

        let temp_dir = tempdir().unwrap();

        let bundled_file_path = temp_dir.path().join("some_junit_file.xml");
        std::fs::write(&bundled_file_path, "").unwrap();

        let bundle_path = temp_dir.path().join(BUNDLE_FILE_NAME);
        let mut meta = create_bundle_meta(None);
        meta.base_props.file_sets.push(FileSet::new(
            FileSetType::Junit,
            vec![BundledFile {
                original_path: bundled_file_path.to_str().unwrap().to_string(),
                original_path_rel: None,
                path: INTERNAL_FILE_PATH.to_string(),
                ..Default::default()
            }],
            INTERNAL_FILE_PATH.to_string(),
            None,
        ));
        BundlerUtil::new(&meta, None)
            .make_tarball(&bundle_path)
            .unwrap();

        let zstd_decoder = zstd::Decoder::new(std::fs::File::open(&bundle_path).unwrap()).unwrap();
        let mut archive = tar::Archive::new(zstd_decoder);
        let entries = archive.entries().unwrap();

        let internal_0_count = entries
            .filter_map(|entry| {
                let entry = entry.unwrap();
                let path = entry.header().path().unwrap();
                dbg!(&path.to_str().unwrap());
                if path.to_str().unwrap() == INTERNAL_FILE_PATH {
                    Some(())
                } else {
                    None
                }
            })
            .count();

        assert_eq!(
            internal_0_count, 1,
            "Expected 'internal/0' to appear exactly once in the tarball, but it appeared {internal_0_count} times"
        );
    }

    fn read_bundle_bytes(temp_dir: &TempDir, meta: &BundleMeta) -> Vec<u8> {
        let bundle_path = temp_dir.path().join(BUNDLE_FILE_NAME);
        BundlerUtil::new(meta, None)
            .make_tarball(&bundle_path)
            .unwrap();
        std::fs::read(&bundle_path).unwrap()
    }

    #[test]
    fn test_sync_parse_roundtrip() {
        let temp_dir = tempdir().unwrap();
        let (internal_bundled_file, test_report) = create_internal_bundled_file(&temp_dir, None);
        let meta = create_bundle_meta(Some(internal_bundled_file));
        let bytes = read_bundle_bytes(&temp_dir, &meta);

        let (report, bundle) = parse_internal_bin_and_meta_from_bytes(&bytes).unwrap();
        assert_eq!(report, test_report);
        assert_eq!(bundle, VersionedBundle::V0_7_8(meta));
    }

    #[test]
    fn test_sync_parse_nondefault_bin_path() {
        let temp_dir = tempdir().unwrap();
        let (internal_bundled_file, test_report) =
            create_internal_bundled_file(&temp_dir, Some("new_bin_file.bin".to_string()));
        let meta = create_bundle_meta(Some(internal_bundled_file));
        let bytes = read_bundle_bytes(&temp_dir, &meta);

        let (report, bundle) = parse_internal_bin_and_meta_from_bytes(&bytes).unwrap();
        assert_eq!(report, test_report);
        assert_eq!(bundle, VersionedBundle::V0_7_8(meta));
    }

    #[test]
    fn test_sync_parse_missing_internal_bin() {
        let temp_dir = tempdir().unwrap();
        let meta = create_bundle_meta(None);
        let bytes = read_bundle_bytes(&temp_dir, &meta);

        let result = parse_internal_bin_and_meta_from_bytes(&bytes);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("No internal.bin file found")
        );
    }

    #[test]
    fn test_traces_are_packed_under_traces_prefix() {
        use std::io::Read;

        use crate::traces::{
            DiscoveredTrace, TRACES_PREFIX, compute_trace_identity_hash, trace_archive_name,
        };

        let temp_dir = tempdir().unwrap();
        let bundle_path = temp_dir.path().join(BUNDLE_FILE_NAME);
        let (internal_bundled_file, _test_report) = create_internal_bundled_file(&temp_dir, None);
        let meta = create_bundle_meta(Some(internal_bundled_file));

        // Two distinct traces and one duplicate identity hash that should
        // collapse to a single tarball entry.
        let trace_a_path = temp_dir.path().join("a-trace.zip");
        std::fs::write(&trace_a_path, b"trace-a-bytes").unwrap();
        let trace_b_path = temp_dir.path().join("b-trace.zip");
        std::fs::write(&trace_b_path, b"trace-b-bytes").unwrap();

        let hash_a = compute_trace_identity_hash(
            Some("file_a.ts"),
            Some("ClassA"),
            "suite",
            "test_a",
            "variant",
        );
        let hash_b = compute_trace_identity_hash(
            Some("file_b.ts"),
            Some("ClassB"),
            "suite",
            "test_b",
            "variant",
        );
        assert_ne!(hash_a, hash_b);

        let traces = vec![
            DiscoveredTrace {
                identity_hash: hash_a.clone(),
                source_path: trace_a_path.clone(),
            },
            DiscoveredTrace {
                identity_hash: hash_b.clone(),
                source_path: trace_b_path.clone(),
            },
            // Duplicate: should be deduped by the bundler.
            DiscoveredTrace {
                identity_hash: hash_a.clone(),
                source_path: trace_a_path.clone(),
            },
        ];

        BundlerUtil::new(&meta, None)
            .with_traces(traces)
            .make_tarball(&bundle_path)
            .unwrap();

        // Read back the tarball and look for trace entries.
        let zstd_decoder = zstd::Decoder::new(std::fs::File::open(&bundle_path).unwrap()).unwrap();
        let mut archive = tar::Archive::new(zstd_decoder);
        let mut found: std::collections::HashMap<String, Vec<u8>> =
            std::collections::HashMap::new();
        for entry in archive.entries().unwrap() {
            let mut entry = entry.unwrap();
            let path = entry.path().unwrap().to_string_lossy().to_string();
            if path.starts_with(TRACES_PREFIX) {
                let mut buf = Vec::new();
                entry.read_to_end(&mut buf).unwrap();
                found.insert(path, buf);
            }
        }

        assert_eq!(
            found.len(),
            2,
            "expected exactly two unique trace entries after dedupe"
        );
        assert_eq!(
            found
                .get(&trace_archive_name(&hash_a))
                .map(|v| v.as_slice()),
            Some(b"trace-a-bytes".as_slice())
        );
        assert_eq!(
            found
                .get(&trace_archive_name(&hash_b))
                .map(|v| v.as_slice()),
            Some(b"trace-b-bytes".as_slice())
        );
    }

    #[test]
    fn test_missing_trace_file_is_skipped_not_fatal() {
        use crate::traces::{DiscoveredTrace, compute_trace_identity_hash, trace_archive_name};

        let temp_dir = tempdir().unwrap();
        let bundle_path = temp_dir.path().join(BUNDLE_FILE_NAME);
        let (internal_bundled_file, _test_report) = create_internal_bundled_file(&temp_dir, None);
        let meta = create_bundle_meta(Some(internal_bundled_file));

        let hash = compute_trace_identity_hash(None, None, "suite", "test", "");
        let missing_path = temp_dir.path().join("does-not-exist.zip");

        BundlerUtil::new(&meta, None)
            .with_traces(vec![DiscoveredTrace {
                identity_hash: hash.clone(),
                source_path: missing_path,
            }])
            .make_tarball(&bundle_path)
            .expect("missing trace file must not fail the bundle");

        let zstd_decoder = zstd::Decoder::new(std::fs::File::open(&bundle_path).unwrap()).unwrap();
        let mut archive = tar::Archive::new(zstd_decoder);
        let archive_name = trace_archive_name(&hash);
        let has_trace = archive
            .entries()
            .unwrap()
            .filter_map(Result::ok)
            .any(|entry| {
                entry
                    .path()
                    .ok()
                    .map(|p| p.to_string_lossy() == archive_name.as_str())
                    .unwrap_or(false)
            });
        assert!(!has_trace, "missing source file should not be packed");
    }

    #[test]
    fn test_sync_parse_missing_meta() {
        // Build a tarball that only contains an internal.bin entry (no meta.json)
        let temp_dir = tempdir().unwrap();
        let tarball_path = temp_dir.path().join("no_meta.tar.zstd");

        {
            let tar_file = std::fs::File::create(&tarball_path).unwrap();
            let zstd_enc = zstd::Encoder::new(tar_file, 15).unwrap();
            let mut tar_builder = tar::Builder::new(zstd_enc);

            let content = b"not a real protobuf";
            let mut header = tar::Header::new_gnu();
            header.set_size(content.len() as u64);
            header.set_cksum();
            tar_builder
                .append_data(&mut header, INTERNAL_BIN_FILENAME, &content[..])
                .unwrap();

            tar_builder.into_inner().unwrap().finish().unwrap();
        }

        let bytes = std::fs::read(&tarball_path).unwrap();
        let result = parse_internal_bin_and_meta_from_bytes(&bytes);
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("No meta.json file found in the tarball")
        );
    }

    #[test]
    fn test_sync_parse_empty_tarball() {
        // Build an empty tarball (no entries)
        let temp_dir = tempdir().unwrap();
        let tarball_path = temp_dir.path().join("empty.tar.zstd");

        {
            let tar_file = std::fs::File::create(&tarball_path).unwrap();
            let zstd_enc = zstd::Encoder::new(tar_file, 15).unwrap();
            let tar_builder = tar::Builder::new(zstd_enc);
            tar_builder.into_inner().unwrap().finish().unwrap();
        }

        let bytes = std::fs::read(&tarball_path).unwrap();
        let result = parse_internal_bin_and_meta_from_bytes(&bytes);
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("No meta.json file found in the tarball")
        );
    }
}
