use context::{meta::id::generate_info_id_variant_wrapper, repo::RepoUrlParts};
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
    pub is_quarantined: bool,
    pub failure_message: Option<String>,
}

impl Test {
    pub fn new<T: AsRef<str>>(
        id: Option<T>,
        name: String,
        parent_name: String,
        class_name: Option<String>,
        file: Option<String>,
        org_slug: T,
        repo: &RepoUrlParts,
        timestamp_millis: Option<i64>,
        variant: T,
    ) -> Self {
        let mut test = Self {
            parent_name,
            name,
            class_name,
            file,
            id: String::with_capacity(0),
            timestamp_millis,
            is_quarantined: false,
            failure_message: None,
        };

        if let Some(id) = id {
            test.generate_custom_uuid(org_slug.as_ref(), repo, id.as_ref(), variant.as_ref());
        } else {
            test.set_id(org_slug, repo, variant);
        }

        test
    }

    pub fn set_id<T: AsRef<str>>(&mut self, org_slug: T, repo: &RepoUrlParts, variant: T) {
        self.id = generate_info_id_variant_wrapper(
            org_slug.as_ref(),
            repo.repo_full_name().as_str(),
            self.file.as_deref(),
            self.class_name.as_deref(),
            Some(self.parent_name.as_str()),
            Some(self.name.as_str()),
            None,
            variant.as_ref(),
        );
    }

    pub fn generate_custom_uuid<T: AsRef<str>>(
        &mut self,
        org_slug: T,
        repo: &RepoUrlParts,
        id: T,
        variant: T,
    ) {
        if id.as_ref().is_empty() {
            self.set_id(org_slug.as_ref(), repo, variant.as_ref());
            return;
        }
        self.id = generate_info_id_variant_wrapper(
            org_slug.as_ref(),
            repo.repo_full_name().as_str(),
            self.file.as_deref(),
            self.class_name.as_deref(),
            Some(self.parent_name.as_str()),
            Some(self.name.as_str()),
            Some(id.as_ref()),
            variant.as_ref(),
        );
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
            None,
            name.clone(),
            parent_name.clone(),
            class_name.clone(),
            file.clone(),
            org_slug,
            &repo,
            Some(0),
            "",
        );
        assert_eq!(result.name, name);
        assert_eq!(result.parent_name, parent_name);
        assert_eq!(result.class_name, class_name);
        assert_eq!(result.file, file);
        assert_eq!(result.id, "aad1f138-09ab-5ea9-9c21-af48a03d6edd");
        let result = Test::new(
            Some("aad1f138-09ab-5ea9-9c21-af48a03d6edd"),
            name.clone(),
            parent_name.clone(),
            class_name.clone(),
            file.clone(),
            org_slug,
            &repo,
            Some(0),
            "",
        );
        assert_eq!(result.name, name);
        assert_eq!(result.parent_name, parent_name);
        assert_eq!(result.class_name, class_name);
        assert_eq!(result.file, file);
        assert_eq!(result.id, "aad1f138-09ab-5ea9-9c21-af48a03d6edd");
        let result = Test::new(
            Some("trunk:example-id"),
            name.clone(),
            parent_name.clone(),
            class_name.clone(),
            file.clone(),
            org_slug,
            &repo,
            Some(0),
            "",
        );
        assert_eq!(result.name, name);
        assert_eq!(result.parent_name, parent_name);
        assert_eq!(result.class_name, class_name);
        assert_eq!(result.file, file);
        assert_eq!(result.id, "208beb01-6179-546e-b0dd-8502e24ae85c");
        let result = Test {
            name: name.clone(),
            parent_name: parent_name.clone(),
            class_name: class_name.clone(),
            file: file.clone(),
            id: String::from("da5b8893-d6ca-5c1c-9a9c-91f40a2a3649"),
            timestamp_millis: Some(0),
            is_quarantined: false,
            failure_message: None,
        };
        assert_eq!(result.name, name);
        assert_eq!(result.parent_name, parent_name);
        assert_eq!(result.class_name, class_name);
        assert_eq!(result.file, file);
        assert_eq!(result.id, "da5b8893-d6ca-5c1c-9a9c-91f40a2a3649");
    }
}
