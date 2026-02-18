// swift-tools-version: 5.10
import PackageDescription

let package = Package(
  name: "CodexMenuBar",
  platforms: [
    .macOS(.v13)
  ],
  products: [
    .executable(
      name: "CodexMenuBar",
      targets: ["CodexMenuBar"]
    )
  ],
  targets: [
    .executableTarget(
      name: "CodexMenuBar",
      resources: [
        .copy("Resources/codex-app.svg"),
        .copy("Resources/codex.svg"),
      ]
    ),
    .testTarget(
      name: "CodexMenuBarTests",
      dependencies: ["CodexMenuBar"]
    ),
  ]
)
