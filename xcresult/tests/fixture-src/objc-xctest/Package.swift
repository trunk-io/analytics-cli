// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "ObjcXCTest",
    platforms: [.macOS(.v13)],
    targets: [
        .testTarget(name: "ObjcXCTestTests")
    ]
)
