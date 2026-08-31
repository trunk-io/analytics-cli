import Testing

@Suite struct AlphaSuite {
    @Test func shared() { #expect(true) }

    @Suite struct Inner {
        @Test func deep() { #expect(true) }
    }
}
