import AppKit
import Foundation

final class StatusMenuController {
  var ReconnectHandler: (() -> Void)?
  var QuitHandler: (() -> Void)?

  private let statusItem: NSStatusItem
  private let menu: NSMenu

  init() {
    statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
    menu = NSMenu(title: "CodexMenuBar")
    statusItem.menu = menu
    if let button = statusItem.button {
      button.title = "◎"
    }
  }

  func Render(
    turns: [ActiveTurn],
    connectionState: AppServerConnectionState,
    animationFrame: Int,
    now: Date
  ) {
    UpdateButton(
      connectionState: connectionState,
      runningCount: turns.filter { $0.status == .inProgress }.count)

    menu.removeAllItems()
    let status = NSMenuItem(
      title: HeaderTitle(
        connectionState: connectionState,
        runningCount: turns.filter { $0.status == .inProgress }.count), action: nil,
      keyEquivalent: "")
    status.isEnabled = false
    menu.addItem(status)
    menu.addItem(.separator())

    if turns.isEmpty {
      let empty = NSMenuItem(title: "No active turns", action: nil, keyEquivalent: "")
      empty.isEnabled = false
      menu.addItem(empty)
    } else {
      for turn in turns {
        let item = NSMenuItem(
          title: TurnTitle(turn: turn, animationFrame: animationFrame, now: now),
          action: nil,
          keyEquivalent: ""
        )
        item.isEnabled = false
        menu.addItem(item)
      }
    }

    menu.addItem(.separator())

    let reconnect = NSMenuItem(
      title: "Reconnect endpoints", action: #selector(OnReconnect), keyEquivalent: "r")
    reconnect.target = self
    menu.addItem(reconnect)

    let quit = NSMenuItem(title: "Quit CodexMenuBar", action: #selector(OnQuit), keyEquivalent: "q")
    quit.target = self
    menu.addItem(quit)
  }

  @objc
  private func OnReconnect() {
    ReconnectHandler?()
  }

  @objc
  private func OnQuit() {
    QuitHandler?()
  }

  private func UpdateButton(connectionState: AppServerConnectionState, runningCount: Int) {
    guard let button = statusItem.button else {
      return
    }
    switch connectionState {
    case .connected:
      if runningCount > 0 {
        button.title = "◉\(runningCount)"
      } else {
        button.title = "◎"
      }
    case .connecting, .reconnecting:
      button.title = "◌"
    case .failed:
      button.title = "⚠︎"
    case .disconnected:
      button.title = "○"
    }
  }

  private func HeaderTitle(connectionState: AppServerConnectionState, runningCount: Int) -> String {
    switch connectionState {
    case .connected:
      return "CodexMenuBar - \(runningCount) active"
    case .connecting:
      return "CodexMenuBar - connecting"
    case .reconnecting:
      return "CodexMenuBar - reconnecting"
    case .failed(let message):
      return "CodexMenuBar - error: \(message)"
    case .disconnected:
      return "CodexMenuBar - disconnected"
    }
  }

  private func TurnTitle(turn: ActiveTurn, animationFrame: Int, now: Date) -> String {
    let statusLabel = StatusLabel(turn.status)
    let elapsed = turn.ElapsedString(now: now)
    let bar = ProgressBar(turn: turn, animationFrame: animationFrame)
    let legend = LegendText(turn)
    let label = turn.latestLabel ?? "no detail"
    return
      "[\(turn.threadId.prefix(8))/\(turn.turnId)] \(statusLabel) \(elapsed) \(bar)\n  \(legend) - \(label)"
  }

  private func StatusLabel(_ status: TurnExecutionStatus) -> String {
    switch status {
    case .inProgress:
      return "Working"
    case .completed:
      return "Completed"
    case .interrupted:
      return "Interrupted"
    case .failed:
      return "Failed"
    }
  }

  private func ProgressBar(turn: ActiveTurn, animationFrame: Int) -> String {
    switch turn.status {
    case .inProgress:
      return IndeterminateBar(animationFrame: animationFrame)
    case .completed:
      return "[██████████]"
    case .interrupted:
      return "[███░░░░░░░]"
    case .failed:
      return "[██░░░░░░░░]"
    }
  }

  private func IndeterminateBar(animationFrame: Int, width: Int = 10, window: Int = 3) -> String {
    if width <= 0 {
      return "[]"
    }
    var chars = Array(repeating: Character("░"), count: width)
    let start = animationFrame % width
    for offset in 0..<window {
      let index = (start + offset) % width
      chars[index] = "█"
    }
    return "[\(String(chars))]"
  }

  private func LegendText(_ turn: ActiveTurn) -> String {
    let categories = turn.ActiveCategories()
    if categories.isEmpty {
      return "trace: none"
    }
    let parts = categories.map(CategoryLabel)
    return "trace: \(parts.joined(separator: " "))"
  }

  private func CategoryLabel(_ category: ProgressCategory) -> String {
    switch category {
    case .tool:
      return "[tool]"
    case .edit:
      return "[edit]"
    case .waiting:
      return "[wait]"
    case .network:
      return "[net]"
    case .prefill:
      return "[prefill]"
    case .reasoning:
      return "[reason]"
    case .gen:
      return "[gen]"
    }
  }
}
