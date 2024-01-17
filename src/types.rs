use serde::Serialize;

use crate::scanner::{BundleRepo, FileSet};

#[derive(Debug, Serialize, Clone)]
pub struct BundleUploadLocation {
    pub url: String,
    pub key: String,
}

#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
pub struct Repo {
    pub host: String,
    pub owner: String,
    pub name: String,
}

impl Repo {
    pub fn from_url(url: &str) -> anyhow::Result<Self> {
        let parts = if url.starts_with("https://") {
            let url_without_scheme = url.trim_start_matches("https://");
            let parts = url_without_scheme.split('/').collect::<Vec<&str>>();
            if parts.len() != 3 {
                return Err(anyhow::anyhow!(
                    "Invalid repo url format. Expected exactly 3 parts: {:?} (url: {})",
                    parts,
                    url
                ));
            }
            let domain = parts[0];
            let owner = parts[1];
            let name = parts[2].trim_end_matches(".git");
            vec![domain, owner, name]
        } else if url.starts_with("git@") {
            // Example: "git@github.com:owner/repo"
            let domain_and_path = url.split('@').collect::<Vec<&str>>()[1];
            let parts = domain_and_path.split(':').collect::<Vec<&str>>();
            if parts.len() != 2 {
                return Err(anyhow::anyhow!(
                    "Invalid repo url format. Expected exactly 2 parts: {:?} (url: {})",
                    parts,
                    url
                ));
            }
            let domain = parts[0];
            let path = parts[1];
            let path_parts = path.split('/').collect::<Vec<&str>>();
            let owner = path_parts[0];
            let name = path_parts[1];
            vec![domain, owner, name]
        } else {
            return Err(anyhow::anyhow!(
                "Invalid repo url format. Expected https:// or git@: {}",
                url
            ));
        };

        let host = parts[0].trim().to_string();
        let owner = parts[1].trim().to_string();
        let name = parts[2].trim().trim_end_matches(".git").to_string();

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
}

/// Custom tags defined by the user.
///
#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
pub struct CustomTag {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct BundleMeta {
    pub org: String,
    pub repo: BundleRepo,
    pub tags: Vec<CustomTag>,
    pub file_sets: Vec<FileSet>,
    pub envs: std::collections::HashMap<String, String>,
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

    // TODO(TRUNK-10142): add more tests for URL parsing and git integration.
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
