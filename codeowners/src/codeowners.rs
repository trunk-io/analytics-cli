use std::{
    collections::HashMap,
    fs::File,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, RwLock},
    thread,
};

use constants::CODEOWNERS_LOCATIONS;
#[cfg(feature = "pyo3")]
use pyo3::prelude::*;
#[cfg(feature = "pyo3")]
// use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods, gen_stub_pyclass_enum};
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};
use serde::{Deserialize, Serialize};
#[cfg(feature = "wasm")]
use tsify_next::Tsify;
#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

#[cfg(feature = "pyo3")]
use crate::{github::BindingsGitHubOwners, gitlab::BindingsGitLabOwners};
use crate::{
    github::GitHubOwners,
    gitlab::GitLabOwners,
    traits::{FromReader, OwnersOfPath},
};

pub type BundleUploadIDAndCodeOwnersBytes = (String, Option<Vec<u8>>);

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
                .and_then(|file| GitLabOwners::from_reader(&file).map(Owners::GitLabOwners))
                .or_else(|_| {
                    File::open(&path)
                        .map_err(anyhow::Error::from)
                        .and_then(|file| GitHubOwners::from_reader(&file).map(Owners::GitHubOwners))
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
        let owners_result = GitLabOwners::from_reader(codeowners.as_slice())
            .map(Owners::GitLabOwners)
            .or_else(|_| {
                GitHubOwners::from_reader(codeowners.as_slice()).map(Owners::GitHubOwners)
            });

        Self {
            path: PathBuf::new(),
            owners: owners_result.ok(),
        }
    }

    pub fn parse_many_multithreaded(
        to_parse: Vec<BundleUploadIDAndCodeOwnersBytes>,
        num_threads: usize,
    ) -> HashMap<String, Option<Self>> {
        let chunk_size = (to_parse.len() + num_threads - 1) / num_threads;
        let mut handles = Vec::with_capacity(num_threads);
        let results_map: Arc<Mutex<HashMap<String, Option<Self>>>> =
            Arc::new(Mutex::new(HashMap::new()));

        for chunk in to_parse.chunks(chunk_size) {
            let chunk = chunk.to_vec();
            let results_map = Arc::clone(&results_map);
            let handle = thread::spawn(move || {
                for (bundle_upload_id, codeowners_bytes) in chunk.into_iter() {
                    let codeowners = codeowners_bytes.map(Self::parse);
                    let mut results_map = results_map.lock().unwrap();
                    results_map.insert(bundle_upload_id, codeowners);
                }
            });

            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        Arc::try_unwrap(results_map).unwrap().into_inner().unwrap()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
// #[cfg_attr(feature = "pyo3", gen_stub_pyclass_enum, pyclass(eq))]
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

pub fn associate_codeowners_multithreaded(
    codeowners_matchers: HashMap<String, Option<Owners>>,
    to_associate: Vec<(String, Option<String>)>,
    num_threads: usize,
) -> Vec<Vec<String>> {
    let input_len = to_associate.len();
    let chunk_size = (input_len + num_threads - 1) / num_threads;
    let mut handles = Vec::with_capacity(num_threads);
    let codeowners_matchers = Arc::new(RwLock::new(codeowners_matchers));
    let all_associated_owners: Arc<Mutex<Vec<Option<Vec<String>>>>> =
        Arc::new(Mutex::new(vec![None; input_len]));

    for i in 0..num_threads {
        let to_associate = to_associate.clone();
        let codeowners_matchers = Arc::clone(&codeowners_matchers);
        let all_associated_owners = Arc::clone(&all_associated_owners);
        let start = i * chunk_size;
        let end = ((i + 1) * chunk_size).min(input_len);
        let handle = thread::spawn(move || {
            let codeowners_matchers = codeowners_matchers.read().unwrap();
            for j in start..end {
                let (bundle_upload_id, file) = &to_associate[j];
                let codeowners_matcher = codeowners_matchers.get(bundle_upload_id);
                let associated_owners: Vec<String> = match (codeowners_matcher, &file) {
                    (Some(Some(owners)), Some(file)) => match owners {
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
                    },
                    _ => Vec::new(),
                };
                let mut all_associated_owners = all_associated_owners.lock().unwrap();
                all_associated_owners[j] = Some(associated_owners);
            }
        });

        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }

    Arc::try_unwrap(all_associated_owners)
        .unwrap()
        .into_inner()
        .unwrap()
        .into_iter()
        .flatten()
        .collect()
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

    fn make_codeowners_bytes(i: usize) -> Vec<u8> {
        format!("{i}.txt @user{i}").into_bytes()
    }

    #[test]
    pub fn test_multithreaded_parsing_and_association() {
        let num_codeowners_files = 100;
        let num_files_to_associate_owners = 1000;
        let num_threads = 4;

        let codeowners_files: Vec<BundleUploadIDAndCodeOwnersBytes> = (0..num_codeowners_files)
            .map(|i| (i.to_string(), Some(make_codeowners_bytes(i))))
            .collect();
        let to_associate: Vec<(String, Option<String>)> = (0..num_files_to_associate_owners)
            .map(|i| {
                let mut file = "foo".to_string();
                if i % 2 == 0 {
                    let file_prefix = i % num_codeowners_files;
                    file = format!("{file_prefix}.txt");
                }
                ((i % num_codeowners_files).to_string(), Some(file))
            })
            .collect();

        let codeowners_matchers =
            CodeOwners::parse_many_multithreaded(codeowners_files, num_threads)
                .into_iter()
                .map(|(bundle_upload_id, codeowners)| {
                    (
                        bundle_upload_id,
                        codeowners.and_then(|codeowners| codeowners.owners),
                    )
                })
                .collect();
        let owners = crate::associate_codeowners_multithreaded(
            codeowners_matchers,
            to_associate,
            num_threads,
        );

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
}
