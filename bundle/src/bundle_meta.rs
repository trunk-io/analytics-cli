//! BundleMeta is the contents of meta.json
//! - We create a BundleMeta (current CLI version) on upload
//! - We read BundleMetaV_* (old versions), incrementally, during parsing on services side
//!
use std::ops::Deref;

use codeowners::CodeOwners;
use context::repo::BundleRepo;
use serde::{Deserialize, Serialize};

use crate::{files::FileSet, CustomTag, MapType, Test};

#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

pub const META_VERSION: &str = "1";

// 0.5.29 was first version to include bundle_upload_id and serves as the base
#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "wasm", wasm_bindgen(getter_with_clone))]
pub struct BundleMetaBaseProps {
    pub version: String,
    pub cli_version: String,
    pub org: String,
    pub repo: BundleRepo,
    pub bundle_upload_id: String,
    pub tags: Vec<CustomTag>,
    pub file_sets: Vec<FileSet>,
    pub envs: MapType,
    pub upload_time_epoch: u64,
    pub test_command: Option<String>,
    pub os_info: Option<String>,
    pub quarantined_tests: Vec<Test>,
    pub codeowners: Option<CodeOwners>,
}

impl BundleMetaBaseProps {
    pub fn new(
        version: String,
        cli_version: String,
        org: String,
        repo: BundleRepo,
        bundle_upload_id: String,
        tags: Vec<CustomTag>,
        file_sets: Vec<FileSet>,
        envs: MapType,
        upload_time_epoch: u64,
        test_command: Option<String>,
        os_info: Option<String>,
        quarantined_tests: Vec<Test>,
        codeowners: Option<CodeOwners>,
    ) -> Self {
        BundleMetaBaseProps {
            version,
            cli_version,
            org,
            repo,
            bundle_upload_id,
            tags,
            file_sets,
            envs,
            upload_time_epoch,
            test_command,
            os_info,
            quarantined_tests,
            codeowners,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "wasm", wasm_bindgen(getter_with_clone))]
pub struct BundleMetaV0_5_29(pub BundleMetaBaseProps);

impl BundleMetaV0_5_29 {
    pub fn new(props: BundleMetaBaseProps) -> Self {
        Self(props)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "wasm", wasm_bindgen(getter_with_clone))]
pub struct BundleMetaV0_6(pub BundleMetaV0_5_29);

impl Deref for BundleMetaV0_6 {
    type Target = BundleMetaBaseProps;

    fn deref(&self) -> &BundleMetaBaseProps {
        &self.0 .0
    }
}

impl BundleMetaV0_6 {
    pub fn new(v0_5_29: BundleMetaV0_5_29) -> Self {
        Self(v0_5_29)
    }
}

//// Add new versions here ////

pub type BundleMeta = BundleMetaV0_6;
