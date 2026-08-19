// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "ToplevelSwiftTesting",
    platforms: [.macOS(.v13)],
    targets: [
        .testTarget(name: "ToplevelSwiftTestingTests")
    ]
)
