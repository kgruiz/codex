import AppKit
import Foundation

final class TerminalLauncher {
  private let fileManager: FileManager
  private let workspace: NSWorkspace

  init(fileManager: FileManager = .default, workspace: NSWorkspace = .shared) {
    self.fileManager = fileManager
    self.workspace = workspace
  }

  func LaunchQuickStart() {
    _ = OpenTerminal(workingDirectory: NSHomeDirectory(), command: "codex")
  }

  func OpenTerminal(at workingDirectory: String) {
    _ = OpenTerminal(workingDirectory: workingDirectory, command: nil)
  }

  @discardableResult
  func OpenTerminal(workingDirectory: String, command: String?) -> Bool {
    do {
      let scriptUrl = try WriteScript(workingDirectory: workingDirectory, command: command)
      if workspace.open(scriptUrl) {
        return true
      }
      return OpenWithTerminalApp(scriptUrl: scriptUrl)
    } catch {
      return false
    }
  }

  func ScriptBody(workingDirectory: String, command: String?) -> String {
    var lines = [
      "#!/bin/zsh",
      "set -e",
      "cd \(ShellQuoted(workingDirectory))",
    ]

    if let command, !command.isEmpty {
      lines.append(command)
    } else {
      lines.append("exec \"${SHELL:-/bin/zsh}\" -l")
    }

    return lines.joined(separator: "\n") + "\n"
  }

  func ShellQuoted(_ value: String) -> String {
    if value.isEmpty {
      return "''"
    }

    let escaped = value.replacingOccurrences(of: "'", with: "'\\''")
    return "'\(escaped)'"
  }

  private func WriteScript(workingDirectory: String, command: String?) throws -> URL {
    let scriptUrl = fileManager.temporaryDirectory
      .appendingPathComponent("codex-menubar-\(UUID().uuidString)")
      .appendingPathExtension("command")

    let contents = ScriptBody(workingDirectory: workingDirectory, command: command)
    try contents.write(to: scriptUrl, atomically: true, encoding: .utf8)
    try fileManager.setAttributes([.posixPermissions: 0o755], ofItemAtPath: scriptUrl.path)
    return scriptUrl
  }

  private func OpenWithTerminalApp(scriptUrl: URL) -> Bool {
    let process = Process()
    process.executableURL = URL(fileURLWithPath: "/usr/bin/open")
    process.arguments = ["-a", "Terminal", scriptUrl.path]

    do {
      try process.run()
      process.waitUntilExit()
      return process.terminationStatus == 0
    } catch {
      return false
    }
  }
}
