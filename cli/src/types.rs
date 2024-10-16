use context::repo::BundleRepo;
use serde::{Deserialize, Serialize};

use crate::codeowners::CodeOwners;
use crate::scanner::FileSet;

pub struct RunResult {
    pub exit_code: i32,
    pub failures: Vec<Test>,
    pub exec_start: Option<std::time::SystemTime>,
}

pub struct QuarantineRunResult {
    pub exit_code: i32,
    pub quarantine_status: QuarantineBulkTestStatus,
}

#[derive(Debug, Serialize, Clone, Deserialize)]
pub struct Test {
    pub name: String,
    #[serde(rename = "parentName")]
    pub parent_name: String,
    #[serde(rename = "className")]
    pub class_name: Option<String>,
    pub file: Option<String>,
    pub id: String,
}

impl Test {
    pub fn new(
        name: String,
        parent_name: String,
        class_name: Option<String>,
        file: Option<String>,
        id: Option<String>,
        org_slug: &str,
        repo: &BundleRepo,
    ) -> Self {
        if let Some(id) = id {
            return Test {
                parent_name,
                name,
                class_name,
                file,
                id,
            };
        }
        // generate a unique id if not provided
        let repo_full_name = format!("{}/{}/{}", repo.repo.host, repo.repo.owner, repo.repo.name);
        let info_id_input = [
            org_slug,
            &repo_full_name,
            file.as_deref().unwrap_or(""),
            class_name.as_deref().unwrap_or(""),
            &parent_name,
            &name,
            "JUNIT_TESTCASE",
        ]
        .join("#");
        let id =
            uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_URL, info_id_input.as_bytes()).to_string();
        Test {
            parent_name,
            name,
            class_name,
            file,
            id,
        }
    }
}

#[derive(Debug, Serialize, Clone, Deserialize, Default)]
pub struct QuarantineBulkTestStatus {
    #[serde(rename = "groupIsQuarantined")]
    pub group_is_quarantined: bool,
    #[serde(rename = "quarantineResults")]
    pub quarantine_results: Vec<Test>,
}

#[derive(Debug, Serialize, Clone)]
pub struct BundleUploader {
    pub org_slug: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub enum FileSetType {
    #[default]
    Junit,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct BundledFile {
    pub original_path: String,
    pub original_path_rel: String,
    pub path: String,
    pub last_modified_epoch_ns: u128,
    pub owners: Vec<String>,
    pub team: Option<String>,
}

/// Custom tags defined by the user.
///
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct CustomTag {
    pub key: String,
    pub value: String,
}

pub const META_VERSION: &str = "1";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BundleMeta {
    pub version: String,
    pub cli_version: String,
    pub org: String,
    pub repo: BundleRepo,
    pub bundle_upload_id: String,
    pub tags: Vec<CustomTag>,
    pub file_sets: Vec<FileSet>,
    pub envs: std::collections::HashMap<String, String>,
    pub upload_time_epoch: u64,
    pub test_command: Option<String>,
    pub os_info: Option<String>,
    pub quarantined_tests: Vec<Test>,
    pub codeowners: Option<CodeOwners>,
}

#[cfg(test)]
mod tests {
    use context::repo::RepoUrlParts as Repo;

    use super::*;

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

    #[test]
    fn test_test_new() {
        let name = "test_name".to_string();
        let parent_name = "parent_name".to_string();
        let class_name = Some("class_name".to_string());
        let file = Some("file".to_string());
        let org_slug = "org_slug";
        let repo = BundleRepo {
            repo: Repo {
                host: "host".to_string(),
                owner: "owner".to_string(),
                name: "name".to_string(),
            },
            repo_root: "repo_root".to_string(),
            repo_url: "repo_url".to_string(),
            repo_head_sha: "repo_head_sha".to_string(),
            repo_head_branch: "repo_head_branch".to_string(),
            repo_head_commit_epoch: 1724102768,
            repo_head_commit_message: "repo_head_commit_message".to_string(),
            repo_head_author_name: "repo_head_author_name".to_string(),
            repo_head_author_email: "repo_head_author_email".to_string(),
        };
        let result = Test::new(
            name.clone(),
            parent_name.clone(),
            class_name.clone(),
            file.clone(),
            None,
            org_slug,
            &repo,
        );
        assert_eq!(result.name, name);
        assert_eq!(result.parent_name, parent_name);
        assert_eq!(result.class_name, class_name);
        assert_eq!(result.file, file);
        assert_eq!(result.id, "aad1f138-09ab-5ea9-9c21-af48a03d6edd");
        let result = Test::new(
            name.clone(),
            parent_name.clone(),
            class_name.clone(),
            file.clone(),
            Some(String::from("da5b8893-d6ca-5c1c-9a9c-91f40a2a3649")),
            org_slug,
            &repo,
        );
        assert_eq!(result.name, name);
        assert_eq!(result.parent_name, parent_name);
        assert_eq!(result.class_name, class_name);
        assert_eq!(result.file, file);
        assert_eq!(result.id, "da5b8893-d6ca-5c1c-9a9c-91f40a2a3649");
    }
}
