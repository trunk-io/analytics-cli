import XCTest

/// XCTest skips exercised by smoke-test xcodebuild (see perform_smoke_test action).
/// On Xcode 27+, skip reasons appear as `Skip Message` nodes in the xcresult bundle.
final class SmokeXCTestSkips: XCTestCase {
    func testPass() {
        XCTAssertTrue(true)
    }

    func testThrowSkip() throws {
        throw XCTSkip("runtime skip reason")
    }

    func testSkipIf() throws {
        try XCTSkipIf(true, "skip if true")
    }

    func testSkipUnless() throws {
        try XCTSkipUnless(false, "skip unless false")
    }
}
