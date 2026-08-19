import Testing

@Suite
struct InRepoHelperRaisesFailureTests {
    /// The helper's file is in the repo, so nothing rejects it — the test's own
    /// call-stack frame has to win on its own merits for this file to be reported.
    @Test
    func failsInsideHelper() {
        recordIssueFromHelper("recorded from a helper in the test target")
    }
}
