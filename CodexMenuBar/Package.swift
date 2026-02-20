// swift-tools-version: 5.10
import PackageDescription

let package = Package(
  name: "CodexMenuBar",
  platforms: [
    .macOS(.v14)
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
        .copy("Resources/svgs/codex-app.svg"),
        .copy("Resources/svgs/codex.svg"),
      ]
    ),
    .testTarget(
      name: "CodexMenuBarTests",
      dependencies: ["CodexMenuBar"]
    ),
  ]
)
