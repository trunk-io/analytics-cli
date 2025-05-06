#[cfg(feature = "pyo3")]
use pyo3::prelude::*;
#[cfg(feature = "pyo3")]
use pyo3_stub_gen::derive::gen_stub_pyclass;
#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

use crate::{
    env::parser::{clean_branch, BranchClass, CIInfo},
    repo::BundleRepo,
};

#[cfg(feature = "bindings")]
pub mod bindings;
pub mod validator;

#[cfg_attr(feature = "pyo3", gen_stub_pyclass, pyclass(get_all))]
#[cfg_attr(feature = "wasm", wasm_bindgen(getter_with_clone))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetaContext {
    pub ci_info: CIInfo,
}

impl MetaContext {
    pub fn new(ci_info: &CIInfo, repo: &BundleRepo, stable_branches: &[&str]) -> Self {
        let mut enriched_ci_info = ci_info.clone();

        if enriched_ci_info.branch.is_none() {
            let new_branch = clean_branch(&repo.repo_head_branch);
            let new_branch_class = BranchClass::from((
                new_branch.as_str(),
                enriched_ci_info.pr_number,
                None,
                stable_branches,
            ));
            enriched_ci_info.branch = Some(new_branch);
            enriched_ci_info.branch_class = Some(new_branch_class);
        }
        if enriched_ci_info.actor.is_none() {
            enriched_ci_info.actor = Some(repo.repo_head_author_email.clone());
        }
        if enriched_ci_info.commit_message.is_none() {
            enriched_ci_info.commit_message = Some(repo.repo_head_commit_message.clone());
        }
        if enriched_ci_info.committer_name.is_none() {
            enriched_ci_info.committer_name = Some(repo.repo_head_author_name.clone());
        }
        if enriched_ci_info.committer_email.is_none() {
            enriched_ci_info.committer_email = Some(repo.repo_head_author_email.clone());
        }
        if enriched_ci_info.author_name.is_none() {
            enriched_ci_info.author_name = Some(repo.repo_head_author_name.clone());
        }
        if enriched_ci_info.author_email.is_none() {
            enriched_ci_info.author_email = Some(repo.repo_head_author_email.clone());
        }

        Self {
            ci_info: enriched_ci_info,
        }
    }
}
