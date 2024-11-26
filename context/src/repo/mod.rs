use anyhow::Context;
use lazy_static::lazy_static;
#[cfg(feature = "pyo3")]
use pyo3::prelude::*;
#[cfg(feature = "pyo3")]
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
#[cfg(feature = "git-access")]
use std::process::Command;
#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

pub mod validator;

pub const GIT_REMOTE_ORIGIN_URL_CONFIG: &str = "remote.origin.url";

lazy_static! {
    static ref GH_MERGE_BRANCH_REGEX: Regex =
        Regex::new(r"refs\/remotes\/pull\/[0-9]+\/merge").unwrap();
}

#[derive(Debug, Clone, Default)]
struct BundleRepoOptions {
    repo_root: Option<PathBuf>,
    repo_url: Option<String>,
    repo_head_sha: Option<String>,
    repo_head_branch: Option<String>,
    repo_head_commit_epoch: Option<i64>,
}

#[cfg_attr(feature = "pyo3", gen_stub_pyclass, pyclass(get_all))]
#[cfg_attr(feature = "wasm", wasm_bindgen(getter_with_clone))]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BundleRepo {
    pub repo: RepoUrlParts,
    pub repo_root: String,
    pub repo_url: String,
    pub repo_head_sha: String,
    /// Added in v0.5.34
    pub repo_head_sha_short: Option<String>,
    pub repo_head_branch: String,
    pub repo_head_commit_epoch: i64,
    pub repo_head_commit_message: String,
    pub repo_head_author_name: String,
    pub repo_head_author_email: String,
}

impl BundleRepo {
    const SHORT_SHA_LEN: usize = 7;

    pub fn new(
        repo_root: Option<String>,
        repo_url: Option<String>,
        repo_head_sha: Option<String>,
        repo_head_branch: Option<String>,
        repo_head_commit_epoch: Option<String>,
    ) -> anyhow::Result<BundleRepo> {
        #[allow(unused_mut)]
        let mut bundle_repo_options = BundleRepoOptions {
            repo_root: repo_root
                .as_ref()
                .map(|repo_root| PathBuf::from(repo_root))
                .or_else(|| std::env::current_dir().ok()),
            repo_url,
            repo_head_sha,
            repo_head_branch,
            repo_head_commit_epoch: repo_head_commit_epoch.and_then(|s| s.parse().ok()),
        };
        #[allow(unused_mut)]
        let mut head_commit_message = None;
        #[allow(unused_mut)]
        let mut head_commit_author = None;

        #[cfg(feature = "git-access")]
        {
            // If repo root found, try to get repo details from git.
            if let Some(git_repo) = bundle_repo_options
                .repo_root
                .as_ref()
                .and_then(|dir| gix::open(dir).ok())
            {
                bundle_repo_options.repo_url = bundle_repo_options.repo_url.or_else(|| {
                    git_repo
                        .config_snapshot()
                        .string(GIT_REMOTE_ORIGIN_URL_CONFIG)
                        .map(|s| s.to_string())
                });

                if let Ok(mut git_head) = git_repo.head() {
                    bundle_repo_options.repo_head_branch = bundle_repo_options
                        .repo_head_branch
                        .or_else(|| git_head.referent_name().map(|s| s.as_bstr().to_string()))
                        .or_else(|| {
                            Self::git_head_branch_from_remote_branches(&git_repo)
                                .ok()
                                .flatten()
                        });

                    if let Ok(mut commit) = git_head.peel_to_commit_in_place() {
                        commit = Self::resolve_repo_head_commit(
                            &git_repo,
                            commit,
                            bundle_repo_options
                                .repo_head_branch
                                .clone()
                                .unwrap_or_default(),
                        );

                        bundle_repo_options.repo_head_sha = bundle_repo_options
                            .repo_head_sha
                            .or_else(|| Some(commit.id().to_string()));
                        bundle_repo_options.repo_head_commit_epoch = bundle_repo_options
                            .repo_head_commit_epoch
                            .or_else(|| commit.time().ok().map(|time| time.seconds));
                        head_commit_message =
                            commit.message().map(|msg| msg.title.to_string()).ok();
                        head_commit_author = commit
                            .author()
                            .ok()
                            .map(|signature| signature.to_owned())
                            .map(|a| (a.name.to_string(), a.email.to_string()));
                    }
                }
            }
        }

        // Require URL which should be known at this point.
        let repo_url = bundle_repo_options
            .repo_url
            .context("failed to get repo URL")?;
        let repo_url_parts =
            RepoUrlParts::from_url(&repo_url).context("failed to parse repo URL")?;
        let (repo_head_author_name, repo_head_author_email) =
            head_commit_author.unwrap_or_default();
        let repo_head_sha = bundle_repo_options.repo_head_sha.unwrap_or_default();
        let repo_head_sha_short =
            &repo_head_sha[..std::cmp::min(Self::SHORT_SHA_LEN, repo_head_sha.len())];
        Ok(BundleRepo {
            repo: repo_url_parts,
            repo_root: bundle_repo_options
                .repo_root
                .and_then(|p| p.to_str().map(String::from))
                .unwrap_or_default(),
            repo_url,
            repo_head_branch: bundle_repo_options.repo_head_branch.unwrap_or_default(),
            repo_head_sha: repo_head_sha.clone(),
            repo_head_sha_short: Some(repo_head_sha_short.to_string()),
            repo_head_commit_epoch: bundle_repo_options
                .repo_head_commit_epoch
                .unwrap_or_default(),
            repo_head_commit_message: head_commit_message.unwrap_or_default(),
            repo_head_author_name,
            repo_head_author_email,
        })
    }

    #[cfg(feature = "git-access")]
    fn git_head_branch_from_remote_branches(
        git_repo: &gix::Repository,
    ) -> anyhow::Result<Option<String>> {
        for remote_branch in git_repo
            .references()?
            .remote_branches()?
            .filter_map(Result::ok)
        {
            if let Some(target_id) = remote_branch.target().try_id() {
                if target_id.as_bytes() == remote_branch.id().as_bytes() {
                    return Ok(remote_branch.name().to_path().to_str().map(String::from));
                }
            }
        }
        Ok(None)
    }

    #[cfg(feature = "git-access")]
    fn resolve_repo_head_commit<'a>(
        git_repo: &'a gix::Repository,
        current_commit: gix::Commit<'a>,
        repo_head_branch: String,
    ) -> gix::Commit<'a> {
        // for GH actions, grab PR branch HEAD commit, not the PR merge commit
        if GH_MERGE_BRANCH_REGEX.is_match(&repo_head_branch)
            && current_commit.parent_ids().count() == 2
        {
            log::info!("Detected merge commit");

            // attempt to grab PR commit if fetch --depth=2 was done upstream
            if let Some(pr_head_id) = current_commit.parent_ids().last() {
                if let Ok(pr_head_commit) = git_repo.find_commit(pr_head_id) {
                    log::info!(
                        "Found PR branch HEAD commit with SHA {}, using this as commit",
                        pr_head_commit.id().to_string()
                    );
                    return pr_head_commit;
                }
            }

            log::info!("PR branch HEAD commit not found, fetching remote with --depth=2...");
            let branch_to_fetch = repo_head_branch.replace("remotes/", "");
            match Command::new("git")
                .arg("fetch")
                .arg("--depth=2")
                .arg("origin")
                .arg(branch_to_fetch)
                .output()
            {
                Ok(fetch_output) => {
                    if !fetch_output.status.success() {
                        log::info!(
                            "Received unsuccessful status after fetch: {}. Defaulting to merge commit with SHA {}",
                            fetch_output.status,
                            current_commit.id().to_string(),
                        );
                        return current_commit;
                    }
                }
                Err(e) => {
                    log::info!(
                        "Encountered error during fetch: {}. Defaulting to merge commit with SHA {}",
                        e,
                        current_commit.id().to_string(),
                    );
                    return current_commit;
                }
            }

            if let Some(pr_head_id) = current_commit.parent_ids().last() {
                if let Ok(pr_head_commit) = git_repo.find_commit(pr_head_id) {
                    log::info!(
                        "Found PR branch HEAD commit with SHA {}, using this as commit",
                        pr_head_commit.id().to_string()
                    );
                    return pr_head_commit;
                }
            }

            log::info!(
                "PR branch HEAD commit not found. Defaulting to merge commit with SHA {}",
                current_commit.id().to_string()
            );
        }

        return current_commit;
    }
}

#[cfg(feature = "pyo3")]
#[gen_stub_pymethods]
#[pymethods]
impl BundleRepo {
    #[new]
    fn py_new(
        repo: RepoUrlParts,
        repo_root: String,
        repo_url: String,
        repo_head_sha: String,
        repo_head_sha_short: String,
        repo_head_branch: String,
        repo_head_commit_epoch: i64,
        repo_head_commit_message: String,
        repo_head_author_name: String,
        repo_head_author_email: String,
    ) -> Self {
        Self {
            repo,
            repo_root,
            repo_url,
            repo_head_sha,
            repo_head_sha_short: Some(repo_head_sha_short),
            repo_head_branch,
            repo_head_commit_epoch,
            repo_head_commit_message,
            repo_head_author_name,
            repo_head_author_email,
        }
    }
}

#[cfg(feature = "wasm")]
#[wasm_bindgen]
impl BundleRepo {
    #[wasm_bindgen(constructor)]
    pub fn js_new(
        repo: RepoUrlParts,
        repo_root: String,
        repo_url: String,
        repo_head_sha: String,
        repo_head_sha_short: String,
        repo_head_branch: String,
        repo_head_commit_epoch: i64,
        repo_head_commit_message: String,
        repo_head_author_name: String,
        repo_head_author_email: String,
    ) -> Self {
        Self {
            repo,
            repo_root,
            repo_url,
            repo_head_sha,
            repo_head_sha_short: Some(repo_head_sha_short),
            repo_head_branch,
            repo_head_commit_epoch,
            repo_head_commit_message,
            repo_head_author_name,
            repo_head_author_email,
        }
    }
}

/// The [`Repo` common type](https://github.com/trunk-io/trunk/blob/518397f/trunk/services/common/types/repo.ts#L10)
// NOTE: This is named `RepoUrlParts` to prevent confusion as to its purpose
#[cfg_attr(feature = "pyo3", gen_stub_pyclass, pyclass(get_all))]
#[cfg_attr(feature = "wasm", wasm_bindgen(getter_with_clone))]
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepoUrlParts {
    pub host: String,
    pub owner: String,
    pub name: String,
}

impl RepoUrlParts {
    pub fn from_url(url: &str) -> anyhow::Result<Self> {
        let re1 = Regex::new(r"^(ssh|git|http|https|ftp|ftps)://([^/]*?@)?([^/]*)/(.+)/([^/]+)")?;
        let re2 = Regex::new(r"^([^/]*?@)([^/]*):(.+)/([^/]+)")?;

        let parts = if re1.is_match(url) {
            let caps = re1.captures(url).expect("failed to parse url");
            if caps.len() != 6 {
                return Err(anyhow::anyhow!(
                    "Invalid repo url format. Expected 6 parts: {:?} (url: {})",
                    caps,
                    url
                ));
            }
            let domain = caps.get(3).map(|m| m.as_str()).unwrap_or("");
            let owner = caps.get(4).map(|m| m.as_str()).unwrap_or("");
            let name = caps.get(5).map(|m| m.as_str()).unwrap_or("");
            (domain, owner, name)
        } else if re2.is_match(url) {
            let caps = re2.captures(url).expect("failed to parse url");
            if caps.len() != 5 {
                return Err(anyhow::anyhow!(
                    "Invalid repo url format. Expected 6 parts: {:?} (url: {})",
                    caps,
                    url
                ));
            }
            let domain = caps.get(2).map(|m| m.as_str()).unwrap_or("");
            let owner = caps.get(3).map(|m| m.as_str()).unwrap_or("");
            let name = caps.get(4).map(|m| m.as_str()).unwrap_or("");
            (domain, owner, name)
        } else {
            return Err(anyhow::anyhow!("Invalid repo url format: {}", url));
        };

        let host = parts.0.trim().to_string();
        let owner = parts.1.trim().to_string();
        let name = parts
            .2
            .trim()
            .trim_end_matches('/')
            .trim_end_matches(".git")
            .to_string();

        if host.is_empty() || owner.is_empty() || name.is_empty() {
            return Err(anyhow::anyhow!(
                "Invalid repo url format. Expected non-empty parts: {:?} (url: {})",
                parts,
                url
            ));
        }

        Ok(Self { host, owner, name })
    }

    pub fn repo_full_name(&self) -> String {
        format!("{}/{}/{}", self.host, self.owner, self.name)
    }
}

#[cfg(feature = "pyo3")]
#[gen_stub_pymethods]
#[pymethods]
impl RepoUrlParts {
    #[new]
    fn py_new(host: String, owner: String, name: String) -> Self {
        Self { host, owner, name }
    }
}

#[cfg(feature = "wasm")]
#[wasm_bindgen]
impl RepoUrlParts {
    #[wasm_bindgen(constructor)]
    pub fn js_new(host: String, owner: String, name: String) -> Self {
        Self { host, owner, name }
    }
}
