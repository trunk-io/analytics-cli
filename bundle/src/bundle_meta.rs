//! BundleMeta is the contents of meta.json
//! - We create a BundleMeta (current CLI version) on upload
//! - We read BundleMetaV_* (old versions), incrementally, during parsing on services side
//!

use std::collections::HashMap;

use codeowners::CodeOwners;
#[cfg(feature = "bindings")]
use context::junit;
use context::repo::BundleRepo;
#[cfg(feature = "pyo3")]
use pyo3::{exceptions::PyTypeError, prelude::*};
#[cfg(feature = "pyo3")]
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};
use serde::{Deserialize, Serialize};
#[cfg(feature = "wasm")]
use tsify_next::Tsify;
#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

use crate::{
    files::{BundledFile, FileSet},
    CustomTag, Test,
};

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
    pub use_uncloned_repo: Option<bool>,
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

#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq, Eq)]
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
    pub bundle_upload_id_v2: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "pyo3", gen_stub_pyclass, pyclass(get_all))]
#[cfg_attr(feature = "wasm", derive(Tsify))]
pub struct BundleMetaV0_7_6 {
    #[serde(flatten)]
    pub base_props: BundleMetaBaseProps,
    #[serde(flatten)]
    pub junit_props: BundleMetaJunitProps,
    #[serde(flatten)]
    pub debug_props: BundleMetaDebugProps,
    pub bundle_upload_id_v2: String,
    pub variant: Option<String>,
}

impl From<BundleMetaV0_7_6> for BundleMetaV0_6_3 {
    fn from(bundle_meta: BundleMetaV0_7_6) -> Self {
        BundleMetaV0_6_3 {
            bundle_upload_id_v2: bundle_meta.base_props.bundle_upload_id.clone(),
            base_props: bundle_meta.base_props,
            debug_props: bundle_meta.debug_props,
            junit_props: bundle_meta.junit_props,
        }
    }
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

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "pyo3", gen_stub_pyclass, pyclass(get_all))]
#[cfg_attr(feature = "wasm", derive(Tsify))]
pub struct BundleMetaV0_7_7 {
    #[serde(flatten)]
    pub base_props: BundleMetaBaseProps,
    #[serde(flatten)]
    pub junit_props: BundleMetaJunitProps,
    #[serde(flatten)]
    pub debug_props: BundleMetaDebugProps,
    pub bundle_upload_id_v2: String,
    pub variant: Option<String>,
    pub internal_bundled_file: Option<BundledFile>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "pyo3", gen_stub_pyclass, pyclass(get_all))]
#[cfg_attr(feature = "wasm", derive(Tsify))]
pub struct BundleMetaV0_7_8 {
    #[serde(flatten)]
    pub base_props: BundleMetaBaseProps,
    #[serde(flatten)]
    pub junit_props: BundleMetaJunitProps,
    #[serde(flatten)]
    pub debug_props: BundleMetaDebugProps,
    pub bundle_upload_id_v2: String,
    pub variant: Option<String>,
    pub internal_bundled_file: Option<BundledFile>,
    pub failed_tests: Vec<Test>,
}

impl From<BundleMetaV0_7_8> for BundleMetaV0_7_7 {
    fn from(bundle_meta: BundleMetaV0_7_8) -> Self {
        BundleMetaV0_7_7 {
            base_props: bundle_meta.base_props,
            junit_props: bundle_meta.junit_props,
            debug_props: bundle_meta.debug_props,
            bundle_upload_id_v2: bundle_meta.bundle_upload_id_v2,
            variant: bundle_meta.variant,
            internal_bundled_file: bundle_meta.internal_bundled_file,
        }
    }
}

impl From<BundleMetaV0_7_7> for BundleMetaV0_7_6 {
    fn from(bundle_meta: BundleMetaV0_7_7) -> Self {
        BundleMetaV0_7_6 {
            base_props: bundle_meta.base_props,
            junit_props: bundle_meta.junit_props,
            debug_props: bundle_meta.debug_props,
            bundle_upload_id_v2: bundle_meta.bundle_upload_id_v2,
            variant: bundle_meta.variant,
        }
    }
}

impl From<BundleMetaV0_7_8> for BundleMetaV0_7_6 {
    fn from(bundle_meta: BundleMetaV0_7_8) -> Self {
        BundleMetaV0_7_6 {
            base_props: bundle_meta.base_props,
            junit_props: bundle_meta.junit_props,
            debug_props: bundle_meta.debug_props,
            bundle_upload_id_v2: bundle_meta.bundle_upload_id_v2,
            variant: bundle_meta.variant,
        }
    }
}

impl From<BundleMetaV0_7_8> for BundleMetaV0_6_3 {
    fn from(bundle_meta: BundleMetaV0_7_8) -> Self {
        BundleMetaV0_6_3 {
            base_props: bundle_meta.base_props,
            junit_props: bundle_meta.junit_props,
            debug_props: bundle_meta.debug_props,
            bundle_upload_id_v2: bundle_meta.bundle_upload_id_v2,
        }
    }
}

impl From<BundleMetaV0_7_8> for BundleMetaV0_6_2 {
    fn from(bundle_meta: BundleMetaV0_7_8) -> Self {
        BundleMetaV0_6_2 {
            base_props: bundle_meta.base_props,
            junit_props: bundle_meta.junit_props,
            debug_props: bundle_meta.debug_props,
        }
    }
}

impl From<BundleMetaV0_7_8> for BundleMetaV0_5_34 {
    fn from(bundle_meta: BundleMetaV0_7_8) -> Self {
        BundleMetaV0_5_34 {
            base_props: bundle_meta.base_props,
            junit_props: bundle_meta.junit_props,
        }
    }
}

impl From<BundleMetaV0_7_8> for BundleMetaV0_5_29 {
    fn from(bundle_meta: BundleMetaV0_7_8) -> Self {
        BundleMetaV0_5_29 {
            base_props: bundle_meta.base_props,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
#[cfg_attr(feature = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
#[serde(tag = "schema")]
pub enum VersionedBundle {
    V0_5_29(BundleMetaV0_5_29),
    V0_5_34(BundleMetaV0_5_34),
    V0_6_2(BundleMetaV0_6_2),
    V0_6_3(BundleMetaV0_6_3),
    V0_7_6(BundleMetaV0_7_6),
    V0_7_7(BundleMetaV0_7_7),
    V0_7_8(BundleMetaV0_7_8),
}

impl VersionedBundle {
    pub fn internal_bundled_file(&self) -> Option<BundledFile> {
        match self {
            Self::V0_7_8(data) => data.internal_bundled_file.clone(),
            Self::V0_7_7(data) => data.internal_bundled_file.clone(),
            _ => None,
        }
    }
}

#[cfg_attr(feature = "wasm", wasm_bindgen(getter_with_clone))]
#[derive(Debug, Clone)]
pub struct VersionedBundleWithBindingsReport {
    pub versioned_bundle: VersionedBundle,
    #[cfg(feature = "bindings")]
    pub bindings_report: Vec<junit::bindings::BindingsReport>,
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
    pub fn dump_json(&self) -> PyResult<String> {
        serde_json::to_string(&self.0).map_err(|err| PyTypeError::new_err(err.to_string()))
    }

    pub fn get_v0_5_29(&self) -> BundleMetaV0_5_29 {
        match &self.0 {
            VersionedBundle::V0_7_8(bundle_meta) => {
                BundleMetaV0_5_29::from(BundleMetaV0_6_3::from(BundleMetaV0_7_6::from(
                    BundleMetaV0_7_7::from(bundle_meta.clone()),
                )))
            }
            VersionedBundle::V0_7_7(bundle_meta) => BundleMetaV0_5_29::from(
                BundleMetaV0_6_3::from(BundleMetaV0_7_6::from(bundle_meta.clone())),
            ),
            VersionedBundle::V0_7_6(bundle_meta) => {
                BundleMetaV0_5_29::from(BundleMetaV0_6_3::from(bundle_meta.clone()))
            }
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
            VersionedBundle::V0_7_8(bundle_meta) => {
                Some(BundleMetaV0_5_34::from(BundleMetaV0_6_3::from(
                    BundleMetaV0_7_6::from(BundleMetaV0_7_7::from(bundle_meta.clone())),
                )))
            }
            VersionedBundle::V0_7_7(bundle_meta) => Some(BundleMetaV0_5_34::from(
                BundleMetaV0_6_3::from(BundleMetaV0_7_6::from(bundle_meta.clone())),
            )),
            VersionedBundle::V0_7_6(bundle_meta) => Some(BundleMetaV0_5_34::from(
                BundleMetaV0_6_3::from(bundle_meta.clone()),
            )),
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
            VersionedBundle::V0_7_8(bundle_meta) => {
                Some(BundleMetaV0_6_2::from(BundleMetaV0_6_3::from(
                    BundleMetaV0_7_6::from(BundleMetaV0_7_7::from(bundle_meta.clone())),
                )))
            }
            VersionedBundle::V0_7_7(bundle_meta) => Some(BundleMetaV0_6_2::from(
                BundleMetaV0_6_3::from(BundleMetaV0_7_6::from(bundle_meta.clone())),
            )),
            VersionedBundle::V0_7_6(bundle_meta) => Some(BundleMetaV0_6_2::from(
                BundleMetaV0_6_3::from(bundle_meta.clone()),
            )),
            VersionedBundle::V0_6_3(bundle_meta) => {
                Some(BundleMetaV0_6_2::from(bundle_meta.clone()))
            }
            VersionedBundle::V0_6_2(bundle_meta) => Some(bundle_meta.clone()),
            _ => None,
        }
    }
    pub fn get_v0_6_3(&self) -> Option<BundleMetaV0_6_3> {
        match &self.0 {
            VersionedBundle::V0_7_8(bundle_meta) => Some(BundleMetaV0_6_3::from(
                BundleMetaV0_7_6::from(BundleMetaV0_7_7::from(bundle_meta.clone())),
            )),
            VersionedBundle::V0_7_7(bundle_meta) => Some(BundleMetaV0_6_3::from(
                BundleMetaV0_7_6::from(bundle_meta.clone()),
            )),
            VersionedBundle::V0_7_6(bundle_meta) => {
                Some(BundleMetaV0_6_3::from(bundle_meta.clone()))
            }
            VersionedBundle::V0_6_3(bundle_meta) => Some(bundle_meta.clone()),
            _ => None,
        }
    }
    pub fn get_v0_7_6(&self) -> Option<BundleMetaV0_7_6> {
        match &self.0 {
            VersionedBundle::V0_7_8(bundle_meta) => Some(BundleMetaV0_7_6::from(
                BundleMetaV0_7_7::from(bundle_meta.clone()),
            )),
            VersionedBundle::V0_7_7(bundle_meta) => {
                Some(BundleMetaV0_7_6::from(bundle_meta.clone()))
            }
            VersionedBundle::V0_7_6(bundle_meta) => Some(bundle_meta.clone()),
            _ => None,
        }
    }
    pub fn get_v0_7_7(&self) -> Option<BundleMetaV0_7_7> {
        match &self.0 {
            VersionedBundle::V0_7_8(bundle_meta) => {
                Some(BundleMetaV0_7_7::from(bundle_meta.clone()))
            }
            VersionedBundle::V0_7_7(bundle_meta) => Some(bundle_meta.clone()),
            _ => None,
        }
    }
    pub fn get_v0_7_8(&self) -> Option<BundleMetaV0_7_8> {
        match &self.0 {
            VersionedBundle::V0_7_8(bundle_meta) => Some(bundle_meta.clone()),
            _ => None,
        }
    }
}

/// Signifies the latest BundleMeta version
pub type BundleMeta = BundleMetaV0_7_8;
