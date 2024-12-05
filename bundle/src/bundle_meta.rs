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

impl From<BundleMetaV0_5_34> for BundleMetaV0_5_29 {
    fn from(bundle_meta: BundleMetaV0_5_34) -> Self {
        BundleMetaV0_5_29 {
            base_props: bundle_meta.base_props,
        }
    }
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

impl From<BundleMetaV0_6_2> for BundleMetaV0_5_34 {
    fn from(bundle_meta: BundleMetaV0_6_2) -> Self {
        BundleMetaV0_5_34 {
            base_props: bundle_meta.base_props,
            junit_props: bundle_meta.junit_props,
        }
    }
}

impl From<BundleMetaV0_6_2> for BundleMetaV0_5_29 {
    fn from(bundle_meta: BundleMetaV0_6_2) -> Self {
        BundleMetaV0_5_29 {
            base_props: bundle_meta.base_props,
        }
    }
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

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "pyo3", gen_stub_pyclass, pyclass(get_all))]
#[cfg_attr(feature = "wasm", derive(Tsify))]
pub struct BundleMetaV0_6_3 {
    #[serde(flatten)]
    pub base_props: BundleMetaBaseProps,
    #[serde(flatten)]
    pub junit_props: BundleMetaJunitProps,
    #[serde(flatten)]
    pub debug_props: BundleMetaDebugProps,
    pub bundle_upload_id_v2: Option<String>, // TODO(pat): make this required
}

impl From<BundleMetaV0_6_3> for BundleMetaV0_6_2 {
    fn from(bundle_meta: BundleMetaV0_6_3) -> Self {
        BundleMetaV0_6_2 {
            base_props: bundle_meta.base_props,
            junit_props: bundle_meta.junit_props,
            debug_props: bundle_meta.debug_props,
        }
    }
}

impl From<BundleMetaV0_6_3> for BundleMetaV0_5_34 {
    fn from(bundle_meta: BundleMetaV0_6_3) -> Self {
        BundleMetaV0_5_34 {
            base_props: bundle_meta.base_props,
            junit_props: bundle_meta.junit_props,
        }
    }
}

impl From<BundleMetaV0_6_3> for BundleMetaV0_5_29 {
    fn from(bundle_meta: BundleMetaV0_6_3) -> Self {
        BundleMetaV0_5_29 {
            base_props: bundle_meta.base_props,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
#[cfg_attr(feature = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
#[serde(tag = "schema")]
pub enum VersionedBundle {
    V0_5_29(BundleMetaV0_5_29),
    V0_5_34(BundleMetaV0_5_34),
    V0_6_2(BundleMetaV0_6_2),
    V0_6_3(BundleMetaV0_6_3),
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
    pub fn get_v0_5_29(&self) -> BundleMetaV0_5_29 {
        match &self.0 {
            VersionedBundle::V0_6_3(bundle_meta) => {
                BundleMetaV0_5_29::from(BundleMetaV0_6_2::from(bundle_meta.clone()))
            }
            VersionedBundle::V0_6_2(bundle_meta) => {
                BundleMetaV0_5_29::from(BundleMetaV0_5_34::from(bundle_meta.clone()))
            }
            VersionedBundle::V0_5_34(bundle_meta) => BundleMetaV0_5_29::from(bundle_meta.clone()),
            VersionedBundle::V0_5_29(bundle_meta) => bundle_meta.clone(),
        }
    }
    pub fn get_v0_5_34(&self) -> Option<BundleMetaV0_5_34> {
        match &self.0 {
            VersionedBundle::V0_6_3(bundle_meta) => {
                Some(BundleMetaV0_5_34::from(bundle_meta.clone()))
            }
            VersionedBundle::V0_6_2(bundle_meta) => {
                Some(BundleMetaV0_5_34::from(bundle_meta.clone()))
            }
            VersionedBundle::V0_5_34(bundle_meta) => Some(bundle_meta.clone()),
            _ => None,
        }
    }
    pub fn get_v0_6_2(&self) -> Option<BundleMetaV0_6_2> {
        match &self.0 {
            VersionedBundle::V0_6_3(bundle_meta) => {
                Some(BundleMetaV0_6_2::from(bundle_meta.clone()))
            }
            VersionedBundle::V0_6_2(bundle_meta) => Some(bundle_meta.clone()),
            _ => None,
        }
    }
    pub fn get_v0_6_3(&self) -> Option<BundleMetaV0_6_3> {
        match &self.0 {
            VersionedBundle::V0_6_3(bundle_meta) => Some(bundle_meta.clone()),
            _ => None,
        }
    }
}

/// Signifies the latest BundleMeta version
pub type BundleMeta = BundleMetaV0_6_3;
