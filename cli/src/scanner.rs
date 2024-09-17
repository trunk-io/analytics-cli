use std::format;
use std::time::SystemTime;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::codeowners::CodeOwners;
use crate::constants::{ALLOW_LIST, ENVS_TO_GET};
use crate::types::{BundledFile, FileSetType, Repo};
use crate::utils::from_non_empty_or_default;

pub const GIT_REMOTE_ORIGIN_URL_CONFIG: &str = "remote.origin.url";

#[derive(Debug)]
struct HeadAuthor {
    pub name: String,
    pub email: String,
}

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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FileSet {
    pub file_set_type: FileSetType,
    pub files: Vec<BundledFile>,
    pub glob: String,
}

impl FileSet {
    /// Scan a file set from a glob path.
    /// And generates file set using file counter.
    ///
    pub fn scan_from_glob(
        repo_root: &str,
        glob_path: String,
        file_counter: &mut FileSetCounter,
        team: Option<String>,
        codeowners: &Option<CodeOwners>,
        start: Option<SystemTime>,
    ) -> anyhow::Result<FileSet> {
        let path_to_scan = if !std::path::Path::new(&glob_path).is_absolute() {
            std::path::Path::new(repo_root)
                .join(&glob_path)
                .to_str()
                .expect("failed to convert path to string")
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

            let original_path = path
                .to_str()
                .expect("failed to convert path to string")
                .to_string();

            // Check if file is allowed.
            let mut is_allowed = false;
            for allow in ALLOW_LIST {
                let re = Regex::new(allow).unwrap();
                if re.is_match(&original_path) {
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
                .and_then(|codeowners_owners| codeowners_owners.of(path.as_path()))
                .map(|codeowners_owners_of_path| {
                    codeowners_owners_of_path
                        .iter()
                        .map(|owner| owner.to_string())
                        .collect::<Vec<String>>()
                })
                .unwrap_or_default();

            // Save file under junit/0, junit/1, etc.
            // This is to avoid having to deal with potential file name collisions.
            files.push(BundledFile {
                original_path,
                path: format!("junit/{}", file_counter.count_file()),
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
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BundleRepo {
    pub repo: Repo,
    pub repo_root: String,
    pub repo_url: String,
    pub repo_head_sha: String,
    pub repo_head_branch: String,
    pub repo_head_commit_epoch: i64,
    pub repo_head_commit_message: String,
    pub repo_head_author_name: String,
    pub repo_head_author_email: String,
}

impl BundleRepo {
    /// Read important fields from git repo root.
    ///
    pub fn try_read_from_root(
        in_repo_root: Option<String>,
        in_repo_url: Option<String>,
        in_repo_head_sha: Option<String>,
        in_repo_head_branch: Option<String>,
        in_repo_head_commit_epoch: Option<String>,
    ) -> anyhow::Result<BundleRepo> {
        let mut out_repo_url = in_repo_url.clone();
        let mut out_repo_head_sha = in_repo_head_sha.clone();
        let mut out_repo_head_branch = in_repo_head_branch.clone();
        let mut out_repo_head_commit_epoch =
            from_non_empty_or_default(in_repo_head_commit_epoch, None, |s| {
                Some(
                    s.parse::<i64>()
                        .expect("failed to parse commit epoch override"),
                )
            });
        let out_repo_root =
            from_non_empty_or_default(in_repo_root, Self::default_to_working_directory(), Some);

        let mut git_head_author = None;
        let mut git_head_commit_message = None;
        // If repo root found, try to get repo details from git.
        if let Some(repo_root) = &out_repo_root {
            // Read git repo.
            log::info!("Reading git repo at {:?}", &repo_root);

            let git_repo = gix::open(repo_root)?;
            let git_url = git_repo
                .config_snapshot()
                .string_by_key(GIT_REMOTE_ORIGIN_URL_CONFIG)
                .map(|s| s.to_string());
            let mut git_head = git_repo.head()?;

            let mut git_head_branch = git_head.referent_name().map(|s| s.as_bstr().to_string());
            if git_head_branch.is_none() {
                for r in git_repo.references()?.remote_branches()? {
                    match r {
                        Ok(r) => {
                            let target = r.target();
                            let id = target.try_id();
                            if id.is_some()
                                && git_head.id().is_some()
                                && id.unwrap().to_string() == git_head.id().unwrap().to_string()
                            {
                                git_head_branch =
                                    r.name().to_path().to_str().map(|s| s.to_string());
                                break;
                            };
                        }
                        Err(e) => {
                            log::debug!("Unexpected error when trying to find reference {:?}", e);
                        }
                    }
                }
            }
            let git_head_sha = git_head.id().map(|id| id.to_string());
            let git_head_commit_time = git_head.peel_to_commit_in_place()?.time()?;
            git_head_commit_message = git_head.peel_to_commit_in_place().map_or(None, |commit| {
                commit
                    .message()
                    .map_or(None, |msg| Some(msg.title.to_string()))
            });
            git_head_author = git_head
                .peel_to_commit_in_place()
                .map(|commit| {
                    if let Ok(author) = commit.author() {
                        Some(HeadAuthor {
                            name: author.name.to_string(),
                            email: author.email.to_string(),
                        })
                    } else {
                        None
                    }
                })
                .ok()
                .flatten();
            log::info!("Found git_url: {:?}", git_url);
            log::info!("Found git_sha: {:?}", git_head_sha);
            log::info!("Found git_branch: {:?}", git_head_branch);
            log::info!("Found git_commit_time: {:?}", git_head_commit_time);
            log::info!("Found git_commit_message: {:?}", git_head_commit_message);
            log::info!("Found git_author: {:?}", git_head_author);

            out_repo_url = from_non_empty_or_default(in_repo_url, git_url, Some);
            out_repo_head_sha = from_non_empty_or_default(in_repo_head_sha, git_head_sha, Some);
            out_repo_head_branch =
                from_non_empty_or_default(in_repo_head_branch, git_head_branch, Some);
            if out_repo_head_commit_epoch.is_none() {
                out_repo_head_commit_epoch = Some(git_head_commit_time.seconds);
            }
        }

        // Require URL which should be known at this point.
        let repo_url = out_repo_url.expect("failed to get repo url");
        let repo = Repo::from_url(&repo_url)?;
        let (git_head_author_name, git_head_author_email) = if let Some(author) = git_head_author {
            (author.name, author.email)
        } else {
            (String::default(), String::default())
        };

        Ok(BundleRepo {
            repo,
            repo_root: out_repo_root.unwrap_or("".to_string()),
            repo_url,
            repo_head_branch: out_repo_head_branch.unwrap_or_default(),
            repo_head_sha: out_repo_head_sha.expect("failed to get repo head sha"),
            repo_head_commit_epoch: out_repo_head_commit_epoch
                .expect("failed to get repo head commit time"),
            repo_head_commit_message: git_head_commit_message.unwrap_or("".to_string()),
            repo_head_author_name: git_head_author_name,
            repo_head_author_email: git_head_author_email,
        })
    }

    fn default_to_working_directory() -> Option<String> {
        std::env::current_dir()
            .expect("failed to resolve working directory")
            .to_str()
            .map(|s| s.to_string())
    }
}

pub struct EnvScanner;

impl EnvScanner {
    pub fn scan_env() -> std::collections::HashMap<String, String> {
        let mut envs = std::collections::HashMap::with_capacity(ENVS_TO_GET.len());
        for env in ENVS_TO_GET {
            if let Ok(val) = std::env::var(env) {
                envs.insert(env.to_string(), val);
            }
        }
        envs
    }
}
