use context::repo::RepoUrlParts;
#[cfg(feature = "pyo3")]
use pyo3::prelude::*;
#[cfg(feature = "pyo3")]
use pyo3_stub_gen::derive::gen_stub_pyclass;
use serde::{Deserialize, Serialize};
#[cfg(feature = "wasm")]
use tsify_next::Tsify;
#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "pyo3", gen_stub_pyclass, pyclass(get_all))]
#[cfg_attr(feature = "wasm", derive(Tsify))]
pub struct Test {
    pub name: String,
    #[serde(rename = "parentName")]
    pub parent_name: String,
    #[serde(rename = "className")]
    pub class_name: Option<String>,
    pub file: Option<String>,
    pub id: String,
    /// Added in v0.6.9
    pub timestamp_millis: Option<i64>,
}

impl Test {
    pub fn new<T: AsRef<str>>(
        name: String,
        parent_name: String,
        class_name: Option<String>,
        file: Option<String>,
        org_slug: T,
        repo: &RepoUrlParts,
        timestamp_millis: Option<i64>,
    ) -> Self {
        let mut test = Self {
            parent_name,
            name,
            class_name,
            file,
            id: String::with_capacity(0),
            timestamp_millis,
        };

        test.set_id(org_slug, repo);

        test
    }

    pub fn set_id<T: AsRef<str>>(&mut self, org_slug: T, repo: &RepoUrlParts) {
        let info_id_input = [
            org_slug.as_ref(),
            repo.repo_full_name().as_str(),
            self.file.as_deref().unwrap_or(""),
            self.class_name.as_deref().unwrap_or(""),
            &self.parent_name,
            &self.name,
            "JUNIT_TESTCASE",
        ]
        .join("#");
        self.id =
            uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_URL, info_id_input.as_bytes()).to_string()
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

/// Custom tags defined by the user.
///
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "pyo3", gen_stub_pyclass, pyclass(get_all))]
#[cfg_attr(feature = "wasm", derive(Tsify))]
pub struct CustomTag {
    pub key: String,
    pub value: String,
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
            let actual: Vec<CustomTag> = crate::custom_tag::parse_custom_tags(&tags_str).unwrap();
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
            let actual = crate::custom_tag::parse_custom_tags(&tags_str);
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
        let repo = Repo {
            host: "host".to_string(),
            owner: "owner".to_string(),
            name: "name".to_string(),
        };
        let result = Test::new(
            name.clone(),
            parent_name.clone(),
            class_name.clone(),
            file.clone(),
            org_slug,
            &repo,
            Some(0),
        );
        assert_eq!(result.name, name);
        assert_eq!(result.parent_name, parent_name);
        assert_eq!(result.class_name, class_name);
        assert_eq!(result.file, file);
        assert_eq!(result.id, "aad1f138-09ab-5ea9-9c21-af48a03d6edd");
        let result = Test {
            name: name.clone(),
            parent_name: parent_name.clone(),
            class_name: class_name.clone(),
            file: file.clone(),
            id: String::from("da5b8893-d6ca-5c1c-9a9c-91f40a2a3649"),
            timestamp_millis: Some(0),
        };
        assert_eq!(result.name, name);
        assert_eq!(result.parent_name, parent_name);
        assert_eq!(result.class_name, class_name);
        assert_eq!(result.file, file);
        assert_eq!(result.id, "da5b8893-d6ca-5c1c-9a9c-91f40a2a3649");
    }
}
