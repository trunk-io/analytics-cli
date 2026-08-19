// swift-tools-version: 6.0
import PackageDescription

// See `dependency-raises-failure/Package.swift` for why the dependency is a git
// URL rather than a path.
let package = Package(
    name: "CrashInDependency",
    platforms: [.macOS(.v13)],
    dependencies: [
        .package(url: "./Dependency", branch: "main")
    ],
    targets: [
        .testTarget(
            name: "CrashInDependencyTests",
            dependencies: [.product(name: "FixtureSupport", package: "Dependency")]
        )
    ]
)
