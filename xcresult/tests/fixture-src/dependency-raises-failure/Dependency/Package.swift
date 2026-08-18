// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "FixtureSupport",
    platforms: [.macOS(.v13)],
    products: [
        .library(name: "FixtureSupport", targets: ["FixtureSupport"])
    ],
    targets: [
        .target(name: "FixtureSupport")
    ]
)
