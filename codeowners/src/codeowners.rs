use std::{
    fs::File,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::Result;
use constants::CODEOWNERS_LOCATIONS;
#[cfg(feature = "pyo3")]
use pyo3::prelude::*;
#[cfg(feature = "pyo3")]
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};
use serde::{Deserialize, Serialize};
use tokio::task;
#[cfg(feature = "wasm")]
use tsify_next::Tsify;
#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

#[cfg(feature = "pyo3")]
use crate::{github::BindingsGitHubOwners, gitlab::BindingsGitLabOwners};
use crate::{
    github::{GitHubOwner, GitHubOwners},
    gitlab::{GitLabOwner, GitLabOwners},
    traits::{FromReader, OwnersOfPath},
};

// TODO(TRUNK-13628): Implement serializing and deserializing for CodeOwners
#[derive(Default, Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "pyo3", gen_stub_pyclass, pyclass)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
pub struct CodeOwners {
    pub path: PathBuf,
    #[serde(skip_serializing, skip_deserializing)]
    pub owners: Option<Owners>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "pyo3", pyclass(eq, eq_int))]
#[cfg_attr(feature = "wasm", derive(Tsify))]
pub enum OwnersSource {
    Unknown,
    GitHub,
    GitLab,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "pyo3", gen_stub_pyclass, pyclass)]
pub struct CodeOwnersFile {
    pub bytes: Vec<u8>,
    pub owners_source: OwnersSource,
}

pub fn flatten_code_owners(codeowners: &CodeOwners, file: &String) -> Vec<String> {
    match codeowners.owners.as_ref() {
        Some(Owners::GitHubOwners(owners)) => {
            let owners = owners.of(file).unwrap_or_default();
            return owners
                .iter()
                .map(|github_owner| match github_owner {
                    GitHubOwner::Username(name)
                    | GitHubOwner::Team(name)
                    | GitHubOwner::Email(name) => name.clone(),
                })
                .collect();
        }
        Some(Owners::GitLabOwners(owners)) => {
            let owners = owners.of(file).unwrap_or_default();
            return owners
                .iter()
                .map(|gitlab_owner| match gitlab_owner {
                    GitLabOwner::Name(name) | GitLabOwner::Email(name) => name.clone(),
                })
                .collect();
        }
        None => {
            vec![]
        }
    }
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
                .and_then(|file| GitLabOwners::from_reader(&file).map(Owners::GitLabOwners))
                .or_else(|_| {
                    File::open(&path)
                        .map_err(anyhow::Error::from)
                        .and_then(|file| GitHubOwners::from_reader(&file).map(Owners::GitHubOwners))
                });

            if let Err(ref err) = owners_result {
                tracing::warn!(
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
    // used by etl
    pub fn parse(codeowners: Vec<u8>, owners_source: &OwnersSource) -> Self {
        let owners_result = match owners_source {
            OwnersSource::GitHub => {
                GitHubOwners::from_reader(codeowners.as_slice()).map(Owners::GitHubOwners)
            }
            OwnersSource::GitLab => {
                GitLabOwners::from_reader(codeowners.as_slice()).map(Owners::GitLabOwners)
            }
            OwnersSource::Unknown => GitLabOwners::from_reader(codeowners.as_slice())
                .map(Owners::GitLabOwners)
                .or_else(|_| {
                    GitHubOwners::from_reader(codeowners.as_slice()).map(Owners::GitHubOwners)
                }),
        };
        Self {
            path: PathBuf::new(),
            owners: owners_result.ok(),
        }
    }

    pub async fn parse_many_multithreaded(to_parse: Vec<CodeOwnersFile>) -> Result<Vec<Self>> {
        let tasks = to_parse
            .into_iter()
            .enumerate()
            .map(
                |(
                    i,
                    CodeOwnersFile {
                        bytes,
                        owners_source,
                    },
                )| {
                    task::spawn(async move { (i, Self::parse(bytes, &owners_source)) })
                },
            )
            .collect::<Vec<_>>();

        let mut results = vec![None; tasks.len()];
        for task in tasks {
            let (i, result) = task.await?;
            results[i] = Some(result);
        }

        Ok(results.into_iter().flatten().collect())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Owners {
    GitHubOwners(GitHubOwners),
    GitLabOwners(GitLabOwners),
}

// TODO(TRUNK-13784): Make this smarter and return only an object with a .of method
// instead of forcing the ETL to try GitHub or GitLab
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "pyo3", gen_stub_pyclass, pyclass)]
pub struct BindingsOwners(pub Owners);

#[cfg(feature = "pyo3")]
#[gen_stub_pymethods]
#[pymethods]
impl BindingsOwners {
    pub fn get_github_owners(&self) -> Option<BindingsGitHubOwners> {
        match &self.0 {
            Owners::GitHubOwners(owners) => Some(BindingsGitHubOwners(owners.clone())),
            _ => None,
        }
    }
    pub fn get_gitlab_owners(&self) -> Option<BindingsGitLabOwners> {
        match &self.0 {
            Owners::GitLabOwners(owners) => Some(BindingsGitLabOwners(owners.clone())),
            _ => None,
        }
    }
}

pub fn associate_codeowners<T: AsRef<Path>>(owners: &Owners, file: T) -> Vec<String> {
    match owners {
        Owners::GitHubOwners(gho) => gho
            .of(file)
            .unwrap_or_default()
            .iter()
            .map(ToString::to_string)
            .collect(),
        Owners::GitLabOwners(glo) => glo
            .of(file)
            .unwrap_or_default()
            .iter()
            .map(ToString::to_string)
            .collect(),
    }
}

pub async fn associate_codeowners_multithreaded<T: AsRef<Path> + Send + Sync + 'static>(
    to_associate: Vec<(Arc<Owners>, T)>,
) -> Result<Vec<Vec<String>>> {
    let tasks = to_associate
        .into_iter()
        .enumerate()
        .map(|(i, (owners, file))| {
            task::spawn(async move { (i, associate_codeowners(owners.as_ref(), file)) })
        })
        .collect::<Vec<_>>();

    let mut results = vec![None; tasks.len()];
    for task in tasks {
        let (i, result) = task.await?;
        results[i] = Some(result);
    }

    Ok(results.into_iter().flatten().collect())
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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_codeowners_bytes(i: usize, owners_source: Option<OwnersSource>) -> CodeOwnersFile {
        CodeOwnersFile {
            bytes: format!("{i}.txt @user{i}").into_bytes(),
            owners_source: owners_source.unwrap_or(OwnersSource::Unknown),
        }
    }

    #[tokio::test]
    pub async fn test_multithreaded_parsing_and_association_default() {
        let num_codeowners_files = 100;
        let num_files_to_associate_owners = 1000;

        let codeowners_files: Vec<CodeOwnersFile> = (0..num_codeowners_files)
            .map(|i| make_codeowners_bytes(i, None))
            .collect();

        let codeowners_matchers = CodeOwners::parse_many_multithreaded(codeowners_files)
            .await
            .unwrap();

        let to_associate: Vec<(Arc<Owners>, String)> = (0..num_files_to_associate_owners)
            .map(|i| {
                let mut file = "unassociated".to_string();
                if i % 2 == 0 {
                    let file_prefix = i % num_codeowners_files;
                    file = format!("{file_prefix}.txt");
                }
                (
                    Arc::new(
                        codeowners_matchers[i % num_codeowners_files]
                            .owners
                            .clone()
                            .unwrap(),
                    ),
                    file,
                )
            })
            .collect();

        let owners = crate::associate_codeowners_multithreaded(to_associate)
            .await
            .unwrap();

        assert_eq!(owners.len(), num_files_to_associate_owners);
        for (i, owners) in owners.iter().enumerate() {
            if i % 2 == 0 {
                assert_eq!(owners.len(), 1);
                let user_id = i % num_codeowners_files;
                assert_eq!(owners[0], format!("@user{user_id}"));
            } else {
                assert_eq!(owners.len(), 0);
            }
        }
    }

    #[tokio::test]
    pub async fn test_multithreaded_parsing_and_association_specified() {
        let num_codeowners_files = 100;
        let num_files_to_associate_owners = 1000;

        let codeowners_files = (0..num_codeowners_files)
            .map(|i| {
                // use syntax specific to each codeowners file type
                if i % 2 == 0 {
                    CodeOwnersFile {
                        bytes: format!("{i}.txt** @user{i}").into_bytes(),
                        owners_source: OwnersSource::GitHub,
                    }
                } else {
                    CodeOwnersFile {
                        bytes: format!("^[User{i}] @user{i}\n{i}.txt").into_bytes(),
                        owners_source: OwnersSource::GitLab,
                    }
                }
            })
            .collect();

        let codeowners_matchers = CodeOwners::parse_many_multithreaded(codeowners_files)
            .await
            .unwrap();

        let to_associate: Vec<(Arc<Owners>, String)> = (0..num_files_to_associate_owners)
            .map(|i| {
                let file_prefix = i % num_codeowners_files;
                let file = format!("{file_prefix}.txt");
                (
                    Arc::new(
                        codeowners_matchers[i % num_codeowners_files]
                            .owners
                            .clone()
                            .unwrap(),
                    ),
                    file,
                )
            })
            .collect();

        let owners = crate::associate_codeowners_multithreaded(to_associate)
            .await
            .unwrap();

        assert_eq!(owners.len(), num_files_to_associate_owners);
        for (i, owners) in owners.iter().enumerate() {
            assert_eq!(owners.len(), 1);
            let user_id = i % num_codeowners_files;
            assert_eq!(owners[0], format!("@user{user_id}"));
        }
    }
}
