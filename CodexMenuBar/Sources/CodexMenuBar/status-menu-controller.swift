import AppKit
import Foundation

final class StatusMenuController: NSObject, NSMenuDelegate {
  var ReconnectHandler: (() -> Void)?
  var ReconnectEndpointHandler: ((String) -> Void)?
  var QuitHandler: (() -> Void)?

  private let statusItem: NSStatusItem
  private let menu: NSMenu
  private let statusIcon: NSImage?
  private var expandedEndpointIds: Set<String> = []
  private var expandedRunKeysByEndpoint: [String: Set<String>] = [:]
  private var cachedEndpointRows: [EndpointRow] = []
  private var cachedConnectionState: AppServerConnectionState = .disconnected
  private var cachedAnimationFrame = 0

  override init() {
    statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
    menu = NSMenu(title: "CodexMenuBar")
    statusIcon = Self.LoadStatusIcon()
    super.init()
    menu.delegate = self
    statusItem.menu = menu
    if let button = statusItem.button {
      button.title = ""
      button.image = statusIcon
      button.imagePosition = .imageLeading
    }
  }

  func Render(
    endpointRows: [EndpointRow],
    connectionState: AppServerConnectionState,
    animationFrame: Int,
    now: Date
  ) {
    cachedEndpointRows = endpointRows
    cachedConnectionState = connectionState
    cachedAnimationFrame = animationFrame

    let endpointIds = Set(endpointRows.map(\.endpointId))
    expandedEndpointIds = expandedEndpointIds.intersection(endpointIds)
    expandedRunKeysByEndpoint = expandedRunKeysByEndpoint.filter { endpointIds.contains($0.key) }

    let runningCount = endpointRows.filter { $0.activeTurn != nil }.count
    UpdateButton(connectionState: connectionState, runningCount: runningCount)

    menu.removeAllItems()

    // Header
    let headerItem = NSMenuItem(
      title: HeaderTitle(connectionState: connectionState, runningCount: runningCount),
      action: nil, keyEquivalent: "")
    headerItem.isEnabled = false
    menu.addItem(headerItem)
    menu.addItem(.separator())

    // Endpoint rows
    if endpointRows.isEmpty {
      let emptyItem = NSMenuItem(
        title: "No active Codex sessions", action: nil, keyEquivalent: "")
      emptyItem.isEnabled = false
      menu.addItem(emptyItem)

      if connectionState == .connected || connectionState == .connecting {
        let hintItem = NSMenuItem(
          title: "Run codex in a terminal to start a session",
          action: nil, keyEquivalent: "")
        hintItem.isEnabled = false
        menu.addItem(hintItem)
      }
    } else {
      for endpointRow in endpointRows {
        let item = NSMenuItem(title: "", action: nil, keyEquivalent: "")
        item.view = TurnMenuRowView(
          endpointRow: endpointRow,
          now: now,
          isExpanded: expandedEndpointIds.contains(endpointRow.endpointId),
          expandedRunKeys: expandedRunKeysByEndpoint[endpointRow.endpointId] ?? [],
          onToggle: { [weak self] endpointId in
            guard let self else { return }
            if self.expandedEndpointIds.contains(endpointId) {
              self.expandedEndpointIds.remove(endpointId)
            } else {
              self.expandedEndpointIds.insert(endpointId)
            }
            self.Render(
              endpointRows: self.cachedEndpointRows,
              connectionState: self.cachedConnectionState,
              animationFrame: self.cachedAnimationFrame,
              now: Date())
          },
          onToggleHistoryRun: { [weak self] endpointId, runKey in
            guard let self else { return }
            var expandedRunKeys = self.expandedRunKeysByEndpoint[endpointId] ?? []
            if expandedRunKeys.contains(runKey) {
              expandedRunKeys.remove(runKey)
            } else {
              expandedRunKeys.insert(runKey)
            }
            self.expandedRunKeysByEndpoint[endpointId] = expandedRunKeys
            self.Render(
              endpointRows: self.cachedEndpointRows,
              connectionState: self.cachedConnectionState,
              animationFrame: self.cachedAnimationFrame,
              now: Date())
          },
          onReconnectEndpoint: { [weak self] endpointId in
            self?.ReconnectEndpointHandler?(endpointId)
          })
        item.isEnabled = true
        menu.addItem(item)
      }
    }

    menu.addItem(.separator())

    // Rate limits footer
    if let rateLimits = endpointRows.first(where: { $0.rateLimits != nil })?.rateLimits {
      if let remaining = rateLimits.remaining, let limit = rateLimits.limit {
        var rateLimitText = "Rate: \(remaining)/\(limit) remaining"
        if let resetsAt = rateLimits.resetsAt {
          let seconds = max(0, Int(resetsAt.timeIntervalSince(now)))
          if seconds > 0 {
            let minutes = seconds / 60
            let secs = seconds % 60
            if minutes > 0 {
              rateLimitText += ", resets in \(minutes)m \(secs)s"
            } else {
              rateLimitText += ", resets in \(secs)s"
            }
          }
        }
        let rateLimitItem = NSMenuItem(
          title: rateLimitText, action: nil, keyEquivalent: "")
        rateLimitItem.isEnabled = false
        menu.addItem(rateLimitItem)
        menu.addItem(.separator())
      }
    }

    // Actions
    let reconnect = NSMenuItem(
      title: "Reconnect endpoints", action: #selector(OnReconnect), keyEquivalent: "r")
    reconnect.target = self
    menu.addItem(reconnect)

    let quit = NSMenuItem(
      title: "Quit CodexMenuBar", action: #selector(OnQuit), keyEquivalent: "q")
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

  func menuDidClose(_ menu: NSMenu) {
    expandedEndpointIds.removeAll()
    expandedRunKeysByEndpoint.removeAll()
  }

  private func UpdateButton(connectionState: AppServerConnectionState, runningCount: Int) {
    guard let button = statusItem.button else { return }

    if let statusIcon {
      button.image = statusIcon
      button.imagePosition = .imageLeading
      switch connectionState {
      case .connected:
        button.title = runningCount > 0 ? "\(runningCount)" : ""
      case .connecting, .reconnecting:
        button.title = "…"
      case .failed:
        button.title = "!"
      case .disconnected:
        button.title = ""
      }
      return
    }

    switch connectionState {
    case .connected:
      button.title = runningCount > 0 ? "◉\(runningCount)" : "◎"
    case .connecting, .reconnecting:
      button.title = "◌"
    case .failed:
      button.title = "⚠︎"
    case .disconnected:
      button.title = "○"
    }
  }

  private static func LoadStatusIcon() -> NSImage? {
    let bundle = Bundle.module
    let iconUrls = [
      bundle.url(forResource: "codex-app", withExtension: "svg"),
      bundle.url(forResource: "codex-app", withExtension: "svg", subdirectory: "svgs"),
    ]
    for iconUrl in iconUrls {
      guard let url = iconUrl, let image = NSImage(contentsOf: url) else { continue }
      image.isTemplate = true
      image.size = NSSize(width: 18, height: 18)
      return image
    }
    return nil
  }

  private func HeaderTitle(connectionState: AppServerConnectionState, runningCount: Int) -> String {
    switch connectionState {
    case .connected:
      if runningCount == 0 {
        return "Codex — connected"
      }
      return "Codex — \(runningCount) active"
    case .connecting:
      return "Codex — connecting…"
    case .reconnecting:
      return "Codex — reconnecting…"
    case .failed(let message):
      return "Codex — error: \(message)"
    case .disconnected:
      return "Codex — disconnected"
    }
  }
}
