import XCTest

import FixtureSupport

final class CrashInDependencyTests: XCTestCase {
    /// The process dies inside the dependency, so the failure summary Xcode
    /// records has no file at all — not even a wrong one.
    func testCrashesInsideDependency() {
        crashInsideDependency()
    }
}
