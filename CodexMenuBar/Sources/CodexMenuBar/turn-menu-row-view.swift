import AppKit
import Foundation

// MARK: - TurnMenuRowView

final class TurnMenuRowView: NSView {
  private static let rowWidth: CGFloat = 420
  private static let collapsedRowHeight: CGFloat = 70
  private static let maxExpandedHeight: CGFloat = 500

  private let endpointRow: EndpointRow
  private let isExpanded: Bool
  private let onToggle: ((String) -> Void)?
  private let onReconnectEndpoint: ((String) -> Void)?
  private let onOpenWorkspace: ((String) -> Void)?

  // Collapsed header
  private let statusDot = NSView()
  private let nameLabel = NSTextField(labelWithString: "")
  private let elapsedLabel = NSTextField(labelWithString: "")
  private let chevronLabel = NSTextField(labelWithString: "")
  private let detailLabel = NSTextField(labelWithString: "")
  private let barView = TimelineBarView()

  // Hover card for timeline segments
  private let hoverCard = NSView()
  private let hoverColorSwatch = NSView()
  private let hoverLabel = NSTextField(labelWithString: "")

  // Expanded content
  private let expandedScroll = NSScrollView()
  private let expandedDocView = NSView()

  // Expanded section views
  private let gitModelLabel = NSTextField(labelWithString: "")
  private let tokenBarView = TokenUsageBarView()
  private let tokenDetailLabel = NSTextField(labelWithString: "")
  private let errorCard = NSView()
  private let errorLabel = NSTextField(labelWithString: "")
  private let planTitleLabel = NSTextField(labelWithString: "")
  private let planContentLabel = NSTextField(labelWithString: "")
  private let filesTitleLabel = NSTextField(labelWithString: "")
  private let filesContentLabel = NSTextField(labelWithString: "")
  private let commandsTitleLabel = NSTextField(labelWithString: "")
  private let commandsContentLabel = NSTextField(labelWithString: "")
  private let cwdLabel = NSTextField(labelWithString: "")
  private let openFinderButton = NSButton(title: "Open in Finder", target: nil, action: nil)
  private let reconnectButton = NSButton(title: "Reconnect", target: nil, action: nil)
  private let historyTitleLabel = NSTextField(labelWithString: "")
  private let historyScrollView = NSScrollView()
  private let historyDocumentView = NSView()
  private let historyEmptyLabel = NSTextField(labelWithString: "No past runs yet")
  private var historyRunViews: [RunHistoryRowView] = []

  private var defaultDetailText = "No detail"
  private var barVisible = true
  private var isHovering = false
  private var rowTrackingArea: NSTrackingArea?
  private var computedExpandedHeight: CGFloat = 0

  init(
    endpointRow: EndpointRow,
    now: Date,
    isExpanded: Bool,
    onToggle: ((String) -> Void)?,
    onReconnectEndpoint: ((String) -> Void)?,
    onOpenWorkspace: ((String) -> Void)? = nil
  ) {
    self.endpointRow = endpointRow
    self.isExpanded = isExpanded
    self.onToggle = onToggle
    self.onReconnectEndpoint = onReconnectEndpoint
    self.onOpenWorkspace = onOpenWorkspace
    super.init(frame: NSRect(x: 0, y: 0, width: Self.rowWidth, height: Self.collapsedRowHeight))
    ConfigureViews()
    Update(now: now)
    let height = isExpanded
      ? Self.collapsedRowHeight + computedExpandedHeight
      : Self.collapsedRowHeight
    frame.size.height = height
  }

  @available(*, unavailable)
  required init?(coder: NSCoder) {
    fatalError("init(coder:) has not been implemented")
  }

  override var intrinsicContentSize: NSSize {
    let height = isExpanded
      ? Self.collapsedRowHeight + computedExpandedHeight
      : Self.collapsedRowHeight
    return NSSize(width: Self.rowWidth, height: height)
  }

  // MARK: - Interaction

  override func mouseDown(with event: NSEvent) {
    if isExpanded {
      let pointInSelf = convert(event.locationInWindow, from: nil)
      let pointInExpanded = expandedScroll.convert(pointInSelf, from: self)
      if expandedScroll.frame.contains(pointInSelf) {
        let pointInDoc = expandedDocView.convert(pointInExpanded, from: expandedScroll)
        if openFinderButton.frame.contains(pointInDoc) {
          OnOpenFinderPressed()
          return
        }
        if reconnectButton.frame.contains(pointInDoc) {
          OnReconnectPressed()
          return
        }
        if historyScrollView.frame.contains(pointInDoc) {
          super.mouseDown(with: event)
          return
        }
      }
    }
    onToggle?(endpointRow.endpointId)
  }

  override func updateTrackingAreas() {
    super.updateTrackingAreas()
    if let rowTrackingArea {
      removeTrackingArea(rowTrackingArea)
    }
    let area = NSTrackingArea(
      rect: bounds,
      options: [.activeAlways, .mouseEnteredAndExited, .inVisibleRect],
      owner: self,
      userInfo: nil)
    addTrackingArea(area)
    rowTrackingArea = area
  }

  override func mouseEntered(with event: NSEvent) {
    isHovering = true
    needsDisplay = true
  }

  override func mouseExited(with event: NSEvent) {
    isHovering = false
    needsDisplay = true
  }

  override func draw(_ dirtyRect: NSRect) {
    super.draw(dirtyRect)
    if isHovering {
      NSColor.controlAccentColor.withAlphaComponent(0.06).setFill()
      NSBezierPath(roundedRect: bounds, xRadius: 6, yRadius: 6).fill()
    }
  }

  // MARK: - Layout

  override func layout() {
    super.layout()

    let insets = NSEdgeInsets(top: 8, left: 12, bottom: 8, right: 12)
    let contentWidth = max(0, bounds.width - insets.left - insets.right)

    let dotSize: CGFloat = 8
    let dotX = insets.left
    let topY = bounds.height - insets.top

    // Status dot
    statusDot.frame = NSRect(
      x: dotX, y: topY - 13, width: dotSize, height: dotSize)

    // Chevron
    let chevronWidth: CGFloat = 14
    chevronLabel.frame = NSRect(
      x: bounds.width - insets.right - chevronWidth,
      y: topY - 14, width: chevronWidth, height: 14)

    // Elapsed time (right-aligned, monospaced)
    let elapsedWidth: CGFloat = 80
    elapsedLabel.frame = NSRect(
      x: bounds.width - insets.right - chevronWidth - elapsedWidth,
      y: topY - 14, width: elapsedWidth, height: 14)

    // Name label
    let nameX = dotX + dotSize + 6
    let nameWidth = max(0, elapsedLabel.frame.minX - nameX - 4)
    nameLabel.frame = NSRect(x: nameX, y: topY - 14, width: nameWidth, height: 14)

    // Detail line
    let detailY = topY - 30
    detailLabel.frame = NSRect(
      x: nameX, y: detailY, width: max(0, contentWidth - dotSize - 6), height: 14)

    // Hover card (overlays detail)
    hoverCard.frame = detailLabel.frame
    let swatchSize: CGFloat = 7
    hoverColorSwatch.frame = NSRect(
      x: 6, y: (hoverCard.bounds.height - swatchSize) / 2,
      width: swatchSize, height: swatchSize)
    hoverLabel.frame = NSRect(
      x: hoverColorSwatch.frame.maxX + 6, y: 0,
      width: max(0, hoverCard.bounds.width - hoverColorSwatch.frame.maxX - 12),
      height: hoverCard.bounds.height)

    // Timeline bar
    let barHeight: CGFloat = 8
    let barY = detailY - barHeight - 4
    barView.frame = NSRect(
      x: insets.left, y: barY, width: contentWidth, height: barHeight)
    barView.isHidden = !barVisible

    // Expanded content
    let expandedTop = barY - 4
    let expandedHeight = max(0, expandedTop - insets.bottom)
    expandedScroll.frame = NSRect(
      x: insets.left, y: insets.bottom,
      width: contentWidth, height: expandedHeight)
    expandedScroll.isHidden = !isExpanded

    if isExpanded {
      LayoutExpandedContent(availableWidth: contentWidth)
    }
  }

  private func LayoutExpandedContent(availableWidth: CGFloat) {
    let innerInset: CGFloat = 4
    let sectionSpacing: CGFloat = 10
    let innerWidth = max(0, availableWidth - innerInset * 2)
    var y: CGFloat = 8

    // History section
    historyTitleLabel.frame = NSRect(x: innerInset, y: y, width: innerWidth, height: 14)
    y += 18

    let historyHeight: CGFloat = historyRunViews.isEmpty ? 20 : min(
      CGFloat(historyRunViews.count) * (RunHistoryRowView.preferredHeight + 4) + 8, 140)
    historyScrollView.frame = NSRect(
      x: innerInset, y: y, width: innerWidth, height: historyHeight)
    historyEmptyLabel.frame = NSRect(
      x: 4, y: max(0, (historyHeight - 14) / 2),
      width: max(0, innerWidth - 8), height: 14)
    LayoutHistoryRows()
    y += historyHeight + sectionSpacing

    // Action buttons
    let buttonWidth: CGFloat = 100
    let buttonHeight: CGFloat = 22
    let buttonSpacing: CGFloat = 8
    openFinderButton.frame = NSRect(
      x: innerInset, y: y, width: buttonWidth, height: buttonHeight)
    reconnectButton.frame = NSRect(
      x: innerInset + buttonWidth + buttonSpacing, y: y,
      width: buttonWidth, height: buttonHeight)
    openFinderButton.isHidden = endpointRow.cwd == nil
    y += buttonHeight + sectionSpacing

    // Workspace path
    cwdLabel.frame = NSRect(x: innerInset, y: y, width: innerWidth, height: 12)
    cwdLabel.isHidden = endpointRow.cwd == nil
    if endpointRow.cwd != nil {
      y += 16
    }

    // Commands section
    if !endpointRow.commands.isEmpty {
      commandsTitleLabel.frame = NSRect(x: innerInset, y: y, width: innerWidth, height: 12)
      commandsTitleLabel.isHidden = false
      y += 14
      let cmdLines = min(endpointRow.commands.count, 5)
      let cmdHeight = CGFloat(cmdLines) * 13 + 2
      commandsContentLabel.frame = NSRect(
        x: innerInset, y: y, width: innerWidth, height: cmdHeight)
      commandsContentLabel.isHidden = false
      y += cmdHeight + sectionSpacing
    } else {
      commandsTitleLabel.isHidden = true
      commandsContentLabel.isHidden = true
    }

    // File changes section
    if !endpointRow.fileChanges.isEmpty {
      filesTitleLabel.frame = NSRect(x: innerInset, y: y, width: innerWidth, height: 12)
      filesTitleLabel.isHidden = false
      y += 14
      let fileLines = min(endpointRow.fileChanges.count, 8)
      let filesHeight = CGFloat(fileLines) * 13 + 2
      filesContentLabel.frame = NSRect(
        x: innerInset, y: y, width: innerWidth, height: filesHeight)
      filesContentLabel.isHidden = false
      y += filesHeight + sectionSpacing
    } else {
      filesTitleLabel.isHidden = true
      filesContentLabel.isHidden = true
    }

    // Plan section
    if !endpointRow.planSteps.isEmpty {
      planTitleLabel.frame = NSRect(x: innerInset, y: y, width: innerWidth, height: 12)
      planTitleLabel.isHidden = false
      y += 14
      let planLines = min(endpointRow.planSteps.count, 6)
      let planHeight = CGFloat(planLines) * 13 + 2
      planContentLabel.frame = NSRect(
        x: innerInset, y: y, width: innerWidth, height: planHeight)
      planContentLabel.isHidden = false
      y += planHeight + sectionSpacing
    } else {
      planTitleLabel.isHidden = true
      planContentLabel.isHidden = true
    }

    // Error card
    if endpointRow.latestError != nil {
      let errorHeight: CGFloat = 32
      errorCard.frame = NSRect(x: innerInset, y: y, width: innerWidth, height: errorHeight)
      errorLabel.frame = NSRect(x: 8, y: 2, width: max(0, innerWidth - 16), height: errorHeight - 4)
      errorCard.isHidden = false
      y += errorHeight + sectionSpacing
    } else {
      errorCard.isHidden = true
    }

    // Token usage
    if let usage = endpointRow.tokenUsage, usage.totalTokens > 0 {
      tokenBarView.frame = NSRect(x: innerInset, y: y, width: innerWidth, height: 10)
      tokenBarView.isHidden = false
      y += 14
      tokenDetailLabel.frame = NSRect(x: innerInset, y: y, width: innerWidth, height: 12)
      tokenDetailLabel.isHidden = false
      y += 16
    } else {
      tokenBarView.isHidden = true
      tokenDetailLabel.isHidden = true
    }

    // Git + model line
    let hasGitOrModel =
      endpointRow.gitInfo?.branch != nil || endpointRow.model != nil
    if hasGitOrModel {
      gitModelLabel.frame = NSRect(x: innerInset, y: y, width: innerWidth, height: 12)
      gitModelLabel.isHidden = false
      y += 16
    } else {
      gitModelLabel.isHidden = true
    }

    y += 4

    let docHeight = max(y, expandedScroll.bounds.height)
    expandedDocView.frame = NSRect(x: 0, y: 0, width: availableWidth, height: docHeight)
    expandedScroll.documentView?.scroll(NSPoint(x: 0, y: max(0, docHeight - expandedScroll.bounds.height)))
  }

  // MARK: - Configuration

  private func ConfigureViews() {
    wantsLayer = true

    // Status dot
    statusDot.wantsLayer = true
    statusDot.layer?.cornerRadius = 4

    // Name
    nameLabel.font = NSFont.systemFont(ofSize: 12, weight: .semibold)
    nameLabel.lineBreakMode = .byTruncatingTail
    nameLabel.maximumNumberOfLines = 1

    // Elapsed time
    elapsedLabel.font = NSFont.monospacedDigitSystemFont(ofSize: 11, weight: .medium)
    elapsedLabel.textColor = .secondaryLabelColor
    elapsedLabel.alignment = .right
    elapsedLabel.lineBreakMode = .byTruncatingTail
    elapsedLabel.maximumNumberOfLines = 1

    // Chevron
    chevronLabel.font = NSFont.systemFont(ofSize: 10, weight: .medium)
    chevronLabel.textColor = .tertiaryLabelColor
    chevronLabel.alignment = .center
    chevronLabel.maximumNumberOfLines = 1
    chevronLabel.stringValue = isExpanded ? "▾" : "▸"

    // Detail
    detailLabel.font = NSFont.systemFont(ofSize: 11, weight: .regular)
    detailLabel.textColor = .secondaryLabelColor
    detailLabel.lineBreakMode = .byTruncatingTail
    detailLabel.maximumNumberOfLines = 1

    // Hover card
    hoverCard.wantsLayer = true
    hoverCard.layer?.cornerRadius = 4
    hoverCard.layer?.backgroundColor =
      NSColor.controlBackgroundColor.withAlphaComponent(0.85).cgColor
    hoverCard.layer?.borderWidth = 1
    hoverCard.layer?.borderColor = NSColor.separatorColor.withAlphaComponent(0.55).cgColor
    hoverCard.isHidden = true

    hoverColorSwatch.wantsLayer = true
    hoverColorSwatch.layer?.cornerRadius = 3.5

    hoverLabel.font = NSFont.systemFont(ofSize: 11, weight: .medium)
    hoverLabel.lineBreakMode = .byTruncatingTail
    hoverLabel.maximumNumberOfLines = 1

    // Expanded scroll container
    expandedScroll.drawsBackground = false
    expandedScroll.borderType = .noBorder
    expandedScroll.hasVerticalScroller = true
    expandedScroll.autohidesScrollers = true
    expandedScroll.documentView = expandedDocView
    expandedDocView.wantsLayer = true
    expandedDocView.layer?.cornerRadius = 4
    expandedDocView.layer?.backgroundColor =
      NSColor.controlBackgroundColor.withAlphaComponent(0.4).cgColor

    // Git/model line
    gitModelLabel.font = NSFont.systemFont(ofSize: 10, weight: .medium)
    gitModelLabel.textColor = .secondaryLabelColor
    gitModelLabel.lineBreakMode = .byTruncatingTail
    gitModelLabel.maximumNumberOfLines = 1

    // Token usage
    tokenDetailLabel.font = NSFont.monospacedDigitSystemFont(ofSize: 10, weight: .regular)
    tokenDetailLabel.textColor = .secondaryLabelColor
    tokenDetailLabel.lineBreakMode = .byTruncatingTail
    tokenDetailLabel.maximumNumberOfLines = 1

    // Error card
    errorCard.wantsLayer = true
    errorCard.layer?.cornerRadius = 4
    errorCard.layer?.backgroundColor = NSColor.systemRed.withAlphaComponent(0.1).cgColor
    errorCard.layer?.borderWidth = 1
    errorCard.layer?.borderColor = NSColor.systemRed.withAlphaComponent(0.3).cgColor

    errorLabel.font = NSFont.systemFont(ofSize: 10, weight: .medium)
    errorLabel.textColor = NSColor.systemRed
    errorLabel.lineBreakMode = .byTruncatingTail
    errorLabel.maximumNumberOfLines = 2

    // Plan section
    planTitleLabel.font = NSFont.systemFont(ofSize: 10, weight: .semibold)
    planTitleLabel.textColor = .secondaryLabelColor
    planTitleLabel.maximumNumberOfLines = 1

    planContentLabel.font = NSFont.systemFont(ofSize: 10, weight: .regular)
    planContentLabel.textColor = .secondaryLabelColor
    planContentLabel.lineBreakMode = .byTruncatingTail
    planContentLabel.maximumNumberOfLines = 6

    // Files section
    filesTitleLabel.font = NSFont.systemFont(ofSize: 10, weight: .semibold)
    filesTitleLabel.textColor = .secondaryLabelColor
    filesTitleLabel.maximumNumberOfLines = 1

    filesContentLabel.font = NSFont.monospacedSystemFont(ofSize: 10, weight: .regular)
    filesContentLabel.textColor = .secondaryLabelColor
    filesContentLabel.lineBreakMode = .byTruncatingTail
    filesContentLabel.maximumNumberOfLines = 8

    // Commands section
    commandsTitleLabel.font = NSFont.systemFont(ofSize: 10, weight: .semibold)
    commandsTitleLabel.textColor = .secondaryLabelColor
    commandsTitleLabel.maximumNumberOfLines = 1

    commandsContentLabel.font = NSFont.monospacedSystemFont(ofSize: 10, weight: .regular)
    commandsContentLabel.textColor = .secondaryLabelColor
    commandsContentLabel.lineBreakMode = .byTruncatingTail
    commandsContentLabel.maximumNumberOfLines = 5

    // Workspace path
    cwdLabel.font = NSFont.monospacedSystemFont(ofSize: 10, weight: .regular)
    cwdLabel.textColor = .tertiaryLabelColor
    cwdLabel.lineBreakMode = .byTruncatingMiddle
    cwdLabel.maximumNumberOfLines = 1

    // Action buttons
    openFinderButton.target = self
    openFinderButton.action = #selector(OnOpenFinderPressed)
    openFinderButton.bezelStyle = .rounded
    openFinderButton.font = NSFont.systemFont(ofSize: 10, weight: .medium)

    reconnectButton.target = self
    reconnectButton.action = #selector(OnReconnectPressed)
    reconnectButton.bezelStyle = .rounded
    reconnectButton.font = NSFont.systemFont(ofSize: 10, weight: .medium)

    // History
    historyTitleLabel.font = NSFont.systemFont(ofSize: 10, weight: .semibold)
    historyTitleLabel.textColor = .secondaryLabelColor
    historyTitleLabel.maximumNumberOfLines = 1

    historyScrollView.drawsBackground = false
    historyScrollView.borderType = .noBorder
    historyScrollView.hasVerticalScroller = true
    historyScrollView.autohidesScrollers = true
    historyScrollView.documentView = historyDocumentView
    historyDocumentView.wantsLayer = false

    historyEmptyLabel.font = NSFont.systemFont(ofSize: 10, weight: .regular)
    historyEmptyLabel.textColor = .tertiaryLabelColor
    historyEmptyLabel.alignment = .center
    historyEmptyLabel.maximumNumberOfLines = 1

    // Timeline bar hover callback
    barView.OnHoveredSegmentChanged = { [weak self] segment in
      self?.UpdateHoverText(segment: segment)
    }

    // View hierarchy
    addSubview(statusDot)
    addSubview(nameLabel)
    addSubview(elapsedLabel)
    addSubview(chevronLabel)
    addSubview(barView)
    addSubview(detailLabel)
    addSubview(hoverCard)
    hoverCard.addSubview(hoverColorSwatch)
    hoverCard.addSubview(hoverLabel)
    addSubview(expandedScroll)
    expandedDocView.addSubview(gitModelLabel)
    expandedDocView.addSubview(tokenBarView)
    expandedDocView.addSubview(tokenDetailLabel)
    expandedDocView.addSubview(errorCard)
    errorCard.addSubview(errorLabel)
    expandedDocView.addSubview(planTitleLabel)
    expandedDocView.addSubview(planContentLabel)
    expandedDocView.addSubview(filesTitleLabel)
    expandedDocView.addSubview(filesContentLabel)
    expandedDocView.addSubview(commandsTitleLabel)
    expandedDocView.addSubview(commandsContentLabel)
    expandedDocView.addSubview(cwdLabel)
    expandedDocView.addSubview(openFinderButton)
    expandedDocView.addSubview(reconnectButton)
    expandedDocView.addSubview(historyTitleLabel)
    expandedDocView.addSubview(historyScrollView)
    historyScrollView.addSubview(historyEmptyLabel)
  }

  // MARK: - Actions

  @objc
  private func OnReconnectPressed() {
    onReconnectEndpoint?(endpointRow.endpointId)
  }

  @objc
  private func OnOpenFinderPressed() {
    if let cwd = endpointRow.cwd {
      NSWorkspace.shared.open(URL(fileURLWithPath: cwd))
    }
    onOpenWorkspace?(endpointRow.endpointId)
  }

  // MARK: - Update

  private func Update(now: Date) {
    let name = endpointRow.displayName
    guard let turn = endpointRow.activeTurn else {
      barVisible = false
      nameLabel.stringValue = name
      elapsedLabel.stringValue = "Idle"
      statusDot.layer?.backgroundColor = NSColor.systemGray.withAlphaComponent(0.6).cgColor
      defaultDetailText = endpointRow.lastTraceLabel ?? "No active run"
      barView.Configure(segments: [])
      ShowDefaultDetail()
      UpdateExpandedFields(now: now)
      ComputeExpandedHeight(now: now)
      needsLayout = true
      return
    }

    barVisible = true
    nameLabel.stringValue = name
    statusDot.layer?.backgroundColor = StatusDotColor(turn.status).cgColor

    let statusText = StatusLabel(turn.status)
    elapsedLabel.stringValue = "\(statusText) \(turn.ElapsedString(now: now))"

    var summaryParts: [String] = []
    if let traceLabel = endpointRow.lastTraceLabel ?? turn.latestLabel {
      summaryParts.append(traceLabel)
    }
    let fileCount = endpointRow.fileChanges.count
    let cmdCount = endpointRow.commands.count
    if fileCount > 0 {
      summaryParts.append("\(fileCount) file\(fileCount == 1 ? "" : "s")")
    }
    if cmdCount > 0 {
      summaryParts.append("\(cmdCount) cmd\(cmdCount == 1 ? "" : "s")")
    }
    defaultDetailText = summaryParts.isEmpty ? "Working…" : summaryParts.joined(separator: " · ")

    barView.Configure(segments: turn.TimelineSegments(now: now))
    ShowDefaultDetail()
    UpdateExpandedFields(now: now)
    ComputeExpandedHeight(now: now)
    needsLayout = true
  }

  private func UpdateExpandedFields(now: Date) {
    // Git + Model line
    var gitModelParts: [String] = []
    if let branch = endpointRow.gitInfo?.branch {
      var part = branch
      if let sha = endpointRow.gitInfo?.sha {
        part += " · \(String(sha.prefix(7)))"
      }
      gitModelParts.append(part)
    }
    if let model = endpointRow.model {
      if gitModelParts.isEmpty {
        gitModelParts.append(model)
      } else {
        let padding = String(
          repeating: " ",
          count: max(1, 50 - gitModelParts.joined().count))
        gitModelParts.append("\(padding)\(model)")
      }
    }
    gitModelLabel.stringValue = gitModelParts.joined()

    // Token usage
    if let usage = endpointRow.tokenUsage, usage.totalTokens > 0 {
      tokenBarView.Configure(usage: usage)
      var parts: [String] = []
      parts.append("In: \(FormatTokenCount(usage.inputTokens))")
      if usage.cachedInputTokens > 0 {
        parts[parts.count - 1] += " (\(FormatTokenCount(usage.cachedInputTokens)) cached)"
      }
      parts.append("Out: \(FormatTokenCount(usage.outputTokens))")
      if usage.reasoningTokens > 0 {
        parts.append("Reasoning: \(FormatTokenCount(usage.reasoningTokens))")
      }
      tokenDetailLabel.stringValue = parts.joined(separator: " · ")
    }

    // Error
    if let error = endpointRow.latestError {
      var errorText = error.message
      if error.willRetry {
        errorText += " (retrying…)"
      }
      errorLabel.stringValue = errorText
    }

    // Plan
    if !endpointRow.planSteps.isEmpty {
      let completed = endpointRow.planSteps.filter { $0.status == .completed }.count
      let total = endpointRow.planSteps.count
      planTitleLabel.stringValue = "Plan (\(completed)/\(total) complete)"

      let lines = endpointRow.planSteps.prefix(6).map { step -> String in
        let icon: String
        switch step.status {
        case .completed: icon = "✓"
        case .inProgress: icon = "●"
        case .pending: icon = "○"
        }
        return "  \(icon) \(Truncate(step.description, limit: 55))"
      }
      planContentLabel.stringValue = lines.joined(separator: "\n")
    }

    // File changes
    if !endpointRow.fileChanges.isEmpty {
      filesTitleLabel.stringValue = "Files Changed (\(endpointRow.fileChanges.count))"
      let lines = endpointRow.fileChanges.prefix(8).map { change -> String in
        let lastComponent = (change.path as NSString).lastPathComponent
        let parentDir = (change.path as NSString).deletingLastPathComponent
        let shortParent = parentDir.isEmpty ? "" : "\(parentDir)/"
        return "  \(change.kind.label) \(shortParent)\(lastComponent)"
      }
      filesContentLabel.stringValue = lines.joined(separator: "\n")
    }

    // Commands
    if !endpointRow.commands.isEmpty {
      commandsTitleLabel.stringValue = "Commands (\(endpointRow.commands.count))"
      let lines = endpointRow.commands.suffix(5).map { cmd -> String in
        let shortCmd = Truncate(cmd.command, limit: 40)
        var suffix = ""
        if let exitCode = cmd.exitCode {
          suffix += "  exit \(exitCode)"
        }
        if let ms = cmd.durationMs {
          let sec = Double(ms) / 1000.0
          suffix += "  \(String(format: "%.1fs", sec))"
        }
        return "  > \(shortCmd)\(suffix)"
      }
      commandsContentLabel.stringValue = lines.joined(separator: "\n")
    }

    // Workspace path
    if let cwd = endpointRow.cwd {
      cwdLabel.stringValue = cwd.replacingOccurrences(of: NSHomeDirectory(), with: "~")
    }

    // History
    let runCount = endpointRow.recentRuns.count
    historyTitleLabel.stringValue =
      runCount > 0 ? "Past runs (\(runCount))" : "Past runs"
    RebuildHistoryRows(now: now)
  }

  private func ComputeExpandedHeight(now: Date) {
    guard isExpanded else {
      computedExpandedHeight = 0
      return
    }
    var h: CGFloat = 8
    let sectionSpacing: CGFloat = 10

    // Git/model
    if endpointRow.gitInfo?.branch != nil || endpointRow.model != nil {
      h += 16
    }
    // Token usage
    if let usage = endpointRow.tokenUsage, usage.totalTokens > 0 {
      h += 30
    }
    // Error
    if endpointRow.latestError != nil {
      h += 32 + sectionSpacing
    }
    // Plan
    if !endpointRow.planSteps.isEmpty {
      h += 14 + CGFloat(min(endpointRow.planSteps.count, 6)) * 13 + 2 + sectionSpacing
    }
    // Files
    if !endpointRow.fileChanges.isEmpty {
      h += 14 + CGFloat(min(endpointRow.fileChanges.count, 8)) * 13 + 2 + sectionSpacing
    }
    // Commands
    if !endpointRow.commands.isEmpty {
      h += 14 + CGFloat(min(endpointRow.commands.count, 5)) * 13 + 2 + sectionSpacing
    }
    // CWD
    if endpointRow.cwd != nil {
      h += 16
    }
    // Buttons
    h += 22 + sectionSpacing
    // History title
    h += 18
    // History scroll
    let historyH: CGFloat = historyRunViews.isEmpty ? 20 : min(
      CGFloat(historyRunViews.count) * (RunHistoryRowView.preferredHeight + 4) + 8, 140)
    h += historyH + sectionSpacing
    h += 4

    computedExpandedHeight = min(h, Self.maxExpandedHeight - Self.collapsedRowHeight)
  }

  private func RebuildHistoryRows(now: Date) {
    for row in historyRunViews {
      row.removeFromSuperview()
    }
    historyRunViews.removeAll(keepingCapacity: true)

    for (index, run) in endpointRow.recentRuns.enumerated() {
      let historyRow = RunHistoryRowView(frame: .zero)
      historyRow.Configure(run: run, isLastRun: index == 0)
      historyDocumentView.addSubview(historyRow)
      historyRunViews.append(historyRow)
    }

    historyEmptyLabel.isHidden = !historyRunViews.isEmpty
    needsLayout = true
  }

  private func LayoutHistoryRows() {
    let rowHeight: CGFloat = RunHistoryRowView.preferredHeight
    let rowSpacing: CGFloat = 4
    let contentWidth = max(0, historyScrollView.bounds.width)

    var y: CGFloat = rowSpacing
    for row in historyRunViews.reversed() {
      row.frame = NSRect(x: 0, y: y, width: contentWidth, height: rowHeight)
      y += rowHeight + rowSpacing
    }

    let contentHeight = max(historyScrollView.bounds.height, y)
    historyDocumentView.frame = NSRect(
      x: 0, y: 0, width: contentWidth, height: contentHeight)
  }

  // MARK: - Hover

  private func UpdateHoverText(segment: TimelineSegment?) {
    guard barVisible else {
      ShowDefaultDetail()
      return
    }
    guard let segment else {
      ShowDefaultDetail()
      return
    }

    let segmentColor = SegmentFillColor(segment.kind)
    hoverCard.layer?.borderColor = segmentColor.withAlphaComponent(0.8).cgColor
    hoverColorSwatch.layer?.backgroundColor = segmentColor.cgColor
    hoverLabel.stringValue = HoverText(segment: segment)
    detailLabel.isHidden = true
    hoverCard.isHidden = false
  }

  private func ShowDefaultDetail() {
    detailLabel.stringValue = defaultDetailText
    detailLabel.isHidden = false
    hoverCard.isHidden = true
  }

  // MARK: - Formatting helpers

  private func HoverText(segment: TimelineSegment) -> String {
    let category = SegmentKindLabel(segment.kind)
    let duration = FormatDuration(segment.duration)
    let start = FormatClockTime(segment.startedAt)
    let end = FormatClockTime(segment.endedAt)
    if let label = segment.label, !label.isEmpty {
      return "\(category) · \(duration) · \(start)-\(end) · \(label)"
    }
    return "\(category) · \(duration) · \(start)-\(end)"
  }

  private func StatusLabel(_ status: TurnExecutionStatus) -> String {
    switch status {
    case .inProgress: return "Working"
    case .completed: return "Done"
    case .interrupted: return "Interrupted"
    case .failed: return "Failed"
    }
  }

  private func StatusDotColor(_ status: TurnExecutionStatus) -> NSColor {
    switch status {
    case .inProgress: return NSColor.systemGreen
    case .completed: return NSColor.systemGray
    case .interrupted: return NSColor.systemOrange
    case .failed: return NSColor.systemRed
    }
  }

  private func Truncate(_ value: String, limit: Int) -> String {
    if value.count <= limit { return value }
    return "\(value.prefix(max(0, limit - 1)))…"
  }
}

// MARK: - RunHistoryRowView

final class RunHistoryRowView: NSView {
  static let preferredHeight: CGFloat = 40

  private let statusDot = NSView()
  private let titleLabel = NSTextField(labelWithString: "")
  private let timelineBarView = TimelineBarView()

  override init(frame frameRect: NSRect) {
    super.init(frame: frameRect)
    wantsLayer = true

    statusDot.wantsLayer = true
    statusDot.layer?.cornerRadius = 3

    titleLabel.font = NSFont.monospacedDigitSystemFont(ofSize: 10, weight: .regular)
    titleLabel.lineBreakMode = .byTruncatingTail
    titleLabel.maximumNumberOfLines = 1

    timelineBarView.OnHoveredSegmentChanged = nil

    addSubview(statusDot)
    addSubview(titleLabel)
    addSubview(timelineBarView)
  }

  @available(*, unavailable)
  required init?(coder: NSCoder) {
    fatalError("init(coder:) has not been implemented")
  }

  override func layout() {
    super.layout()
    let dotSize: CGFloat = 6
    statusDot.frame = NSRect(
      x: 2, y: bounds.height - 11, width: dotSize, height: dotSize)
    titleLabel.frame = NSRect(
      x: dotSize + 6, y: bounds.height - 14,
      width: max(0, bounds.width - dotSize - 6), height: 12)
    timelineBarView.frame = NSRect(
      x: 0, y: 0, width: bounds.width, height: 8)
  }

  func Configure(run: CompletedRun, isLastRun: Bool) {
    let elapsed = run.ElapsedString()
    let status = StatusText(run.status)
    let suffix = isLastRun ? " · latest" : ""
    titleLabel.stringValue = "\(status) in \(elapsed)\(suffix)"
    statusDot.layer?.backgroundColor = StatusColor(run.status).cgColor
    timelineBarView.Configure(segments: run.TimelineSegments())
    needsLayout = true
  }

  private func StatusText(_ status: TurnExecutionStatus) -> String {
    switch status {
    case .inProgress: return "Working"
    case .completed: return "Completed"
    case .interrupted: return "Interrupted"
    case .failed: return "Failed"
    }
  }

  private func StatusColor(_ status: TurnExecutionStatus) -> NSColor {
    switch status {
    case .inProgress: return .systemGreen
    case .completed: return .systemGray
    case .interrupted: return .systemOrange
    case .failed: return .systemRed
    }
  }
}

// MARK: - TokenUsageBarView

final class TokenUsageBarView: NSView {
  private var fraction: Double = 0
  private var usageText = ""

  override init(frame frameRect: NSRect) {
    super.init(frame: frameRect)
    wantsLayer = true
  }

  @available(*, unavailable)
  required init?(coder: NSCoder) {
    fatalError("init(coder:) has not been implemented")
  }

  func Configure(usage: TokenUsageInfo) {
    fraction = usage.contextUsageFraction ?? 0
    if let cw = usage.contextWindow {
      usageText = "\(FormatTokenCount(usage.totalTokens)) / \(FormatTokenCount(cw))"
    } else {
      usageText = FormatTokenCount(usage.totalTokens)
    }
    needsDisplay = true
  }

  override func draw(_ dirtyRect: NSRect) {
    super.draw(dirtyRect)
    let trackRect = bounds.insetBy(dx: 0.5, dy: 0.5)
    guard trackRect.width > 0, trackRect.height > 0 else { return }

    let trackPath = NSBezierPath(roundedRect: trackRect, xRadius: 4, yRadius: 4)
    NSColor.controlBackgroundColor.withAlphaComponent(0.8).setFill()
    trackPath.fill()

    if fraction > 0 {
      NSGraphicsContext.saveGraphicsState()
      trackPath.addClip()
      let fillWidth = trackRect.width * CGFloat(min(1.0, fraction))
      let fillRect = NSRect(
        x: trackRect.minX, y: trackRect.minY,
        width: fillWidth, height: trackRect.height)
      let fillColor =
        fraction > 0.85
        ? NSColor.systemOrange.withAlphaComponent(0.7)
        : NSColor.controlAccentColor.withAlphaComponent(0.5)
      fillColor.setFill()
      NSBezierPath(rect: fillRect).fill()
      NSGraphicsContext.restoreGraphicsState()
    }

    NSColor.separatorColor.withAlphaComponent(0.6).setStroke()
    trackPath.lineWidth = 0.5
    trackPath.stroke()

    let textAttrs: [NSAttributedString.Key: Any] = [
      .font: NSFont.monospacedDigitSystemFont(ofSize: 9, weight: .medium),
      .foregroundColor: NSColor.secondaryLabelColor,
    ]
    let textSize = usageText.size(withAttributes: textAttrs)
    let textPoint = NSPoint(
      x: trackRect.maxX - textSize.width - 4,
      y: (trackRect.height - textSize.height) / 2 + trackRect.minY)
    usageText.draw(at: textPoint, withAttributes: textAttrs)
  }
}

// MARK: - TimelineBarView

final class TimelineBarView: NSView {
  var OnHoveredSegmentChanged: ((TimelineSegment?) -> Void)?

  private var segments: [TimelineSegment] = []
  private var segmentRects: [CGRect] = []
  private var hoverIndex: Int?
  private var trackingArea: NSTrackingArea?

  override init(frame frameRect: NSRect) {
    super.init(frame: frameRect)
    wantsLayer = true
  }

  @available(*, unavailable)
  required init?(coder: NSCoder) {
    fatalError("init(coder:) has not been implemented")
  }

  func Configure(segments: [TimelineSegment]) {
    self.segments = segments.filter { $0.duration > 0 }
    hoverIndex = nil
    segmentRects = []
    OnHoveredSegmentChanged?(nil)
    needsDisplay = true
  }

  override func updateTrackingAreas() {
    super.updateTrackingAreas()
    if let trackingArea {
      removeTrackingArea(trackingArea)
    }
    let options: NSTrackingArea.Options = [
      .activeAlways, .mouseEnteredAndExited, .mouseMoved, .inVisibleRect,
    ]
    let area = NSTrackingArea(rect: bounds, options: options, owner: self, userInfo: nil)
    addTrackingArea(area)
    trackingArea = area
  }

  override func mouseEntered(with event: NSEvent) {
    UpdateHoverIndex(location: convert(event.locationInWindow, from: nil))
  }

  override func mouseMoved(with event: NSEvent) {
    UpdateHoverIndex(location: convert(event.locationInWindow, from: nil))
  }

  override func mouseExited(with event: NSEvent) {
    if hoverIndex != nil {
      hoverIndex = nil
      OnHoveredSegmentChanged?(nil)
      needsDisplay = true
    }
  }

  override func draw(_ dirtyRect: NSRect) {
    super.draw(dirtyRect)
    let trackRect = bounds.insetBy(dx: 0.5, dy: 0.5)
    guard trackRect.width > 0, trackRect.height > 0 else { return }

    let trackPath = NSBezierPath(roundedRect: trackRect, xRadius: 4, yRadius: 4)
    NSColor.controlBackgroundColor.withAlphaComponent(0.8).setFill()
    trackPath.fill()

    let pixelWidth = max(0, Int(trackRect.width.rounded(.down)))
    let widths = AllocatePixelWidths(totalWidth: pixelWidth)
    segmentRects = []

    NSGraphicsContext.saveGraphicsState()
    trackPath.addClip()

    var x = trackRect.minX
    for (index, width) in widths.enumerated() {
      if width <= 0 {
        segmentRects.append(.null)
        continue
      }
      let segmentRect = NSRect(
        x: x, y: trackRect.minY, width: CGFloat(width), height: trackRect.height
      ).intersection(trackRect)
      segmentRects.append(segmentRect)
      x += CGFloat(width)
      SegmentFillColor(segments[index].kind).setFill()
      NSBezierPath(rect: segmentRect).fill()
    }

    if widths.count >= 2 {
      NSColor.separatorColor.withAlphaComponent(0.5).setStroke()
      for index in 1..<widths.count {
        let previousRect = segmentRects[index - 1]
        let currentRect = segmentRects[index]
        if previousRect.isNull || currentRect.isNull { continue }
        let boundaryX = currentRect.minX
        let separator = NSBezierPath()
        separator.move(to: CGPoint(x: boundaryX, y: trackRect.minY))
        separator.line(to: CGPoint(x: boundaryX, y: trackRect.maxY))
        separator.lineWidth = 0.5
        separator.stroke()
      }
    }

    NSGraphicsContext.restoreGraphicsState()

    NSColor.separatorColor.withAlphaComponent(0.6).setStroke()
    trackPath.lineWidth = 0.5
    trackPath.stroke()

    if let hoverIndex, hoverIndex < segmentRects.count {
      let rect = segmentRects[hoverIndex]
      if !rect.isNull {
        NSColor.controlAccentColor.withAlphaComponent(0.95).setStroke()
        let highlightPath = NSBezierPath(
          roundedRect: rect.insetBy(dx: 0.5, dy: 0.5), xRadius: 3, yRadius: 3)
        highlightPath.lineWidth = 1.2
        highlightPath.stroke()
      }
    }
  }

  private func UpdateHoverIndex(location: CGPoint) {
    let nextIndex = segmentRects.firstIndex(where: { !$0.isNull && $0.contains(location) })
    if nextIndex == hoverIndex { return }
    hoverIndex = nextIndex
    if let nextIndex, nextIndex < segments.count {
      OnHoveredSegmentChanged?(segments[nextIndex])
    } else {
      OnHoveredSegmentChanged?(nil)
    }
    needsDisplay = true
  }

  private func AllocatePixelWidths(totalWidth: Int) -> [Int] {
    if segments.isEmpty || totalWidth <= 0 {
      return Array(repeating: 0, count: segments.count)
    }

    let durations = segments.map { max(0, $0.duration) }
    let totalDuration = durations.reduce(0, +)
    if totalDuration <= 0 {
      let base = totalWidth / segments.count
      let remainder = totalWidth % segments.count
      return durations.indices.map { base + ($0 < remainder ? 1 : 0) }
    }

    let exactWidths = durations.map { ($0 / totalDuration) * Double(totalWidth) }
    var widths = exactWidths.map { Int($0.rounded(.down)) }
    let remainders = exactWidths.map { $0 - Double(Int($0.rounded(.down))) }
    let minimumWidths = exactWidths.map { $0 > 0 ? 1 : 0 }

    for index in widths.indices where widths[index] < minimumWidths[index] {
      widths[index] = minimumWidths[index]
    }

    var assigned = widths.reduce(0, +)

    if assigned > totalWidth {
      var reducible = widths.indices.filter { widths[$0] > minimumWidths[$0] }
      while assigned > totalWidth && !reducible.isEmpty {
        reducible.sort { lhs, rhs in
          if remainders[lhs] != remainders[rhs] { return remainders[lhs] < remainders[rhs] }
          return widths[lhs] > widths[rhs]
        }
        guard let index = reducible.first else { break }
        widths[index] -= 1
        assigned -= 1
        reducible = widths.indices.filter { widths[$0] > minimumWidths[$0] }
      }
      if assigned > totalWidth {
        var positive = widths.indices.filter { widths[$0] > 0 }
        while assigned > totalWidth && !positive.isEmpty {
          positive.sort { widths[$0] > widths[$1] }
          guard let index = positive.first else { break }
          widths[index] -= 1
          assigned -= 1
          positive = widths.indices.filter { widths[$0] > 0 }
        }
      }
    }

    if assigned < totalWidth {
      let order = widths.indices.sorted { lhs, rhs in
        if remainders[lhs] != remainders[rhs] { return remainders[lhs] > remainders[rhs] }
        return durations[lhs] > durations[rhs]
      }
      if !order.isEmpty {
        var cursor = 0
        while assigned < totalWidth {
          widths[order[cursor % order.count]] += 1
          assigned += 1
          cursor += 1
        }
      }
    }

    return widths
  }
}

// MARK: - Shared formatting

private let durationFormatter: DateComponentsFormatter = {
  let formatter = DateComponentsFormatter()
  formatter.allowedUnits = [.hour, .minute, .second]
  formatter.unitsStyle = .abbreviated
  formatter.maximumUnitCount = 2
  formatter.zeroFormattingBehavior = [.dropLeading]
  return formatter
}()

private let clockTimeFormatter: DateFormatter = {
  let formatter = DateFormatter()
  formatter.timeStyle = .medium
  formatter.dateStyle = .none
  return formatter
}()

private func SegmentFillColor(_ kind: TimelineSegmentKind) -> NSColor {
  switch kind {
  case .category(let category):
    switch category {
    case .tool: return NSColor.systemIndigo.withAlphaComponent(0.9)
    case .edit: return NSColor.systemPurple.withAlphaComponent(0.9)
    case .waiting: return NSColor.systemRed.withAlphaComponent(0.9)
    case .network: return NSColor.systemBlue.withAlphaComponent(0.9)
    case .prefill: return NSColor.systemOrange.withAlphaComponent(0.9)
    case .reasoning: return NSColor.systemPink.withAlphaComponent(0.9)
    case .gen: return NSColor.systemGreen.withAlphaComponent(0.9)
    }
  case .idle:
    return NSColor.systemGray.withAlphaComponent(0.35)
  }
}

private func SegmentKindLabel(_ kind: TimelineSegmentKind) -> String {
  switch kind {
  case .category(let category):
    switch category {
    case .tool: return "Tool"
    case .edit: return "Edit"
    case .waiting: return "Waiting"
    case .network: return "Network"
    case .prefill: return "Prefill"
    case .reasoning: return "Reasoning"
    case .gen: return "Generation"
    }
  case .idle:
    return "Idle"
  }
}

private func FormatClockTime(_ date: Date) -> String {
  clockTimeFormatter.string(from: date)
}

private func FormatDuration(_ duration: TimeInterval) -> String {
  if duration <= 0 { return "0s" }
  return durationFormatter.string(from: duration) ?? "0s"
}
