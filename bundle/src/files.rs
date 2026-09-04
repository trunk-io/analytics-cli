use std::{
    collections::HashSet,
    fmt::Debug,
    format,
    path::{Path, PathBuf},
    time::SystemTime,
};

use chrono::{DateTime, Utc, serde::ts_milliseconds};
use codeowners::{CodeOwners, Owners, OwnersOfPath, OwnersSource};
use constants::ALLOW_LIST;
use context::junit::junit_path::{
    JunitReportFileWithTestRunnerReport, TestRunnerReport, TestRunnerReportStatus,
};
use glob::glob;
#[cfg(feature = "pyo3")]
use pyo3::prelude::*;
#[cfg(feature = "pyo3")]
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pyclass_enum, gen_stub_pymethods};
use regex::Regex;
use serde::{Deserialize, Serialize};
#[cfg(feature = "wasm")]
use tsify_next::Tsify;
#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

/// A repo root and the file paths resolved against it, kept canonical so that both
/// sides of a `starts_with` or `strip_prefix` share a prefix.
#[derive(Debug, Clone)]
struct RepoRoot(PathBuf);

impl RepoRoot {
    fn canonical<T: AsRef<str>>(repo_root: T) -> Self {
        let repo_root = repo_root.as_ref();
        Self(std::fs::canonicalize(repo_root).unwrap_or_else(|_| PathBuf::from(repo_root)))
    }

    fn as_str(&self) -> std::borrow::Cow<'_, str> {
        self.0.to_string_lossy()
    }

    fn canonicalize<T: AsRef<Path>>(&self, path: T) -> PathBuf {
        let path = path.as_ref();
        std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn contains<T: AsRef<Path>>(&self, path: T) -> bool {
        path.as_ref().starts_with(&self.0)
    }

    /// The first path lying within the repo, so a file linked in from outside keeps
    /// the route that reached it and stays repo-relative.
    fn within_or(&self, path: PathBuf, fallback: PathBuf) -> PathBuf {
        if self.contains(&path) { path } else { fallback }
    }
}

#[derive(Debug, Default, Clone)]
pub struct FileSetBuilder {
    count: usize,
    file_sets: Vec<FileSet>,
    codeowners: Option<CodeOwners>,
}

impl FileSetBuilder {
    pub fn build_file_sets<T: AsRef<str>, U: AsRef<Path>>(
        repo_root: T,
        junit_paths: &[JunitReportFileWithTestRunnerReport],
        codeowners_path: &Option<U>,
        codeowners_type: Option<OwnersSource>,
        exec_start: Option<SystemTime>,
    ) -> anyhow::Result<Self> {
        let repo_root = repo_root.as_ref();

        let codeowners = CodeOwners::find_file(repo_root, codeowners_path, codeowners_type);

        let file_set_builder =
            Self::file_sets_from_glob(repo_root, junit_paths, codeowners, exec_start)?;

        // Handle case when paths are not globs.
        if file_set_builder.count == 0 {
            let junit_paths_with_glob = junit_paths
                .iter()
                .cloned()
                .flat_map(|junit_wrapper| {
                    let mut junit_wrapper_xml = junit_wrapper.clone();
                    junit_wrapper_xml.junit_path = PathBuf::from(junit_wrapper_xml.junit_path)
                        .join("**/*.xml")
                        .to_string_lossy()
                        .to_string();
                    let mut junit_wrapper_internal = junit_wrapper.clone();
                    junit_wrapper_internal.junit_path =
                        PathBuf::from(junit_wrapper_internal.junit_path)
                            .join("**/*.bin")
                            .to_string_lossy()
                            .to_string();
                    vec![junit_wrapper_xml, junit_wrapper_internal]
                })
                .collect::<Vec<_>>();

            return Self::file_sets_from_glob(
                repo_root,
                junit_paths_with_glob.as_slice(),
                file_set_builder.codeowners,
                exec_start,
            );
        }

        Ok(file_set_builder)
    }

    fn file_sets_from_glob(
        repo_root: &str,
        junit_paths: &[JunitReportFileWithTestRunnerReport],
        codeowners: Option<CodeOwners>,
        exec_start: Option<SystemTime>,
    ) -> anyhow::Result<Self> {
        let repo_root = RepoRoot::canonical(repo_root);
        let files_per_glob = Self::collect_files_per_glob(&repo_root, junit_paths)?;

        let (count, file_sets) = junit_paths.iter().zip(files_per_glob).try_fold(
            (0, Vec::with_capacity(junit_paths.len())),
            |(index, mut file_sets), (junit_wrapper, paths)| -> anyhow::Result<_> {
                let (index, files) = Self::bundle_files(
                    &paths,
                    index,
                    &repo_root,
                    junit_wrapper,
                    &codeowners,
                    exec_start,
                )?;
                file_sets.push(FileSet::new(
                    Self::file_set_type(&files),
                    files,
                    junit_wrapper.junit_path.clone(),
                    junit_wrapper.test_runner_report.clone(),
                ));
                Ok((index, file_sets))
            },
        )?;

        Ok(Self {
            count,
            file_sets,
            codeowners,
        })
    }

    /// Expands every glob and returns the files each one owns, keyed by canonical
    /// path so a file is bundled once no matter how many globs reach it or how many
    /// routes through symlinked directories lead to it. A glob whose every match was
    /// already claimed owns nothing, which keeps its (now empty) file set in place.
    fn collect_files_per_glob(
        repo_root: &RepoRoot,
        junit_paths: &[JunitReportFileWithTestRunnerReport],
    ) -> anyhow::Result<Vec<Vec<PathBuf>>> {
        let mut claimed: HashSet<PathBuf> = HashSet::new();

        junit_paths
            .iter()
            .map(|junit_wrapper| {
                let matches = Self::scan_from_glob(&junit_wrapper.junit_path, repo_root.as_str())?;
                let matched = matches.len();

                let mut owned: Vec<PathBuf> = matches
                    .into_iter()
                    .filter_map(|path| {
                        let canonical = repo_root.canonicalize(&path);
                        if !claimed.insert(canonical.clone()) {
                            return None;
                        }
                        Some(repo_root.within_or(canonical, path))
                    })
                    .collect();
                owned.sort();

                if owned.len() < matched {
                    tracing::warn!(
                        "glob {:?} matched {} paths resolving to {} files not already \
                         collected; {} duplicate routes were dropped",
                        junit_wrapper.junit_path,
                        matched,
                        owned.len(),
                        matched - owned.len(),
                    );
                }

                Ok(owned)
            })
            .collect()
    }

    fn bundle_files(
        paths: &[PathBuf],
        start_index: usize,
        repo_root: &RepoRoot,
        junit_wrapper: &JunitReportFileWithTestRunnerReport,
        codeowners: &Option<CodeOwners>,
        exec_start: Option<SystemTime>,
    ) -> anyhow::Result<(usize, Vec<BundledFile>)> {
        paths.iter().try_fold(
            (start_index, Vec::with_capacity(paths.len())),
            |(mut index, mut files), path| -> anyhow::Result<_> {
                if let Some(bundled_file) = BundledFile::from_path(
                    path.as_path(),
                    index,
                    repo_root.path(),
                    &junit_wrapper.junit_path,
                    codeowners,
                    exec_start,
                )? {
                    index += 1;
                    files.push(bundled_file);
                }
                Ok((index, files))
            },
        )
    }

    fn file_set_type(files: &[BundledFile]) -> FileSetType {
        files
            .iter()
            .find_map(|file| {
                if file.original_path.ends_with(".bin") {
                    Some(FileSetType::Internal)
                } else {
                    None
                }
            })
            .unwrap_or(FileSetType::Junit)
    }

    pub fn count(&self) -> usize {
        self.count
    }

    pub fn file_sets(&self) -> &[FileSet] {
        &self.file_sets
    }

    pub fn codeowners(&self) -> &Option<CodeOwners> {
        &self.codeowners
    }

    pub fn take_codeowners(&mut self) -> Option<CodeOwners> {
        self.codeowners.take()
    }

    pub fn no_files_found(&self) -> bool {
        self.count() == 0 || self.file_sets().is_empty()
    }

    fn scan_from_glob<T: AsRef<str>, U: AsRef<str>>(
        glob_path: T,
        repo_root: U,
    ) -> anyhow::Result<Vec<PathBuf>> {
        let glob_path = PathBuf::from(glob_path.as_ref());
        let path_to_scan = if glob_path.is_absolute() {
            glob_path
        } else {
            Path::new(repo_root.as_ref()).join(glob_path)
        };

        let paths = glob(&path_to_scan.to_string_lossy())?
            .filter_map(|entry| entry.ok().filter(|path| path.is_file()))
            .collect();

        Ok(paths)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "pyo3", gen_stub_pyclass, pyclass(get_all))]
#[cfg_attr(feature = "wasm", derive(Tsify))]
pub struct FileSet {
    pub file_set_type: FileSetType,
    pub files: Vec<BundledFile>,
    pub glob: String,
    #[serde(flatten)]
    pub test_runner_report: Option<FileSetTestRunnerReport>,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "pyo3", gen_stub_pyclass, pyclass(get_all))]
#[cfg_attr(feature = "wasm", derive(Tsify), tsify(into_wasm_abi, from_wasm_abi))]
pub struct FileSetTestRunnerReport {
    /// Added in v0.6.11. Populated when parsing from BEP, not from junit globs
    pub resolved_status: TestRunnerReportStatus,
    /// Added in v0.9.2. Populated when parsing from BEP, not from junit globs
    #[cfg_attr(feature = "wasm", tsify(type = "number"))]
    #[serde(default, with = "ts_milliseconds")]
    pub resolved_start_time_epoch_ms: DateTime<Utc>,
    /// Added in v0.9.2. Populated when parsing from BEP, not from junit globs
    #[cfg_attr(feature = "wasm", tsify(type = "number"))]
    #[serde(default, with = "ts_milliseconds")]
    pub resolved_end_time_epoch_ms: DateTime<Utc>,
    /// Deprecated. Use `bazel_run_information.label` on test case runs and
    /// `bazel_build_information.label` on test results in `internal.bin` instead.
    /// Retained for backward-compatible `meta.json` deserialization only; not written on new bundles.
    #[deprecated(
        since = "0.13.2",
        note = "use bazel_run_information.label or bazel_build_information.label from internal.bin"
    )]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_label: Option<String>,
}

#[cfg(feature = "pyo3")]
#[gen_stub_pymethods]
#[pymethods]
impl FileSetTestRunnerReport {
    #[cfg(feature = "pyo3")]
    #[new]
    pub fn new(
        resolved_status: TestRunnerReportStatus,
        resolved_start_time_epoch_ms: DateTime<Utc>,
        resolved_end_time_epoch_ms: DateTime<Utc>,
        #[allow(deprecated)] resolved_label: Option<String>,
    ) -> Self {
        Self {
            resolved_status,
            resolved_start_time_epoch_ms,
            resolved_end_time_epoch_ms,
            resolved_label,
        }
    }
}

impl From<TestRunnerReport> for FileSetTestRunnerReport {
    fn from(test_runner_report: TestRunnerReport) -> Self {
        Self {
            resolved_status: test_runner_report.status,
            resolved_start_time_epoch_ms: test_runner_report.start_time,
            resolved_end_time_epoch_ms: test_runner_report.end_time,
            resolved_label: None,
        }
    }
}

impl From<FileSetTestRunnerReport> for TestRunnerReport {
    fn from(test_runner_report: FileSetTestRunnerReport) -> Self {
        Self {
            status: test_runner_report.resolved_status,
            start_time: test_runner_report.resolved_start_time_epoch_ms,
            end_time: test_runner_report.resolved_end_time_epoch_ms,
        }
    }
}

impl FileSet {
    pub fn new(
        file_set_type: FileSetType,
        files: Vec<BundledFile>,
        glob: String,
        test_runner_report: Option<TestRunnerReport>,
    ) -> Self {
        Self {
            file_set_type,
            files,
            glob,
            test_runner_report: test_runner_report.map(FileSetTestRunnerReport::from),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "pyo3", gen_stub_pyclass_enum, pyclass(eq, eq_int))]
#[cfg_attr(feature = "wasm", derive(Tsify))]
pub enum FileSetType {
    #[default]
    Junit,
    Internal,
}

#[cfg(feature = "wasm")]
// u128 will be supported in the next release after 0.2.95
#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "pyo3", gen_stub_pyclass, pyclass(get_all))]
#[cfg_attr(feature = "wasm", derive(Tsify))]
pub struct BundledFile {
    pub original_path: String,
    /// Added in v0.5.33
    pub original_path_rel: Option<String>,
    pub path: String,
    pub owners: Vec<String>,
    pub team: Option<String>,
}

#[cfg(not(feature = "wasm"))]
#[cfg_attr(feature = "pyo3", gen_stub_pyclass, pyclass(get_all))]
#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct BundledFile {
    pub original_path: String,
    /// Added in v0.5.33
    pub original_path_rel: Option<String>,
    pub path: String,
    // deserialize u128 from flatten not supported
    // https://github.com/serde-rs/json/issues/625
    #[serde(skip_deserializing)]
    pub last_modified_epoch_ns: u128,
    pub owners: Vec<String>,
    pub team: Option<String>,
}

impl BundledFile {
    pub fn from_path<T: AsRef<Path>, U: Debug>(
        path: &Path,
        file_index: usize,
        repo_root: T,
        glob_path: U,
        codeowners: &Option<CodeOwners>,
        start: Option<SystemTime>,
    ) -> anyhow::Result<Option<Self>> {
        let original_path_abs = path
            .to_str()
            .ok_or_else(|| anyhow::Error::msg("failed to convert path to string"))?
            .to_string();
        let original_path_rel = path
            .strip_prefix(repo_root)
            .unwrap_or(path)
            .to_str()
            .ok_or_else(|| anyhow::Error::msg("failed to convert path to string"))?
            .to_string();
        // Check if file is allowed.
        let mut is_allowed = false;
        for allow in ALLOW_LIST {
            let re = Regex::new(allow).unwrap();
            if re.is_match(&original_path_abs) {
                is_allowed = true;
                break;
            }
        }
        if !is_allowed {
            tracing::warn!("File {:?} from glob {:?} is not allowed", path, glob_path);
            return Ok(None);
        }

        // When start is provided, check if file is stale
        if let Some(start) = start {
            let modified = path.metadata()?.modified()?;
            if modified < start {
                tracing::warn!("File {:?} from glob {:?} is stale", path, glob_path);
                return Ok(None);
            }
        }

        // Get owners of file.
        // Use the repo-relative path for codeowners matching, not the absolute path.
        // CODEOWNERS patterns are relative to the repo root (e.g., `/src/components`),
        // so we must pass the relative path for correct matching.
        let owners = codeowners
            .as_ref()
            .and_then(|codeowners| codeowners.owners.as_ref())
            .and_then(|codeowners_owners| match codeowners_owners {
                Owners::GitHubOwners(gho) => gho
                    .of(&original_path_rel)
                    .map(|o| o.iter().map(ToString::to_string).collect::<Vec<String>>()),
                Owners::GitLabOwners(glo) => glo
                    .of(&original_path_rel)
                    .map(|o| o.iter().map(ToString::to_string).collect::<Vec<String>>()),
            })
            .unwrap_or_default();

        // Save file under junit/0, junit/1, etc.
        // This is to avoid having to deal with potential file name collisions.
        let path_formatted;
        if original_path_abs.ends_with(".xml") {
            // we currently support junit and internal binary files
            path_formatted = format!("junit/{}", file_index);
        } else if original_path_abs.ends_with(".bin") {
            path_formatted = format!("internal/{}", file_index);
        } else {
            return Ok(None);
        }
        Ok(Some(Self {
            original_path: original_path_abs,
            original_path_rel: Some(original_path_rel),
            path: path_formatted,
            #[cfg(not(feature = "wasm"))]
            last_modified_epoch_ns: path
                .metadata()?
                .modified()?
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos(),
            owners,
            // Added in v0.5.33 but unused
            // We are unable to remove for the time being
            team: None,
        }))
    }

    pub fn get_print_path(&self) -> &str {
        self.original_path_rel
            .as_ref()
            .unwrap_or(&self.original_path)
    }
}
