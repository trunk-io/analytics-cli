import Testing

import FixtureSupport

@Suite
struct TeardownFailureTests {
    /// The test body succeeds and the dependency's trait fails it afterwards, so
    /// the test's own frame is gone by the time the failure is recorded: every
    /// file source is a dependency path and none of them may be reported.
    @Test(.teardownFailure)
    func failsAfterItsOwnFrameIsGone() {}
}
