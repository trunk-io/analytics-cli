import Testing

/// Declared in its own file, and nested inside `OuterSuite`, so a suite that is a
/// child of another suite has to be visited and its file resolved independently.
extension OuterSuite {
    @Suite
    struct InnerSuite {
        @Test
        func innerPasses() {
            #expect(true)
        }

        @Test
        func innerFails() {
            #expect(Bool(false), "deliberate failure inside a nested suite")
        }
    }
}
