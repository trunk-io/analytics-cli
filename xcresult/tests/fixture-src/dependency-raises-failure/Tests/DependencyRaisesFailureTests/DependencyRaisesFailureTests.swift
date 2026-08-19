import Testing

import FixtureSupport

@Suite
struct DependencyRaisesFailureTests {
    /// Every file the failure summary offers — `fileName`, the source code
    /// context's location, and the innermost call-stack frame — points into the
    /// dependency's checkout. Only the test's own call-stack frame names this file.
    @Test
    func failsInsideDependency() {
        recordIssueFromDependency("recorded from the dependency's own source")
    }
}
