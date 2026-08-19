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
}

/// Where a candidate file came from.
///
/// Every variant here answers "where did this failure surface", which is only ever
/// a proxy for "which file owns this test". Keeping the provenance lets a reader —
/// and later a caller that wants to vet a candidate — tell the sources apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileSource {
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
    ) -> Vec<Self> {
        [
            raised_from(failure_summary),
            source_code_location(failure_summary),
            last_stack_frame(failure_summary),
        ]
        .into_iter()
        .flatten()
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

fn last_stack_frame(
    failure_summary: &legacy_schema::ActionTestFailureSummary,
) -> Option<FileCandidate> {
    let call_stack = failure_summary
        .source_code_context
        .as_ref()?
        .call_stack
        .as_ref()?;
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
        .last()
}

#[cfg(test)]
mod tests {
    use rstest::rstest;
    use serde_json::{Value, json};

    use super::*;

    fn xc_string(value: &str) -> Value {
        json!({ "_value": value })
    }

    fn failure_summary(
        file_name: Option<&str>,
        location: Option<&str>,
        stack: &[&str],
    ) -> legacy_schema::ActionTestFailureSummary {
        serde_json::from_value(json!({
            "fileName": file_name.map(xc_string),
            "sourceCodeContext": {
                "location": { "filePath": location.map(xc_string) },
                "callStack": { "_values": stack.iter().map(|path| json!({
                    "symbolInfo": { "location": { "filePath": xc_string(path) } }
                })).collect::<Vec<_>>() }
            }
        }))
        .unwrap()
    }

    #[rstest]
    #[case::spaces_are_encoded("/repo/Tests/My Test.swift", "/repo/Tests/My%20Test.swift")]
    #[case::already_safe("/repo/Tests/Test.swift", "/repo/Tests/Test.swift")]
    fn reported_path_normalizes_once(#[case] path: &str, #[case] expected: &str) {
        assert_eq!(ReportedPath::new(path).as_str(), expected);
    }

    #[test]
    fn candidates_are_offered_in_preference_order_and_keep_their_provenance() {
        let summary = failure_summary(
            Some("/repo/Tests/Raised.swift"),
            Some("/repo/Tests/Location.swift"),
            &["/repo/Tests/Frame.swift"],
        );
        let candidates = FileCandidate::from_failure_summary(&summary);
        assert_eq!(
            candidates
                .iter()
                .map(|candidate| (candidate.path.as_str(), candidate.source))
                .collect::<Vec<_>>(),
            vec![
                ("/repo/Tests/Raised.swift", FileSource::RaisedFrom),
                ("/repo/Tests/Location.swift", FileSource::SourceCodeLocation),
                ("/repo/Tests/Frame.swift", FileSource::LastStackFrame),
            ]
        );
    }

    #[test]
    fn a_summary_offering_nothing_yields_no_candidates() {
        assert!(FileCandidate::from_failure_summary(&failure_summary(None, None, &[])).is_empty());
    }

    #[rstest]
    // Frames run innermost first, so the *last* one with source is taken.
    #[case::last_wins(&["/repo/Tests/First.swift", "/repo/Tests/Second.m"], Some("/repo/Tests/Second.m"))]
    #[case::other_languages_skipped(
        &["/repo/Tests/Real.swift", "/repo/Tests/Generated.cc", "/repo/Readme.md"],
        Some("/repo/Tests/Real.swift")
    )]
    #[case::nothing_usable(&["/repo/Tests/Generated.cc"], None)]
    fn the_stack_fallback_takes_the_outermost_swift_or_objc_frame(
        #[case] stack: &[&str],
        #[case] expected: Option<&str>,
    ) {
        let summary = failure_summary(None, None, stack);
        assert_eq!(
            FileCandidate::from_failure_summary(&summary)
                .first()
                .map(|candidate| candidate.path.as_str().to_string()),
            expected.map(String::from)
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
