// swift-tools-version: 6.2

import PackageDescription

let package = Package(
    name: "KeduMonitor",
    platforms: [
        .macOS(.v14),
    ],
    products: [
        .executable(name: "KeduMonitor", targets: ["KeduMonitor"]),
    ],
    targets: [
        .executableTarget(name: "KeduMonitor"),
    ]
)
