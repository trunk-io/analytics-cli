//! BundleMeta is the contents of meta.json
//! - We create a BundleMeta (current CLI version) on upload
//! - We read BundleMetaV_* (old versions), incrementally, during parsing on services side
//!

use codeowners::CodeOwners;
use context::repo::BundleRepo;
use serde::{Deserialize, Serialize};
#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

use crate::{files::FileSet, CustomTag, MapType, Test};

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
#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "wasm", wasm_bindgen(getter_with_clone))]
pub struct BundleMetaV0_5_29 {
    #[serde(flatten)]
    pub base_props: BundleMetaBaseProps,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "wasm", wasm_bindgen(getter_with_clone))]
pub struct BundleMetaJunitProps {
    pub num_files: usize,
    pub num_tests: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "wasm", wasm_bindgen(getter_with_clone))]
pub struct BundleMeta {
    #[serde(flatten)]
    pub base_props: BundleMetaBaseProps,
    #[serde(flatten)]
    pub junit_props: Option<BundleMetaJunitProps>,
}

//// Add new versions here and rename the above struct ////

pub type BundleMetaV0_5_34 = BundleMeta;

impl BundleMeta {
    pub fn new(
        version: String,
        cli_version: String,
        org: String,
        repo: BundleRepo,
        bundle_upload_id: String,
        tags: Vec<CustomTag>,
        file_sets: Vec<FileSet>,
        num_files: usize,
        num_tests: usize,
        envs: MapType,
        upload_time_epoch: u64,
        test_command: Option<String>,
        os_info: Option<String>,
        quarantined_tests: Vec<Test>,
        codeowners: Option<CodeOwners>,
    ) -> Self {
        BundleMeta {
            base_props: BundleMetaBaseProps {
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
            },
            junit_props: Some(BundleMetaJunitProps {
                num_files,
                num_tests,
            }),
        }
    }
}
