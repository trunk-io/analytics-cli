// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "smoke-test",
    platforms: [
        .macOS(.v15)
    ],
    targets: [
        .testTarget(
            name: "SmokeTestTests",
            path: "Tests",
            swiftSettings: [
                .enableUpcomingFeature("BareSlashRegexLiterals"),
                .enableUpcomingFeature("ConciseMagicFile"),
                .enableUpcomingFeature("ForwardTrailingClosures"),
                .enableUpcomingFeature("ImplicitOpenExistentials"),
                .enableUpcomingFeature("StrictConcurrency"),
            ]
        )
    ]
)

