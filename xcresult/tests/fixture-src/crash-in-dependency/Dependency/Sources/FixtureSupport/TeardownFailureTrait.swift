import Testing

/// A trait that fails the test *after* its body has returned, so the test's own
/// frame is already off the stack when the issue is recorded — the shape of a
/// snapshot-verification or screenshot-diffing trait that checks its work in
/// teardown. Every file the failure summary offers is then the dependency's.
public struct TeardownFailureTrait: TestTrait, TestScoping {
    public func provideScope(
        for test: Test,
        testCase: Test.Case?,
        performing function: () async throws -> Void
    ) async throws {
        try await function()
        Issue.record(
            Comment(rawValue: "the dependency's trait failed the test after its body returned"),
            sourceLocation: SourceLocation(
                fileID: #fileID,
                filePath: #filePath,
                line: #line,
                column: #column
            )
        )
    }
}

extension Trait where Self == TeardownFailureTrait {
    public static var teardownFailure: Self { .init() }
}
