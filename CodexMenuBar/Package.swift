// swift-tools-version: 5.10
import PackageDescription

let package = Package(
    name: "CodexMenuBar",
    platforms: [
        .macOS(.v13),
    ],
    products: [
        .executable(
            name: "CodexMenuBar",
            targets: ["CodexMenuBar"]
        ),
    ],
    targets: [
        .executableTarget(
            name: "CodexMenuBar"
        ),
    ]
)
