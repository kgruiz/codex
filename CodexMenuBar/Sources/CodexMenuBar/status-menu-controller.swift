import AppKit
import Observation
import SwiftUI

final class StatusMenuController: NSObject, NSPopoverDelegate {
  var ReconnectHandler: (() -> Void)?
  var ReconnectEndpointHandler: ((String) -> Void)?
  var QuickStartHandler: (() -> Void)?
  var OpenTerminalHandler: ((String) -> Void)?
  var QuitHandler: (() -> Void)?

  private let model: MenuBarViewModel
  private let statusItem: NSStatusItem
  private let statusIcon: NSImage?
  private let popover: NSPopover

  init(model: MenuBarViewModel) {
    self.model = model
    statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
    statusIcon = Self.LoadStatusIcon()
    popover = NSPopover()
    super.init()

    popover.behavior = .transient
    popover.delegate = self
    popover.contentSize = NSSize(width: 460, height: 580)
    popover.contentViewController = NSHostingController(
      rootView: StatusDropdownView(
        model: model,
        onReconnectAll: { [weak self] in self?.ReconnectHandler?() },
        onReconnectEndpoint: { [weak self] endpointId in
          self?.ReconnectEndpointHandler?(endpointId)
        },
        onQuickStart: { [weak self] in self?.QuickStartHandler?() },
        onOpenTerminal: { [weak self] workingDirectory in
          self?.OpenTerminalHandler?(workingDirectory)
        },
        onQuit: { [weak self] in self?.QuitHandler?() }
      ))

    if let button = statusItem.button {
      button.title = ""
      button.image = statusIcon
      button.imagePosition = .imageLeading
      button.target = self
      button.action = #selector(OnStatusItemPressed)
    }

    UpdateButton()
    ObserveModel()
  }

  @objc
  private func OnStatusItemPressed() {
    guard let button = statusItem.button else {
      return
    }

    if popover.isShown {
      popover.performClose(nil)
    } else {
      popover.show(relativeTo: button.bounds, of: button, preferredEdge: .minY)
    }
  }

  func popoverDidClose(_ notification: Notification) {
    _ = notification
    model.ClearExpandedState()
  }

  private func ObserveModel() {
    withObservationTracking {
      _ = model.connectionState
      _ = model.runningCount
    } onChange: { [weak self] in
      DispatchQueue.main.async {
        self?.UpdateButton()
        self?.ObserveModel()
      }
    }
  }

  private func UpdateButton() {
    guard let button = statusItem.button else { return }

    if let statusIcon {
      button.image = statusIcon
      button.imagePosition = .imageLeading
      switch model.connectionState {
      case .connected:
        button.title = model.runningCount > 0 ? "\(model.runningCount)" : ""
      case .connecting, .reconnecting:
        button.title = "..."
      case .failed:
        button.title = "!"
      case .disconnected:
        button.title = ""
      }
      return
    }

    switch model.connectionState {
    case .connected:
      button.title = model.runningCount > 0 ? "◉\(model.runningCount)" : "◎"
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
}

private struct StatusDropdownView: View {
  @Bindable var model: MenuBarViewModel

  let onReconnectAll: () -> Void
  let onReconnectEndpoint: (String) -> Void
  let onQuickStart: () -> Void
  let onOpenTerminal: (String) -> Void
  let onQuit: () -> Void

  var body: some View {
    VStack(alignment: .leading, spacing: 10) {
      HStack(alignment: .center, spacing: 8) {
        Text(model.headerTitle)
          .font(.headline)
          .lineLimit(1)

        Spacer(minLength: 4)

        if let warningText = model.lowRateLimitWarningText {
          Label(warningText, systemImage: "exclamationmark.triangle.fill")
            .font(.caption)
            .foregroundStyle(.orange)
            .lineLimit(1)
        }
      }

      Divider()

      if model.endpointRows.isEmpty {
        VStack(alignment: .leading, spacing: 6) {
          Text("No active Codex sessions")
            .font(.subheadline)
            .foregroundStyle(.secondary)

          if model.connectionState == .connected || model.connectionState == .connecting {
            Text("Run codex in a terminal to start a session")
              .font(.caption)
              .foregroundStyle(.secondary)
          }

          Button(action: onQuickStart) {
            Label("Quick Start", systemImage: "play.fill")
          }
          .buttonStyle(.borderedProminent)
          .controlSize(.small)
        }
      } else {
        ScrollView {
          LazyVStack(spacing: 8) {
            ForEach(model.endpointRows, id: \.endpointId) { endpointRow in
              TurnMenuRowView(
                endpointRow: endpointRow,
                now: model.now,
                isExpanded: model.expandedEndpointIds.contains(endpointRow.endpointId),
                expandedRunKeys: model.expandedRunKeysByEndpoint[endpointRow.endpointId] ?? [],
                onToggle: { model.ToggleEndpoint(endpointRow.endpointId) },
                onToggleHistoryRun: { runKey in
                  model.ToggleRun(endpointId: endpointRow.endpointId, runKey: runKey)
                },
                isFilesExpanded: model.IsSectionExpanded(
                  endpointId: endpointRow.endpointId, section: .files),
                isCommandsExpanded: model.IsSectionExpanded(
                  endpointId: endpointRow.endpointId, section: .commands),
                isPastRunsExpanded: model.IsSectionExpanded(
                  endpointId: endpointRow.endpointId, section: .pastRuns),
                onToggleFiles: {
                  model.ToggleSection(endpointId: endpointRow.endpointId, section: .files)
                },
                onToggleCommands: {
                  model.ToggleSection(endpointId: endpointRow.endpointId, section: .commands)
                },
                onTogglePastRuns: {
                  model.ToggleSection(endpointId: endpointRow.endpointId, section: .pastRuns)
                },
                onReconnectEndpoint: { onReconnectEndpoint(endpointRow.endpointId) },
                onOpenInTerminal: { cwd in onOpenTerminal(cwd) }
              )
            }
          }
          .padding(.vertical, 2)
        }
        .frame(maxHeight: 420)
      }

      if let rateLimits = model.activeRateLimitInfo,
        let remaining = rateLimits.remaining,
        let limit = rateLimits.limit
      {
        Divider()

        Text(RateLimitText(rateLimits: rateLimits, remaining: remaining, limit: limit))
          .font(.caption)
          .foregroundStyle(.secondary)
      }

      Divider()

      HStack(spacing: 8) {
        Button("Reconnect endpoints", action: onReconnectAll)
        Spacer()
        Button("Quit CodexMenuBar", action: onQuit)
      }
    }
    .padding(12)
    .frame(width: 440)
  }

  private func RateLimitText(rateLimits: RateLimitInfo, remaining: Int, limit: Int) -> String {
    var text = "Rate: \(remaining)/\(limit) remaining"

    if let resetsAt = rateLimits.resetsAt {
      let seconds = max(0, Int(resetsAt.timeIntervalSince(model.now)))
      if seconds > 0 {
        let minutes = seconds / 60
        let secs = seconds % 60
        if minutes > 0 {
          text += ", resets in \(minutes)m \(secs)s"
        } else {
          text += ", resets in \(secs)s"
        }
      }
    }

    return text
  }
}
