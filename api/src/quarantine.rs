//! What a run needs from a quarantine lookup, whichever API answered it. The wire types stay in
//! `message`, one per API, and convert into these.

use std::collections::HashSet;

use crate::message::{GetQuarantineConfigResponse, QuarantineResolutionMode};
use crate::urls::TestCaseGuidScope;

/// The CLI's lookup of its failing tests.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QuarantineLookup {
    pub is_disabled: bool,
    /// The CLI's ids of the looked-up tests that are quarantined.
    pub quarantined_tests: Vec<String>,
    pub quarantine_resolution_mode: QuarantineResolutionMode,
    /// Set on v2, for the tests' GUID links.
    pub test_case_guid_scope: Option<TestCaseGuidScope>,
}

impl From<GetQuarantineConfigResponse> for QuarantineLookup {
    fn from(response: GetQuarantineConfigResponse) -> Self {
        Self {
            is_disabled: response.is_disabled,
            quarantined_tests: response.quarantined_tests,
            quarantine_resolution_mode: response.quarantine_resolution_mode,
            test_case_guid_scope: None,
        }
    }
}

/// The whole quarantined set, which a test runner checks each test it runs against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuarantinedTestSet {
    pub is_disabled: bool,
    pub quarantine_resolution_mode: QuarantineResolutionMode,
    pub tests: QuarantinedTests,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuarantinedTests {
    /// v1: the CLI's test ids.
    ByTestId(HashSet<String>),
    /// v2: test-case GUIDs, which a test's GUID is computed from `scope` to match.
    ByTestCaseGuid {
        scope: TestCaseGuidScope,
        guids: HashSet<String>,
    },
}

impl From<GetQuarantineConfigResponse> for QuarantinedTestSet {
    fn from(response: GetQuarantineConfigResponse) -> Self {
        Self {
            is_disabled: response.is_disabled,
            quarantine_resolution_mode: response.quarantine_resolution_mode,
            tests: QuarantinedTests::ByTestId(response.quarantined_tests.into_iter().collect()),
        }
    }
}
