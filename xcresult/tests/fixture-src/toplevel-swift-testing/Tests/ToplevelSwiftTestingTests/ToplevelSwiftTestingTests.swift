import Testing

/// Declared at the top level rather than in a `@Suite`, so it symbolicates as the
/// bare function `failsInsideHelperWithoutASuite()` with no suite to qualify it.
@Test
func failsInsideHelperWithoutASuite() {
    recordIssueFromHelper("raised from the in-repo helper")
}
