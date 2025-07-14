use bazel_bep::types::build_event_stream::TestStatus;
use chrono::{DateTime, Utc};
#[cfg(feature = "pyo3")]
use pyo3::prelude::*;
#[cfg(feature = "pyo3")]
use pyo3_stub_gen::derive::gen_stub_pyclass_enum;
use serde::{Deserialize, Serialize};
#[cfg(feature = "wasm")]
use tsify_next::Tsify;
#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize, Default, Hash)]
#[cfg_attr(feature = "pyo3", gen_stub_pyclass_enum, pyclass(eq, eq_int))]
#[cfg_attr(feature = "wasm", derive(Tsify))]
pub enum TestRunnerReportStatus {
    #[default]
    Passed,
    Failed,
    Flaky,
}

impl TryFrom<TestStatus> for TestRunnerReportStatus {
    type Error = anyhow::Error;

    fn try_from(status: TestStatus) -> Result<Self, Self::Error> {
        match status {
            TestStatus::Passed => Ok(TestRunnerReportStatus::Passed),
            TestStatus::Failed => Ok(TestRunnerReportStatus::Failed),
            TestStatus::Flaky => Ok(TestRunnerReportStatus::Flaky),
            _ => Err(anyhow::anyhow!("Unknown test status: {:?}", status)),
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash)]
pub struct TestRunnerReport {
    pub status: TestRunnerReportStatus,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub label: Option<String>,
}

/// Encapsulates the glob path for a junit and, if applicable, the flakiness already
/// assigned by the user's test runner. See bazel_bep/parser.rs for more.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JunitReportFileWithTestRunnerReport {
    /// Path or glob pattern to the junit file.
    pub junit_path: String,
    /// Refers to an optional status parsed from the test runner's output, before junits have been parsed.
    /// TODO(TRUNK-13911): We should populate the status for all junits, regardless of the presence of a test runner status.
    pub test_runner_report: Option<TestRunnerReport>,
}

impl From<String> for JunitReportFileWithTestRunnerReport {
    fn from(junit_path: String) -> Self {
        Self {
            junit_path,
            test_runner_report: None,
        }
    }
}
