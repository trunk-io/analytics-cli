use std::path::PathBuf;

use anyhow::Context;
use gix::Repository;
use regex::Regex;
use serde::{Deserialize, Serialize};

pub mod validator;

pub const GIT_REMOTE_ORIGIN_URL_CONFIG: &str = "remote.origin.url";

#[derive(Debug, Clone, Default)]
struct BundleRepoOptions {
    repo_root: Option<PathBuf>,
    repo_url: Option<String>,
    repo_head_sha: Option<String>,
    repo_head_branch: Option<String>,
    repo_head_commit_epoch: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BundleRepo {
    pub repo: RepoUrlParts,
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
    pub fn new(
        repo_root: Option<String>,
        repo_url: Option<String>,
        repo_head_sha: Option<String>,
        repo_head_branch: Option<String>,
        repo_head_commit_epoch: Option<String>,
    ) -> anyhow::Result<BundleRepo> {
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

        let mut head_commit_message = None;
        let mut head_commit_author = None;

        // If repo root found, try to get repo details from git.
        if let Some(git_repo) = bundle_repo_options
            .repo_root
            .as_ref()
            .and_then(|dir| gix::open(dir).ok())
        {
            bundle_repo_options.repo_url = bundle_repo_options.repo_url.or_else(|| {
                git_repo
                    .config_snapshot()
                    .string_by_key(GIT_REMOTE_ORIGIN_URL_CONFIG)
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

                bundle_repo_options.repo_head_sha = bundle_repo_options
                    .repo_head_sha
                    .or_else(|| git_head.id().map(|id| id.to_string()));

                if let Ok(commit) = git_head.peel_to_commit_in_place() {
                    bundle_repo_options.repo_head_commit_epoch = bundle_repo_options
                        .repo_head_commit_epoch
                        .or_else(|| commit.time().ok().map(|time| time.seconds));
                    head_commit_message = commit.message().map(|msg| msg.title.to_string()).ok();
                    head_commit_author = commit.author().ok().map(|signature| signature.to_owned());
                }
            }
        }

        // Require URL which should be known at this point.
        let repo_url = bundle_repo_options
            .repo_url
            .context("failed to get repo URL")?;
        let repo_url_parts =
            RepoUrlParts::from_url(&repo_url).context("failed to parse repo URL")?;
        let (repo_head_author_name, repo_head_author_email) = head_commit_author
            .as_ref()
            .map(|a| (a.name.to_string(), a.email.to_string()))
            .unwrap_or_default();
        Ok(BundleRepo {
            repo: repo_url_parts,
            repo_root: bundle_repo_options
                .repo_root
                .and_then(|p| p.to_str().map(String::from))
                .unwrap_or_default(),
            repo_url,
            repo_head_branch: bundle_repo_options.repo_head_branch.unwrap_or_default(),
            repo_head_sha: bundle_repo_options.repo_head_sha.unwrap_or_default(),
            repo_head_commit_epoch: bundle_repo_options
                .repo_head_commit_epoch
                .unwrap_or_default(),
            repo_head_commit_message: head_commit_message.unwrap_or_default(),
            repo_head_author_name,
            repo_head_author_email,
        })
    }

    fn git_head_branch_from_remote_branches(
        git_repo: &Repository,
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
}

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
}
