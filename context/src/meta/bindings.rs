#[cfg(feature = "pyo3")]
use pyo3::prelude::*;
#[cfg(feature = "pyo3")]
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};
#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

use super::MetaContext;
use crate::env::parser::CIInfo;
#[cfg(feature = "pyo3")]
use crate::repo::BundleRepo;

#[cfg_attr(feature = "pyo3", gen_stub_pyclass, pyclass(get_all))]
#[cfg_attr(feature = "wasm", wasm_bindgen(getter_with_clone))]
#[derive(Clone, Debug)]
pub struct BindingsMetaContext {
    pub ci_info: CIInfo,
}

#[cfg(feature = "pyo3")]
#[gen_stub_pymethods]
#[pymethods]
impl BindingsMetaContext {
    #[cfg(feature = "pyo3")]
    #[new]
    pub fn new(ci_info: &CIInfo, repo: &BundleRepo, stable_branches: Vec<String>) -> Self {
        let stable_branches_ref: &[&str] = &stable_branches
            .iter()
            .map(String::as_str)
            .collect::<Vec<&str>>();
        BindingsMetaContext::from(MetaContext::new(ci_info, repo, stable_branches_ref))
    }
}

impl From<MetaContext> for BindingsMetaContext {
    fn from(MetaContext { ci_info }: MetaContext) -> Self {
        Self { ci_info }
    }
}

impl From<BindingsMetaContext> for MetaContext {
    fn from(val: BindingsMetaContext) -> Self {
        let BindingsMetaContext { ci_info } = val;
        MetaContext { ci_info }
    }
}
