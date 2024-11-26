//! BundleMeta is the contents of meta.json
//! - We create a BundleMeta (current CLI version) on upload
//! - We read BundleMetaV_* (old versions), incrementally, during parsing on services side
//!

use codeowners::CodeOwners;
use context::repo::BundleRepo;
#[cfg(feature = "pyo3")]
use pyo3::prelude::*;
#[cfg(feature = "pyo3")]
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
#[cfg(feature = "wasm")]
use tsify_next::Tsify;
#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

use crate::{files::FileSet, CustomTag, Test};

pub const META_VERSION: &str = "1";
// 0.5.29 was first version to include bundle_upload_id and serves as the base
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "pyo3", gen_stub_pyclass, pyclass(get_all))]
#[cfg_attr(feature = "wasm", derive(Tsify))]
pub struct BundleMetaBaseProps {
    pub version: String,
    pub cli_version: String,
    pub org: String,
    pub repo: BundleRepo,
    pub bundle_upload_id: String,
    pub tags: Vec<CustomTag>,
    pub file_sets: Vec<FileSet>,
    pub envs: HashMap<String, String>,
    pub upload_time_epoch: u64,
    pub test_command: Option<String>,
    pub os_info: Option<String>,
    pub quarantined_tests: Vec<Test>,
    pub codeowners: Option<CodeOwners>,
}
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "pyo3", gen_stub_pyclass, pyclass(get_all))]
#[cfg_attr(feature = "wasm", derive(Tsify))]
pub struct BundleMetaV0_5_29 {
    #[serde(flatten)]
    pub base_props: BundleMetaBaseProps,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "pyo3", gen_stub_pyclass, pyclass(get_all))]
#[cfg_attr(feature = "wasm", derive(Tsify))]
pub struct BundleMetaJunitProps {
    pub num_files: usize,
    pub num_tests: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "pyo3", gen_stub_pyclass, pyclass(get_all))]
#[cfg_attr(feature = "wasm", derive(Tsify))]
pub struct BundleMetaV0_5_34 {
    #[serde(flatten)]
    pub base_props: BundleMetaBaseProps,
    #[serde(flatten)]
    pub junit_props: BundleMetaJunitProps,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "pyo3", gen_stub_pyclass, pyclass(get_all))]
#[cfg_attr(feature = "wasm", derive(Tsify))]
pub struct BundleMetaDebugProps {
    pub command_line: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "pyo3", gen_stub_pyclass, pyclass(get_all))]
#[cfg_attr(feature = "wasm", derive(Tsify))]
pub struct BundleMetaV0_6_2 {
    #[serde(flatten)]
    pub base_props: BundleMetaBaseProps,
    #[serde(flatten)]
    pub junit_props: BundleMetaJunitProps,
    #[serde(flatten)]
    pub debug_props: BundleMetaDebugProps,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
#[cfg_attr(feature = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
#[serde(tag = "schema")]
pub enum VersionedBundle {
    V0_5_29(BundleMetaV0_5_29),
    V0_5_34(BundleMetaV0_5_34),
    V0_6_2(BundleMetaV0_6_2),
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
#[cfg(feature = "pyo3")]
#[gen_stub_pyclass]
#[pyclass]
pub struct BindingsVersionedBundle(pub VersionedBundle);

#[cfg(feature = "pyo3")]
#[gen_stub_pymethods]
#[pymethods]
impl BindingsVersionedBundle {
    pub fn get_v0_5_29(&self) -> Option<BundleMetaV0_5_29> {
        match &self.0 {
            VersionedBundle::V0_5_29(bundle_meta) => Some(bundle_meta.clone()),
            _ => None,
        }
    }
    pub fn get_v0_5_34(&self) -> Option<BundleMetaV0_5_34> {
        match &self.0 {
            VersionedBundle::V0_5_34(bundle_meta) => Some(bundle_meta.clone()),
            _ => None,
        }
    }
    pub fn get_v0_6_2(&self) -> Option<BundleMetaV0_6_2> {
        match &self.0 {
            VersionedBundle::V0_6_2(bundle_meta) => Some(bundle_meta.clone()),
            _ => None,
        }
    }
}

/// Signifies the latest BundleMeta version
pub type BundleMeta = BundleMetaV0_6_2;
