import XCTest

// XCTest runs every `test*` method it finds on a concrete subclass, including the ones
// only this base class declares. So `testInheritedFails` runs twice: once as
// `BaseTests/testInheritedFails`, and once as `ConcreteTests/testInheritedFails`.
class BaseTests: XCTestCase {
    func testInheritedFails() {
        XCTFail("declared on the base class, run under whichever suite reported it")
    }
}
