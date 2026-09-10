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
