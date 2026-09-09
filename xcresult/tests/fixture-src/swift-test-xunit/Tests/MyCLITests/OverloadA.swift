import Testing

@Suite struct OverloadSuite {
    @Test func check() {}
    @Test(arguments: [1]) func check(a: Int) { _ = a }
}
