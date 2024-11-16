use codeowners::CodeOwners;
use context::repo::BundleRepo;
use serde::{Deserialize, Serialize};

use crate::{files::FileSet, CustomTag, MapType, Test};

#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

pub const META_VERSION: &str = "1";

#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "wasm", wasm_bindgen(getter_with_clone))]
pub struct BundleMeta {
    pub version: String,
    pub cli_version: String,
    pub org: String,
    pub repo: BundleRepo,
    pub bundle_upload_id: String,
    pub tags: Vec<CustomTag>,
    pub file_sets: Vec<FileSet>,
    pub num_files: usize,
    pub num_tests: usize,
    pub envs: MapType,
    pub upload_time_epoch: u64,
    pub test_command: Option<String>,
    pub os_info: Option<String>,
    pub quarantined_tests: Vec<Test>,
    pub codeowners: Option<CodeOwners>,
}
