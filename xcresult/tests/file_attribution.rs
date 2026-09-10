//! `file_attribution`'s public surface: which file a failure summary offers, in which
//! order, and how a path is normalised on the way out.

use rstest::rstest;
use serde_json::{Value, json};
use xcresult::file_attribution::{FileCandidate, FileSource, ReportedPath, TestIdentity};
use xcresult::types::legacy_schema;

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
fn a_suiteless_test_is_matched_by_its_bare_function(#[case] symbol: &str, #[case] expected: bool) {
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
    // With no `fileName` and no location, every candidate offered is a stack frame,
    // so this reaches the same filtering through the public entry point.
    let summary = failure_summary(None, None, stack);
    assert_eq!(
        FileCandidate::from_failure_summary(&summary, &identity())
            .iter()
            .inspect(|candidate| assert_eq!(candidate.source, FileSource::LastStackFrame))
            .map(|candidate| candidate.path.as_str())
            .collect::<Vec<_>>(),
        expected
    );
}

// Apple declares `SortedKeyValueArrayPair.value` as `SchemaSerializable`, a type
// the format description never defines, so the generator cannot model it and drops
// the property. The data still carries it, and an object that is both missing the
// property and declared exhaustive fails to deserialize — which silently disabled
// the whole experimental path for any bundle with test attachments.
#[test]
fn a_summary_parses_despite_properties_the_schema_cannot_model() {
    let summary: legacy_schema::ActionTestPlanRunSummaries = serde_json::from_value(json!({
        "failureSummaries": { "_values": [{
            "fileName": xc_string("/repo/Tests/SnapshotReproTests.swift"),
            "attachments": { "_values": [{
                "userInfo": { "storage": { "_values": [{
                    "_type": { "_name": "SortedKeyValueArrayPair" },
                    "key": xc_string("Encoding"),
                    "value": xc_string("{ XCTImageEncodingCompressionQualityKey = 0.7; }")
                }] } }
            }] }
        }] }
    }))
    .expect("a summary carrying attachment metadata must still deserialize");
    let failure_summary = &summary.failure_summaries.unwrap().values[0];
    assert_eq!(
        FileCandidate::from_failure_summary(failure_summary, &identity())
            .first()
            .map(|candidate| candidate.path.as_str().to_string()),
        Some(String::from("/repo/Tests/SnapshotReproTests.swift"))
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
