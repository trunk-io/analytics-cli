/// Kills the test process from inside the dependency.
public func crashInsideDependency() -> Never {
    fatalError("the dependency crashed the test process")
}
