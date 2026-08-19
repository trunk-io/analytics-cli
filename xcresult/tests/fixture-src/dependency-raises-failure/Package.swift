// swift-tools-version: 6.0
import PackageDescription

// `Dependency` is referenced by git URL rather than by path so SPM checks it out
// into `DerivedData/SourcePackages/checkouts/Dependency` — the vendored location
// whose paths the fixture exists to reproduce. `regenerate.sh` turns the checked-in
// `Dependency` directory into a git repository before building.
let package = Package(
    name: "DependencyRaisesFailure",
    platforms: [.macOS(.v13)],
    dependencies: [
        .package(url: "./Dependency", branch: "main")
    ],
    targets: [
        .testTarget(
            name: "DependencyRaisesFailureTests",
            dependencies: [.product(name: "FixtureSupport", package: "Dependency")]
        )
    ]
)
