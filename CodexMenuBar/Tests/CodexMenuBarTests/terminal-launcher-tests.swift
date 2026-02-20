import XCTest

@testable import CodexMenuBar

final class TerminalLauncherTests: XCTestCase {
  func testScriptBodyForQuickStartIncludesCodexCommand() {
    let launcher = TerminalLauncher()
    let script = launcher.ScriptBody(workingDirectory: "/Users/test", command: "codex")

    XCTAssertTrue(script.contains("cd '/Users/test'"))
    XCTAssertTrue(script.contains("codex"))
    XCTAssertFalse(script.contains("exec \"${SHELL:-/bin/zsh}\" -l"))
  }

  func testScriptBodyForOpenTerminalStartsInteractiveShell() {
    let launcher = TerminalLauncher()
    let script = launcher.ScriptBody(workingDirectory: "/Users/test", command: nil)

    XCTAssertTrue(script.contains("cd '/Users/test'"))
    XCTAssertTrue(script.contains("exec \"${SHELL:-/bin/zsh}\" -l"))
  }

  func testShellQuotedEscapesSingleQuotes() {
    let launcher = TerminalLauncher()
    let quoted = launcher.ShellQuoted("/tmp/it's-here")

    XCTAssertEqual(quoted, "'/tmp/it'\\''s-here'")
  }
}
