import Testing

@Suite struct ParamSuite {
    @Test(arguments: [1, 2, 3])
    func squares(n: Int) { #expect(n * n >= n) }

    @Test(arguments: ["a", "b"], [true, false])
    func pairs(s: String, flag: Bool) { #expect(!s.isEmpty || flag) }
}
