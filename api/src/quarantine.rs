//! What a run needs from a quarantine lookup. The wire types stay in `message` and convert into
//! these.

use std::collections::HashSet;

use crate::message::QuarantineResolutionMode;
use crate::urls::TestCaseGuidScope;

/// The CLI's lookup of its failing tests.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QuarantineLookup {
    pub is_disabled: bool,
    /// The CLI's ids of the looked-up tests that are quarantined.
    pub quarantined_tests: Vec<String>,
    pub quarantine_resolution_mode: QuarantineResolutionMode,
    /// For the tests' GUID links. `None` when there was nothing to look up.
    pub test_case_guid_scope: Option<TestCaseGuidScope>,
}

/// The whole quarantined set, which a test runner checks each test it runs against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuarantinedTestSet {
    pub is_disabled: bool,
    pub quarantine_resolution_mode: QuarantineResolutionMode,
    /// A test is quarantined when the GUID computed from `scope` and its own test id is in
    /// `guids`, the quarantined tests' `testId`s.
    pub scope: TestCaseGuidScope,
    pub guids: HashSet<String>,
}
