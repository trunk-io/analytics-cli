use std::{
    fmt::Debug,
    format,
    path::{Path, PathBuf},
    time::SystemTime,
};

use codeowners::{CodeOwners, Owners, OwnersOfPath};
use constants::ALLOW_LIST;
use context::junit::junit_path::{JunitReportFileWithStatus, JunitReportStatus};
use glob::glob;
#[cfg(feature = "pyo3")]
use pyo3::prelude::*;
#[cfg(feature = "pyo3")]
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pyclass_enum};
use regex::Regex;
use serde::{Deserialize, Serialize};
#[cfg(feature = "wasm")]
use tsify_next::Tsify;
#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

#[derive(Debug, Default, Clone)]
pub struct FileSetBuilder {
    count: usize,
    file_sets: Vec<FileSet>,
    codeowners: Option<CodeOwners>,
}

impl FileSetBuilder {
    pub fn build_file_sets<T: AsRef<str>, U: AsRef<Path>>(
        repo_root: T,
        junit_paths: &[JunitReportFileWithStatus],
        team: &Option<String>,
        codeowners_path: &Option<U>,
        exec_start: Option<SystemTime>,
    ) -> anyhow::Result<Self> {
        let repo_root = repo_root.as_ref();

        let codeowners = CodeOwners::find_file(repo_root, codeowners_path);

        let file_set_builder =
            Self::file_sets_from_glob(repo_root, junit_paths, team, codeowners, exec_start)?;

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
                team,
                file_set_builder.codeowners,
                exec_start,
            );
        }

        Ok(file_set_builder)
    }

    fn file_sets_from_glob(
        repo_root: &str,
        junit_paths: &[JunitReportFileWithStatus],
        team: &Option<String>,
        codeowners: Option<CodeOwners>,
        exec_start: Option<SystemTime>,
    ) -> anyhow::Result<Self> {
        junit_paths.iter().try_fold(
            Self {
                codeowners,
                ..Self::default()
            },
            |mut acc, junit_wrapper| -> anyhow::Result<Self> {
                let files = Self::scan_from_glob(&junit_wrapper.junit_path, repo_root)?;
                let codeowners = &acc.codeowners;
                let (count, bundled_files) = files.iter().try_fold(
                    (acc.count, Vec::new()),
                    |mut acc, file| -> anyhow::Result<(usize, Vec<BundledFile>)> {
                        if let Some(bundled_file) = BundledFile::from_path(
                            file.as_path(),
                            acc.0,
                            repo_root,
                            &junit_wrapper.junit_path,
                            team.clone(),
                            codeowners,
                            exec_start,
                        )? {
                            acc.0 += 1;
                            acc.1.push(bundled_file);
                        }
                        Ok(acc)
                    },
                )?;
                // If any file is a binary file, set the file set type to internal.
                let file_set_type = bundled_files.iter().fold(FileSetType::Junit, |acc, file| {
                    if acc == FileSetType::Internal {
                        return acc;
                    }
                    if file.original_path.ends_with(".bin") {
                        return FileSetType::Internal;
                    }
                    acc
                });
                acc.count = count;
                acc.file_sets.push(FileSet::new(
                    bundled_files,
                    junit_wrapper.junit_path.clone(),
                    junit_wrapper.status.clone(),
                    file_set_type,
                ));
                Ok(acc)
            },
        )
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
    /// Added in v0.6.11. Populated when parsing from BEP, not from junit globs
    pub resolved_status: Option<JunitReportStatus>,
}

impl FileSet {
    pub fn new(
        files: Vec<BundledFile>,
        glob: String,
        resolved_status: Option<JunitReportStatus>,
        file_set_type: FileSetType,
    ) -> Self {
        Self {
            file_set_type,
            files,
            glob,
            resolved_status,
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
        team: Option<String>,
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
        let owners = codeowners
            .as_ref()
            .and_then(|codeowners| codeowners.owners.as_ref())
            .and_then(|codeowners_owners| match codeowners_owners {
                Owners::GitHubOwners(gho) => gho
                    .of(path)
                    .map(|o| o.iter().map(ToString::to_string).collect::<Vec<String>>()),
                Owners::GitLabOwners(glo) => glo
                    .of(path)
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
            team,
        }))
    }

    pub fn get_print_path(&self) -> &str {
        self.original_path_rel
            .as_ref()
            .unwrap_or(&self.original_path)
    }
}
