import XCTest

/// Overrides it, so its declaration is here.
final class ChildBTests: BaseTests {
    override func testInherited() { XCTAssertTrue(true) }
}
