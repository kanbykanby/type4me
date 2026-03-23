// swift-tools-version: 6.2
import PackageDescription

let package = Package(
    name: "Type4Me",
    platforms: [.macOS(.v14)],
    targets: [
        .executableTarget(
            name: "Type4Me",
            path: "Type4Me"
        ),
        .testTarget(
            name: "Type4MeTests",
            dependencies: ["Type4Me"],
            path: "Type4MeTests"
        ),
    ]
)
