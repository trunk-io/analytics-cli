import Testing

/// Declares a `shared()` too, in a different file, so the suite component of the JUnit
/// classname is the only thing that can tell the two apart.
@Suite struct BetaSuite {
    @Test func shared() { #expect(true) }
}
