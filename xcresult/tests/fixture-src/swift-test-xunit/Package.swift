// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "MyCLI",
    targets: [
        .target(name: "MyCLI"),
        .testTarget(name: "MyCLITests", dependencies: ["MyCLI"]),
    ]
)
