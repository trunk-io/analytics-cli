import XCTest

// Declares no tests of its own. The file reported for `ConcreteTests/testInheritedFails`
// has to be this one: it is what codeowners resolve from, and this is the suite that chose
// to run the test, not whoever owns `BaseTests.swift`.
final class ConcreteTests: BaseTests {}
