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
    _ = animationFrame
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
        let item = NSMenuItem(title: "", action: nil, keyEquivalent: "")
        item.view = TurnMenuRowView(turn: turn, now: now)
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
}
