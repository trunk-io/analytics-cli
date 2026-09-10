// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "NestedAndPassing",
    platforms: [.macOS(.v13)],
    targets: [
        .testTarget(name: "NestedAndPassingTests")
    ]
)
