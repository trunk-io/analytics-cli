#[cfg(feature = "pyo3")]
use pyo3::prelude::*;
#[cfg(feature = "pyo3")]
use pyo3_stub_gen::derive::gen_stub_pyclass_enum;
use serde::{Deserialize, Serialize};
#[cfg(feature = "wasm")]
use tsify_next::Tsify;
#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "pyo3", gen_stub_pyclass_enum, pyclass(eq, eq_int))]
#[cfg_attr(feature = "wasm", derive(Tsify))]
pub enum TestRunnerJunitStatus {
    #[default]
    Passed,
    Failed,
    Flaky,
}

/// Encapsulates the glob path for a junit and, if applicable, the flakiness already
/// assigned by the user's test runner. See bazel_bep/parser.rs for more.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JunitPathWrapper {
    /// Path or glob pattern to the junit file.
    pub junit_path: String,
    /// Refers to an optional status parsed from the test runner's output, before junits have been parsed.
    pub status: Option<TestRunnerJunitStatus>,
}
