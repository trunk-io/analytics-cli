import XCTest

// Declares no tests of its own, which is what makes it worth capturing: the method it runs
// is written nowhere in this file, so `ConcreteTests/testInheritedFails` has to report
// `BaseTests.swift`. Attributing it here would name a file the test does not appear in.
final class ConcreteTests: BaseTests {}
