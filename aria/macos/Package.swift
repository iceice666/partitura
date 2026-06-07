// swift-tools-version: 6.0

import PackageDescription

let package = Package(
    name: "AriaMacOS",
    platforms: [
        .macOS(.v15)
    ],
    products: [
        .executable(name: "Aria", targets: ["Aria"])
    ],
    dependencies: [
        .package(url: "https://github.com/davidstump/SwiftPhoenixClient", from: "5.0.0")
    ],
    targets: [
        .executableTarget(
            name: "Aria",
            dependencies: [
                .product(name: "SwiftPhoenixClient", package: "SwiftPhoenixClient")
            ],
            path: ".",
            exclude: ["Package.swift", "SMOKE.md"],
            sources: ["Sources/Aria"],
            resources: [
                .process("Resources")
            ],
            swiftSettings: [
                .enableUpcomingFeature("StrictConcurrency")
            ]
        )
    ]
)
