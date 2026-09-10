// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "ObjcCategory",
    platforms: [.macOS(.v13)],
    targets: [
        .testTarget(name: "ObjcCategoryTests")
    ]
)
