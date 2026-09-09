import Testing

@Suite
struct OuterSuite {
    @Test
    func outerPasses() {
        #expect(1 + 1 == 2)
    }
}

@Test
func topLevelPasses() {
    #expect(true)
}
