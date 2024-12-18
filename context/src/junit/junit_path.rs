#[cfg(feature = "pyo3")]
use pyo3::prelude::*;
#[cfg(feature = "pyo3")]
use pyo3_stub_gen::derive::gen_stub_pyclass_enum;
use serde::{Deserialize, Serialize};
#[cfg(feature = "wasm")]
use tsify_next::Tsify;
#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

use bazel_bep::types::build_event_stream::TestStatus;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "pyo3", gen_stub_pyclass_enum, pyclass(eq, eq_int))]
#[cfg_attr(feature = "wasm", derive(Tsify))]
pub enum JunitReportStatus {
    #[default]
    Passed,
    Failed,
    Flaky,
}

impl TryFrom<TestStatus> for JunitReportStatus {
    type Error = ();

    fn try_from(status: TestStatus) -> Result<Self, Self::Error> {
        match status {
            TestStatus::Passed => Ok(JunitReportStatus::Passed),
            TestStatus::Failed => Ok(JunitReportStatus::Failed),
            TestStatus::Flaky => Ok(JunitReportStatus::Flaky),
            _ => Err(()),
        }
    }
}

/// Encapsulates the glob path for a junit and, if applicable, the flakiness already
/// assigned by the user's test runner. See bazel_bep/parser.rs for more.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JunitReportFileWithStatus {
    /// Path or glob pattern to the junit file.
    pub junit_path: String,
    /// Refers to an optional status parsed from the test runner's output, before junits have been parsed.
    /// TODO(TRUNK-13911): We should populate the status for all junits, regardless of the presence of a test runner status.
    pub status: Option<JunitReportStatus>,
}

impl From<String> for JunitReportFileWithStatus {
    fn from(junit_path: String) -> Self {
        Self {
            junit_path,
            status: None,
        }
    }
}
