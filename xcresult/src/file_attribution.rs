//! Deciding which source file a failed test is reported against.
//!
//! An `.xcresult` records where a *failure was raised*. It does not record where a
//! *test is declared* — there is no per-test source location anywhere in the bundle.
//! So the file we report is inferred from the failure, and every source below is
//! answering a slightly different question than the one we are asking.
//!
//! That distinction matters because the reported file is what codeowners are
//! resolved from: a failure raised inside a helper is reported against the helper's
//! file, and the helper may belong to someone else entirely.
//!
//! [`FileSource`] names each place we look so a candidate can be traced back to
//! where it came from, rather than arriving as an anonymous `String`.

use crate::types::legacy_schema;

/// A file path in the form it is reported in the JUnit output.
///
/// Normalization happens once, here, so it cannot be forgotten at a call site or
/// applied inconsistently between two paths that are then compared.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ReportedPath(String);

impl ReportedPath {
    pub fn new(path: &str) -> Self {
        Self(path.replace(' ', "%20"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }

    /// Whether this path is vendored dependency source rather than the repo's own
    /// code: SPM's build dir (Tuist vendors into `<repo>/Tuist/.build/checkouts`),
    /// SPM checkouts under Xcode's `DerivedData/SourcePackages/checkouts`, and
    /// anything else Xcode generates under DerivedData.
    ///
    /// Reporting one of these hands the test to whoever owns the vendored
    /// directory, because that is where codeowners are resolved from.
    pub fn is_vendored_dependency(&self) -> bool {
        DEPENDENCY_PATH_SEGMENTS
            .iter()
            .any(|segment| self.0.contains(segment))
    }
}

const DEPENDENCY_PATH_SEGMENTS: [&str; 3] = ["/.build/", "/checkouts/", "/DerivedData/"];

/// Where a candidate file came from.
///
/// Every variant here answers "where did this failure surface", which is only ever
/// a proxy for "which file owns this test". Keeping the provenance lets a reader —
/// and later a caller that wants to vet a candidate — tell the sources apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileSource {
    /// The call-stack frame whose symbol names the test itself. The only source
    /// that identifies the test rather than the failure, and so the only one that
    /// is right by construction.
    TestFrame,
    /// `failureSummary.fileName`. The site the failure was raised from, which for an
    /// assertion helper is the helper's file rather than the caller's.
    RaisedFrom,
    /// `sourceCodeContext.location.filePath`. The same site as [`Self::RaisedFrom`]
    /// reported through a different field; the two usually agree.
    SourceCodeLocation,
    /// The last Swift or Objective-C frame of the failure's call stack. Frames run
    /// innermost first, so this is the outermost frame with source — typically the
    /// framework or trait that invoked the test rather than the test itself.
    LastStackFrame,
    /// `documentLocationInCreatingWorkspace` on an action-level issue summary. The
    /// only source available without fetching the per-test failure summary.
    DocumentLocation,
}

impl FileSource {
    /// Whether this source identifies the test itself, or merely where a failure
    /// surfaced. Only the latter is a guess, and only a guess needs vetting before
    /// it is reported.
    pub fn is_positive_identification(&self) -> bool {
        matches!(self, Self::TestFrame)
    }
}

/// A test as the bundle names it, and the symbols that name it in a call stack.
///
/// Swift symbolizes a test method as `Suite.testCase()` and Objective-C as
/// `-[Suite testCase]`; a closure declared inside the test is prefixed
/// (`closure #1 in Suite.testCase()`) but is still defined in the test's file. A
/// swift-testing test declared at the top level has no suite and symbolizes as the
/// bare function.
#[derive(Debug, Clone, Copy)]
pub struct TestIdentity<'a> {
    pub suite: Option<&'a str>,
    pub case: &'a str,
}

impl TestIdentity<'_> {
    /// Whether `symbol` is this test's own frame, rather than a helper it called or
    /// the framework that invoked it.
    pub fn is_named_by(&self, symbol: &str) -> bool {
        let expected = match self.suite {
            Some(suite) => vec![
                format!("{}.{}", suite, self.case),
                format!("-[{} {}]", suite, self.case.trim_end_matches("()")),
            ],
            None => vec![self.case.to_string()],
        };
        expected
            .iter()
            .any(|expected| symbol == expected || symbol.ends_with(&format!(" in {}", expected)))
    }

    /// How the action-level issue summaries key this test when Xcode records no
    /// producing target.
    pub fn fallback_key(&self) -> String {
        match self.suite {
            Some(suite) => format!("{}.{}", suite, self.case),
            None => self.case.to_string(),
        }
    }
}

/// A file we could report for a test, and the place we found it.
#[derive(Debug, Clone)]
pub struct FileCandidate {
    pub path: ReportedPath,
    pub source: FileSource,
}

impl FileCandidate {
    fn new(path: &str, source: FileSource) -> Self {
        Self {
            path: ReportedPath::new(path),
            source,
        }
    }

    /// Every file a failure summary offers, in the order we prefer them.
    ///
    /// Ordered rather than merged because the sources disagree: the raised-from
    /// fields point at where the assertion fired, the call-stack fallback at
    /// whatever frame happened to be outermost.
    pub fn from_failure_summary(
        failure_summary: &legacy_schema::ActionTestFailureSummary,
        identity: &TestIdentity,
    ) -> Vec<Self> {
        [
            test_frame(failure_summary, identity),
            raised_from(failure_summary),
            source_code_location(failure_summary),
        ]
        .into_iter()
        .flatten()
        .chain(stack_frames(failure_summary))
        .collect()
    }

    /// The file an action-level issue summary points at, with the `file://` scheme
    /// and line-number fragment stripped.
    pub fn from_issue_summary(
        failure_summary: &legacy_schema::TestFailureIssueSummary,
    ) -> Option<Self> {
        let url = failure_summary
            .document_location_in_creating_workspace
            .as_ref()?
            .url
            .as_ref()?;
        let path = url
            .value
            .replace("file://", "")
            .split('#')
            .next()
            .unwrap_or_default()
            .to_string();
        Some(Self::new(&path, FileSource::DocumentLocation))
    }
}

/// The frame that names the test, and so the test's own file.
///
/// Frames run innermost first, so this sits in the middle of the stack — helpers it
/// called below it, the framework that invoked it above — which is why taking the
/// last frame lands on a dependency.
///
/// `imageName` looks like the natural discriminator here and is not: SPM
/// dependencies are statically linked into the test bundle, so every frame reports
/// the test bundle's name.
fn test_frame(
    failure_summary: &legacy_schema::ActionTestFailureSummary,
    identity: &TestIdentity,
) -> Option<FileCandidate> {
    let call_stack = failure_summary
        .source_code_context
        .as_ref()?
        .call_stack
        .as_ref()?;
    call_stack.values.iter().find_map(|frame| {
        let symbol_info = frame.symbol_info.as_ref()?;
        if !identity.is_named_by(&symbol_info.symbol_name.as_ref()?.value) {
            return None;
        }
        let file_path = symbol_info.location.as_ref()?.file_path.as_ref()?;
        Some(FileCandidate::new(&file_path.value, FileSource::TestFrame))
    })
}

fn raised_from(failure_summary: &legacy_schema::ActionTestFailureSummary) -> Option<FileCandidate> {
    let file_name = failure_summary.file_name.as_ref()?;
    Some(FileCandidate::new(&file_name.value, FileSource::RaisedFrom))
}

fn source_code_location(
    failure_summary: &legacy_schema::ActionTestFailureSummary,
) -> Option<FileCandidate> {
    let file_path = failure_summary
        .source_code_context
        .as_ref()?
        .location
        .as_ref()?
        .file_path
        .as_ref()?;
    Some(FileCandidate::new(
        &file_path.value,
        FileSource::SourceCodeLocation,
    ))
}

/// The failure's Swift and Objective-C frames, outermost first.
///
/// Emitted as a sequence rather than a single "last frame" so that a caller
/// rejecting unusable paths lands on the outermost frame it can actually report,
/// rather than giving up because the outermost one happened to be a dependency.
fn stack_frames(failure_summary: &legacy_schema::ActionTestFailureSummary) -> Vec<FileCandidate> {
    let Some(call_stack) = failure_summary
        .source_code_context
        .as_ref()
        .and_then(|context| context.call_stack.as_ref())
    else {
        return Vec::new();
    };
    call_stack
        .values
        .iter()
        .filter_map(|frame| {
            let file_path = frame
                .symbol_info
                .as_ref()?
                .location
                .as_ref()?
                .file_path
                .as_ref()?;
            Some(FileCandidate::new(
                &file_path.value,
                FileSource::LastStackFrame,
            ))
        })
        // Frames from other languages and from generated code are not files we can
        // report a Swift or Objective-C test against.
        .filter(|candidate| {
            std::path::Path::new(candidate.path.as_str())
                .extension()
                .map(|extension| extension == "swift" || extension == "m")
                .unwrap_or(false)
        })
        .rev()
        .collect()
}

#[cfg(test)]
mod tests {
    use rstest::rstest;
    use serde_json::{Value, json};

    use super::*;

    const SUITE: &str = "SnapshotReproTests";
    const CASE: &str = "failingSnapshot()";

    fn xc_string(value: &str) -> Value {
        json!({ "_value": value })
    }

    fn failure_summary(
        file_name: Option<&str>,
        location: Option<&str>,
        stack: &[(&str, &str)],
    ) -> legacy_schema::ActionTestFailureSummary {
        serde_json::from_value(json!({
            "fileName": file_name.map(xc_string),
            "sourceCodeContext": {
                "location": { "filePath": location.map(xc_string) },
                "callStack": { "_values": stack.iter().map(|(symbol, path)| json!({
                    "symbolInfo": {
                        "symbolName": xc_string(symbol),
                        "location": { "filePath": xc_string(path) }
                    }
                })).collect::<Vec<_>>() }
            }
        }))
        .unwrap()
    }

    fn identity() -> TestIdentity<'static> {
        TestIdentity {
            suite: Some(SUITE),
            case: CASE,
        }
    }

    #[rstest]
    #[case::spaces_are_encoded("/repo/Tests/My Test.swift", "/repo/Tests/My%20Test.swift")]
    #[case::already_safe("/repo/Tests/Test.swift", "/repo/Tests/Test.swift")]
    fn reported_path_normalizes_once(#[case] path: &str, #[case] expected: &str) {
        assert_eq!(ReportedPath::new(path).as_str(), expected);
    }

    #[rstest]
    #[case::tuist_checkout("/repo/Tuist/.build/checkouts/Dep/Dep.swift", true)]
    #[case::derived_data("/repo/DerivedData/SourcePackages/checkouts/Dep/Dep.swift", true)]
    #[case::the_repos_own_code("/repo/Tests/SnapshotReproTests.swift", false)]
    fn reported_path_recognizes_vendored_sources(#[case] path: &str, #[case] expected: bool) {
        assert_eq!(ReportedPath::new(path).is_vendored_dependency(), expected);
    }

    #[rstest]
    #[case::swift_symbol("SnapshotReproTests.failingSnapshot()", true)]
    #[case::objc_symbol("-[SnapshotReproTests failingSnapshot]", true)]
    #[case::closure_inside_test("closure #1 in SnapshotReproTests.failingSnapshot()", true)]
    #[case::helper_the_test_called("assertSnapshot<A, B>(of:as:)", false)]
    #[case::same_case_name_in_another_suite("OtherTests.failingSnapshot()", false)]
    #[case::trait_that_invoked_the_test(
        "closure #1 in _SnapshotsTestTrait.provideScope(for:testCase:performing:)",
        false
    )]
    fn identity_recognizes_only_the_tests_own_frame(#[case] symbol: &str, #[case] expected: bool) {
        assert_eq!(identity().is_named_by(symbol), expected);
    }

    #[rstest]
    #[case::top_level_swift_testing_function("failingSnapshot()", true)]
    #[case::closure_inside_it("closure #1 in failingSnapshot()", true)]
    #[case::suite_scoped_symbol("SnapshotReproTests.failingSnapshot()", false)]
    fn a_suiteless_test_is_matched_by_its_bare_function(
        #[case] symbol: &str,
        #[case] expected: bool,
    ) {
        let identity = TestIdentity {
            suite: None,
            case: CASE,
        };
        assert_eq!(identity.is_named_by(symbol), expected);
    }

    #[test]
    fn candidates_are_offered_in_preference_order_and_keep_their_provenance() {
        let summary = failure_summary(
            Some("/repo/Tests/Raised.swift"),
            Some("/repo/Tests/Location.swift"),
            &[
                ("helper()", "/repo/Tests/Inner.swift"),
                (
                    "SnapshotReproTests.failingSnapshot()",
                    "/repo/Tests/Own.swift",
                ),
                ("framework()", "/repo/Tests/Outer.swift"),
            ],
        );
        assert_eq!(
            FileCandidate::from_failure_summary(&summary, &identity())
                .iter()
                .map(|candidate| (candidate.path.as_str(), candidate.source))
                .collect::<Vec<_>>(),
            vec![
                ("/repo/Tests/Own.swift", FileSource::TestFrame),
                ("/repo/Tests/Raised.swift", FileSource::RaisedFrom),
                ("/repo/Tests/Location.swift", FileSource::SourceCodeLocation),
                // Frames run innermost first, so they are offered outermost first.
                ("/repo/Tests/Outer.swift", FileSource::LastStackFrame),
                ("/repo/Tests/Own.swift", FileSource::LastStackFrame),
                ("/repo/Tests/Inner.swift", FileSource::LastStackFrame),
            ]
        );
    }

    #[test]
    fn a_summary_offering_nothing_yields_no_candidates() {
        let summary = failure_summary(None, None, &[]);
        assert!(FileCandidate::from_failure_summary(&summary, &identity()).is_empty());
    }

    #[rstest]
    #[case::other_languages_skipped(
        &[("a", "/repo/Tests/Real.swift"), ("b", "/repo/Tests/Generated.cc"), ("c", "/repo/Readme.md")],
        vec!["/repo/Tests/Real.swift"]
    )]
    #[case::nothing_usable(&[("a", "/repo/Tests/Generated.cc")], vec![])]
    fn only_swift_and_objc_frames_are_offered(
        #[case] stack: &[(&str, &str)],
        #[case] expected: Vec<&str>,
    ) {
        let summary = failure_summary(None, None, stack);
        assert_eq!(
            stack_frames(&summary)
                .iter()
                .map(|candidate| candidate.path.as_str())
                .collect::<Vec<_>>(),
            expected
        );
    }

    #[rstest]
    #[case::scheme_and_fragment_stripped(
        Some("file:///repo/Tests/Test.swift#EndingLineNumber=8"),
        Some("/repo/Tests/Test.swift")
    )]
    #[case::spaces_encoded(
        Some("file:///repo/Tests/My Test.swift"),
        Some("/repo/Tests/My%20Test.swift")
    )]
    #[case::no_document_location(None, None)]
    fn an_issue_summary_yields_a_cleaned_document_location(
        #[case] url: Option<&str>,
        #[case] expected: Option<&str>,
    ) {
        let summary = serde_json::from_value(json!({
            "documentLocationInCreatingWorkspace": { "url": url.map(xc_string) }
        }))
        .unwrap();
        let candidate = FileCandidate::from_issue_summary(&summary);
        assert_eq!(candidate.as_ref().map(|c| c.path.as_str()), expected);
        if let Some(candidate) = candidate {
            assert_eq!(candidate.source, FileSource::DocumentLocation);
        }
    }
}
