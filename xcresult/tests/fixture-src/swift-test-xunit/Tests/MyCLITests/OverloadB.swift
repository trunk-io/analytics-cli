import Testing

/// Same suite, different file, differing from `check(a:)` only by the argument label.
extension OverloadSuite {
    @Test(arguments: [2]) func check(b: Int) { _ = b }
}
