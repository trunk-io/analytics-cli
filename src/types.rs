use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::scanner::{BundleRepo, FileSet};

pub struct RunResult {
    pub exit_code: i32,
    pub failures: Vec<Test>,
}

#[derive(Debug, Serialize, Clone, Deserialize)]
pub struct CreateBundleUploadRequest {
    pub repo: Repo,
    #[serde(rename = "orgUrlSlug")]
    pub org_url_slug: String,
}

#[derive(Debug, Serialize, Clone, Deserialize)]
pub struct GetQuarantineBulkTestStatusRequest {
    pub repo: Repo,
    #[serde(rename = "orgUrlSlug")]
    pub org_url_slug: String,
    #[serde(rename = "testIdentifiers")]
    pub test_identifiers: Vec<Test>,
}

#[derive(Debug, Serialize, Clone, Deserialize)]
pub struct Test {
    pub name: String,
    #[serde(rename = "parentName")]
    pub parent_name: String,
    #[serde(rename = "className")]
    pub class_name: Option<String>,
    pub file: Option<String>,
}

#[derive(Debug, Serialize, Clone, Deserialize)]
pub struct QuarantineBulkTestStatus {
    #[serde(rename = "groupIsQuarantined")]
    pub group_is_quarantined: bool,
}

#[derive(Debug, Serialize, Clone, Deserialize)]
pub struct BundleUploadLocation {
    pub url: String,
    pub key: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Repo {
    pub host: String,
    pub owner: String,
    pub name: String,
}

impl Repo {
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

        Ok(Repo { host, owner, name })
    }
}

#[derive(Debug, Serialize, Clone)]
pub struct BundleUploader {
    pub org_slug: String,
}

#[derive(Debug, Serialize, Clone, Default)]
pub enum FileSetType {
    #[default]
    Junit,
}

#[derive(Debug, Serialize, Clone, Default)]
pub struct BundledFile {
    pub original_path: String,
    pub path: String,
    pub last_modified_epoch_ns: u128,
}

/// Custom tags defined by the user.
///
#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
pub struct CustomTag {
    pub key: String,
    pub value: String,
}

pub const META_VERSION: &str = "1";

#[derive(Debug, Serialize, Clone)]
pub struct BundleMeta {
    pub version: String,
    pub cli_version: String,
    pub org: String,
    pub repo: BundleRepo,
    pub tags: Vec<CustomTag>,
    pub file_sets: Vec<FileSet>,
    pub envs: std::collections::HashMap<String, String>,
    pub upload_time_epoch: u64,
    pub test_command: Option<String>,
    pub os_info: Option<String>,
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_parse_ssh_urls() {
        let good_urls = &[
            (
                "git@github.com:user/repository.git",
                Repo {
                    host: "github.com".to_string(),
                    owner: "user".to_string(),
                    name: "repository".to_string(),
                },
            ),
            (
                "git@gitlab.com:group/project.git",
                Repo {
                    host: "gitlab.com".to_string(),
                    owner: "group".to_string(),
                    name: "project".to_string(),
                },
            ),
            (
                "git@bitbucket.org:team/repo.git",
                Repo {
                    host: "bitbucket.org".to_string(),
                    owner: "team".to_string(),
                    name: "repo".to_string(),
                },
            ),
            (
                "git@ssh.dev.azure.com:company/project",
                Repo {
                    host: "ssh.dev.azure.com".to_string(),
                    owner: "company".to_string(),
                    name: "project".to_string(),
                },
            ),
            (
                "git@sourceforge.net:owner/repo.git",
                Repo {
                    host: "sourceforge.net".to_string(),
                    owner: "owner".to_string(),
                    name: "repo".to_string(),
                },
            ),
        ];

        for (url, expected) in good_urls {
            let actual = Repo::from_url(url).unwrap();
            assert_eq!(actual, *expected);
        }
    }

    #[test]
    fn test_parse_https_urls() {
        let good_urls = &[
            (
                "https://github.com/username/repository.git",
                Repo {
                    host: "github.com".to_string(),
                    owner: "username".to_string(),
                    name: "repository".to_string(),
                },
            ),
            (
                "https://gitlab.com/group/project.git",
                Repo {
                    host: "gitlab.com".to_string(),
                    owner: "group".to_string(),
                    name: "project".to_string(),
                },
            ),
            (
                "https://bitbucket.org/teamname/reponame.git",
                Repo {
                    host: "bitbucket.org".to_string(),
                    owner: "teamname".to_string(),
                    name: "reponame".to_string(),
                },
            ),
            (
                "https://dev.azure.com/organization/project",
                Repo {
                    host: "dev.azure.com".to_string(),
                    owner: "organization".to_string(),
                    name: "project".to_string(),
                },
            ),
            (
                "https://gitlab.example.edu/groupname/project.git",
                Repo {
                    host: "gitlab.example.edu".to_string(),
                    owner: "groupname".to_string(),
                    name: "project".to_string(),
                },
            ),
        ];

        for (url, expected) in good_urls {
            let actual = Repo::from_url(url).unwrap();
            assert_eq!(actual, *expected);
        }
    }

    #[test]
    fn test_parse_git_urls() {
        let good_urls = &[
            (
                "ssh://github.com/github/testrepo",
                Repo {
                    host: "github.com".to_string(),
                    owner: "github".to_string(),
                    name: "testrepo".to_string(),
                },
            ),
            (
                "git://github.com/github/testrepo",
                Repo {
                    host: "github.com".to_string(),
                    owner: "github".to_string(),
                    name: "testrepo".to_string(),
                },
            ),
            (
                "http://github.com/github/testrepo",
                Repo {
                    host: "github.com".to_string(),
                    owner: "github".to_string(),
                    name: "testrepo".to_string(),
                },
            ),
            (
                "https://github.com/github/testrepo",
                Repo {
                    host: "github.com".to_string(),
                    owner: "github".to_string(),
                    name: "testrepo".to_string(),
                },
            ),
            (
                "ftp://github.com/github/testrepo",
                Repo {
                    host: "github.com".to_string(),
                    owner: "github".to_string(),
                    name: "testrepo".to_string(),
                },
            ),
            (
                "ftps://github.com/github/testrepo",
                Repo {
                    host: "github.com".to_string(),
                    owner: "github".to_string(),
                    name: "testrepo".to_string(),
                },
            ),
            (
                "user@github.com:github/testrepo",
                Repo {
                    host: "github.com".to_string(),
                    owner: "github".to_string(),
                    name: "testrepo".to_string(),
                },
            ),
        ];

        let bad_urls = &[
            "sshy://github.com/github/testrepo",
            "ssh://github.com//testrepo",
            "ssh:/github.com//testrepo",
            "ssh:///testrepo",
            "ssh://github.com/github/",
        ];

        for (url, expected) in good_urls {
            let actual1 = Repo::from_url(url).unwrap();
            assert_eq!(actual1, *expected);
            let actual2 = Repo::from_url(&(url.to_string() + ".git")).unwrap();
            assert_eq!(actual2, *expected);
            let actual3 = Repo::from_url(&(url.to_string() + ".git/")).unwrap();
            assert_eq!(actual3, *expected);
        }

        for url in bad_urls {
            let actual = Repo::from_url(url);
            assert!(actual.is_err());
        }
    }

    #[test]
    pub fn test_parse_good_custom_tags() {
        let good_tags = &[
            (
                vec!["a=b".to_owned(), "1=2".to_owned()],
                vec![
                    CustomTag {
                        key: "a".to_string(),
                        value: "b".to_string(),
                    },
                    CustomTag {
                        key: "1".to_string(),
                        value: "2".to_string(),
                    },
                ],
            ),
            (
                vec![
                    "key1=value1".to_owned(),
                    "key2=value2".to_owned(),
                    "key3=value3".to_owned(),
                ],
                vec![
                    CustomTag {
                        key: "key1".to_string(),
                        value: "value1".to_string(),
                    },
                    CustomTag {
                        key: "key2".to_string(),
                        value: "value2".to_string(),
                    },
                    CustomTag {
                        key: "key3".to_string(),
                        value: "value3".to_string(),
                    },
                ],
            ),
            (
                vec![
                    "key1=value1".to_owned(),
                    "key2=value2".to_owned(),
                    "key3=value3".to_owned(),
                    "key4=value4".to_owned(),
                ],
                vec![
                    CustomTag {
                        key: "key1".to_string(),
                        value: "value1".to_string(),
                    },
                    CustomTag {
                        key: "key2".to_string(),
                        value: "value2".to_string(),
                    },
                    CustomTag {
                        key: "key3".to_string(),
                        value: "value3".to_string(),
                    },
                    CustomTag {
                        key: "key4".to_string(),
                        value: "value4".to_string(),
                    },
                ],
            ),
        ];

        for (tags_str, expected) in good_tags {
            let actual = crate::utils::parse_custom_tags(&tags_str).unwrap();
            assert_eq!(actual, *expected);
        }
    }

    #[test]
    pub fn test_parse_bad_custom_tags() {
        let bad_tags = vec![
            vec!["key1=".to_owned(), "key2=value2".to_owned()],
            vec!["=value".to_owned(), "key2=value2".to_owned()],
            vec!["  =  ".to_owned(), "key2=value2".to_owned()],
        ];

        for tags_str in bad_tags {
            let actual = crate::utils::parse_custom_tags(&tags_str);
            assert!(actual.is_err());
        }
    }
}
