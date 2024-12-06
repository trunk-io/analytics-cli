use std::{
    fs::File,
    path::{Path, PathBuf},
};

use constants::CODEOWNERS_LOCATIONS;
#[cfg(feature = "pyo3")]
use pyo3::prelude::*;
#[cfg(feature = "pyo3")]
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};
use serde::{Deserialize, Serialize};
#[cfg(feature = "wasm")]
use tsify_next::Tsify;
#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

#[cfg(feature = "pyo3")]
use crate::{github::BindingsGitHubOwners, gitlab::BindingsGitLabOwners};
use crate::{github::GitHubOwners, gitlab::GitLabOwners, traits::FromReader};

// TODO(TRUNK-13628): Implement serializing and deserializing for CodeOwners
#[derive(Default, Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "pyo3", gen_stub_pyclass, pyclass)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
pub struct CodeOwners {
    pub path: PathBuf,
    #[serde(skip_serializing, skip_deserializing)]
    pub owners: Option<Owners>,
}

impl CodeOwners {
    pub fn find_file<T: AsRef<Path>, U: AsRef<Path>>(
        repo_root: T,
        codeowners_path_cli_option: &Option<U>,
    ) -> Option<Self> {
        let cli_option_location = codeowners_path_cli_option
            .as_slice()
            .iter()
            .map(|path_str| -> &Path { path_str.as_ref() });
        let default_locations = CODEOWNERS_LOCATIONS
            .iter()
            .map(|path_str| -> &Path { path_str.as_ref() });
        let mut all_locations = cli_option_location.chain(default_locations);

        let codeowners_path =
            all_locations.find_map(|location| locate_codeowners(&repo_root, location));

        codeowners_path.map(|path| {
            let owners_result = File::open(&path)
                .map_err(anyhow::Error::from)
                .and_then(|file| GitHubOwners::from_reader(&file).map(Owners::GitHubOwners))
                .or_else(|_| {
                    File::open(&path)
                        .map_err(anyhow::Error::from)
                        .and_then(|file| GitLabOwners::from_reader(&file).map(Owners::GitLabOwners))
                });

            if let Err(ref err) = owners_result {
                log::error!(
                    "Found CODEOWNERS file `{}`, but couldn't parse it: {}",
                    path.to_string_lossy(),
                    err
                );
            }

            let owners = Result::ok(owners_result);
            Self {
                path: path.canonicalize().unwrap(),
                owners,
            }
        })
    }

    // TODO(TRUNK-13783): take in origin path and parse CODEOWNERS based on location
    // which informs which parser to use (GitHub or GitLab)
    pub fn parse(codeowners: Vec<u8>) -> Self {
        let owners_result = GitHubOwners::from_reader(codeowners.as_slice())
            .map(Owners::GitHubOwners)
            .or_else(|_| {
                GitLabOwners::from_reader(codeowners.as_slice()).map(Owners::GitLabOwners)
            });

        Self {
            path: PathBuf::new(),
            owners: owners_result.ok(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Owners {
    GitHubOwners(GitHubOwners),
    GitLabOwners(GitLabOwners),
}

// TODO(TRUNK-13784): Make this smarter and return only an object with a .of method
// instead of forcing the ETL to try GitHub or GitLab
#[derive(Debug, PartialEq, Eq)]
#[cfg_attr(feature = "pyo3", gen_stub_pyclass, pyclass)]
pub struct BindingsOwners(pub Owners);

#[cfg(feature = "pyo3")]
#[gen_stub_pymethods]
#[pymethods]
impl BindingsOwners {
    pub fn get_github_owners(&self) -> Option<BindingsGitHubOwners> {
        match &self.0 {
            Owners::GitHubOwners(owners) => {
                Some(BindingsGitHubOwners(GitHubOwners::from(owners.clone())))
            }
            _ => None,
        }
    }
    pub fn get_gitlab_owners(&self) -> Option<BindingsGitLabOwners> {
        match &self.0 {
            Owners::GitLabOwners(owners) => {
                Some(BindingsGitLabOwners(GitLabOwners::from(owners.clone())))
            }
            _ => None,
        }
    }
}

const CODEOWNERS: &str = "CODEOWNERS";

fn locate_codeowners<T, U>(repo_root: T, location: U) -> Option<PathBuf>
where
    T: AsRef<Path>,
    U: AsRef<Path>,
{
    let file = repo_root.as_ref().join(location).join(CODEOWNERS);
    if file.is_file() {
        Some(file)
    } else {
        None
    }
}
