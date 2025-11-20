import Testing

@Suite("Smoke Tests")
struct SmokeTestTests {
    
    @Test("Test that always fails for quarantining validation")
    func testAlwaysFails() {
        #expect(Bool(false), "This test always fails to validate quarantining works with xcresult")
    }
}

