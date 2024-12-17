use std::{format, time::SystemTime};

use codeowners::{CodeOwners, Owners, OwnersOfPath};
use constants::ALLOW_LIST;
use context::junit::junit_path::TestRunnerJunitStatus;
#[cfg(feature = "pyo3")]
use pyo3::prelude::*;
#[cfg(feature = "pyo3")]
use pyo3_stub_gen::derive::gen_stub_pyclass;
use regex::Regex;
use serde::{Deserialize, Serialize};
#[cfg(feature = "wasm")]
use tsify_next::Tsify;
#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

use crate::types::{BundledFile, FileSetType};

#[derive(Default, Debug)]
pub struct FileSetCounter {
    inner: usize,
}

impl FileSetCounter {
    pub fn count_file(&mut self) -> usize {
        let prev = self.inner;
        self.inner += 1;
        prev
    }

    pub fn get_count(&self) -> usize {
        self.inner
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "pyo3", gen_stub_pyclass, pyclass(get_all))]
#[cfg_attr(feature = "wasm", derive(Tsify))]
pub struct FileSet {
    pub file_set_type: FileSetType,
    pub files: Vec<BundledFile>,
    pub glob: String,
    pub test_runner_status: Option<TestRunnerJunitStatus>,
}

impl FileSet {
    /// Scan a file set from a glob path.
    /// And generates file set using file counter.
    ///
    pub fn scan_from_glob(
        repo_root: &str,
        glob_path: String,
        test_runner_status: Option<TestRunnerJunitStatus>,
        file_counter: &mut FileSetCounter,
        team: Option<String>,
        codeowners: &Option<CodeOwners>,
        start: Option<SystemTime>,
    ) -> anyhow::Result<FileSet> {
        let path_to_scan = if !std::path::Path::new(&glob_path).is_absolute() {
            std::path::Path::new(repo_root)
                .join(&glob_path)
                .to_str()
                .ok_or_else(|| anyhow::Error::msg("failed to convert path to string"))?
                .to_string()
        } else {
            glob_path.clone()
        };

        let mut files = Vec::new();

        glob::glob(&path_to_scan)?.try_for_each(|entry| {
            let path = match entry {
                Ok(path) => path,
                Err(e) => {
                    return Err(anyhow::anyhow!("Error scanning file set: {:?}", e));
                }
            };

            if !path.is_file() {
                return Ok::<(), anyhow::Error>(());
            }

            let original_path_abs = path
                .to_str()
                .ok_or_else(|| anyhow::Error::msg("failed to convert path to string"))?
                .to_string();
            let original_path_rel = path
                .strip_prefix(repo_root)
                .unwrap_or(&path)
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
                log::warn!("File {:?} from glob {:?} is not allowed", path, glob_path);
                return Ok::<(), anyhow::Error>(());
            }

            // When start is provided, check if file is stale
            if let Some(start) = start {
                let modified = path.metadata()?.modified()?;
                if modified < start {
                    log::warn!("File {:?} from glob {:?} is stale", path, glob_path);
                    return Ok::<(), anyhow::Error>(());
                }
            }

            // Get owners of file.
            let owners = codeowners
                .as_ref()
                .and_then(|codeowners| codeowners.owners.as_ref())
                .and_then(|codeowners_owners| match codeowners_owners {
                    Owners::GitHubOwners(gho) => gho
                        .of(path.as_path())
                        .map(|o| o.iter().map(ToString::to_string).collect::<Vec<String>>()),
                    Owners::GitLabOwners(glo) => glo
                        .of(path.as_path())
                        .map(|o| o.iter().map(ToString::to_string).collect::<Vec<String>>()),
                })
                .unwrap_or_default();

            // Save file under junit/0, junit/1, etc.
            // This is to avoid having to deal with potential file name collisions.
            files.push(BundledFile {
                original_path: original_path_abs,
                original_path_rel: Some(original_path_rel),
                path: format!("junit/{}", file_counter.count_file()),
                #[cfg(not(feature = "wasm"))]
                last_modified_epoch_ns: path
                    .metadata()?
                    .modified()?
                    .duration_since(std::time::UNIX_EPOCH)?
                    .as_nanos(),
                owners,
                team: team.clone(),
            });

            Ok(())
        })?;

        Ok(FileSet {
            file_set_type: FileSetType::Junit,
            files,
            glob: glob_path,
            test_runner_status,
        })
    }
}
