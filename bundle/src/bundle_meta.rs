//! BundleMeta is the contents of meta.json
//! - We create a BundleMeta (current CLI version) on upload
//! - We read BundleMetaV_* (old versions), incrementally, during parsing on services side
//!

use codeowners::CodeOwners;
use context::repo::BundleRepo;
use serde::{Deserialize, Serialize};
#[cfg(feature = "wasm")]
use tsify_next::Tsify;
#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

use crate::{files::FileSet, CustomTag, MapType, Test};

pub const META_VERSION: &str = "1";
// 0.5.29 was first version to include bundle_upload_id and serves as the base
#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
// #[cfg_attr(feature = "wasm", wasm_bindgen(getter_with_clone))]
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
#[cfg_attr(feature = "wasm", derive(Tsify))]
// #[cfg_attr(feature = "wasm", wasm_bindgen(getter_with_clone))]
pub struct BundleMetaV0_5_29 {
    #[serde(flatten)]
    pub base_props: BundleMetaBaseProps,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
// #[cfg_attr(feature = "wasm", wasm_bindgen(getter_with_clone))]
pub struct BundleMetaJunitProps {
    pub num_files: usize,
    pub num_tests: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
// #[cfg_attr(feature = "wasm", wasm_bindgen(getter_with_clone))]
pub struct BundleMetaV0_5_34 {
    #[serde(flatten)]
    pub base_props: BundleMetaBaseProps,
    #[serde(flatten)]
    pub junit_props: BundleMetaJunitProps,
}

#[derive(Serialize, Deserialize, Debug)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
// #[cfg_attr(feature = "wasm", wasm_bindgen(getter_with_clone))]
#[cfg_attr(feature = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
#[serde(tag = "schema")]
pub enum VersionedBundle {
    V0_5_29(BundleMetaV0_5_29),
    V0_5_34(BundleMetaV0_5_34),
}

/// Signifies the latest BundleMeta version
pub type BundleMeta = BundleMetaV0_5_34;
