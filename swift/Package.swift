// Package.swift

// swift-tools-version: 6.1
import PackageDescription

let package = Package(
    name: "IDevice",
    platforms: [
        .iOS(.v12),
        .macOS(.v11),
    ],
    products: [
        .library(
            name: "IDevice",
            targets: ["IDevice"]
        ),
    ],
    targets: [
        .binaryTarget(
            name: "IDevice",
            path: "IDevice.xcframework"
        ),
    ]
)
