use context::repo::RepoUrlParts;
#[cfg(feature = "pyo3")]
use pyo3::prelude::*;
#[cfg(feature = "pyo3")]
use pyo3_stub_gen::derive::gen_stub_pyclass_enum;
use serde::{Deserialize, Serialize};
#[cfg(feature = "wasm")]
use tsify_next::Tsify;

/// Which source the server resolved quarantine status from.
#[derive(Debug, Serialize, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "pyo3", gen_stub_pyclass_enum, pyclass(eq, eq_int))]
#[cfg_attr(feature = "wasm", derive(Tsify))]
#[serde(rename_all = "snake_case")]
pub enum QuarantineResolutionMode {
    Repo,
    TestCollection,
    #[default]
    Unspecified,
}

impl<'de> Deserialize<'de> for QuarantineResolutionMode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(
            match serde_json::Value::deserialize(deserializer)?.as_str() {
                Some("repo") => Self::Repo,
                Some("test_collection") => Self::TestCollection,
                _ => Self::Unspecified,
            },
        )
    }
}

impl From<QuarantineResolutionMode> for proto::upload_metrics::trunk::QuarantineResolutionMode {
    fn from(mode: QuarantineResolutionMode) -> Self {
        match mode {
            QuarantineResolutionMode::Repo => Self::Repo,
            QuarantineResolutionMode::TestCollection => Self::TestCollection,
            QuarantineResolutionMode::Unspecified => Self::Unspecified,
        }
    }
}

impl QuarantineResolutionMode {
    pub fn resolution_log_line(
        &self,
        test_collection_id: Option<&str>,
        repo: &RepoUrlParts,
    ) -> Option<String> {
        match self {
            Self::TestCollection => Some(format!(
                "Quarantine status applied from test collection {}",
                test_collection_id.unwrap_or("unknown"),
            )),
            Self::Repo => Some(format!(
                "Quarantine status applied from repo {}",
                repo.repo_full_name()
            )),
            Self::Unspecified => None,
        }
    }
}
