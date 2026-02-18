import AppKit
import Foundation

// MARK: - TurnMenuRowView

final class TurnMenuRowView: NSView {
  private static let rowWidth: CGFloat = 420
  private static let collapsedIdleHeight: CGFloat = 44
  private static let collapsedActiveHeight: CGFloat = 56
  private static let maxExpandedHeight: CGFloat = 500

  private let endpointRow: EndpointRow
  private let isExpanded: Bool
  private let expandedRunKeys: Set<String>
  private let onToggle: ((String) -> Void)?
  private let onToggleHistoryRun: ((String, String) -> Void)?
  private let onReconnectEndpoint: ((String) -> Void)?
  private let onOpenWorkspace: ((String) -> Void)?

  // Collapsed header
  private let statusDot = NSView()
  private let nameLabel = NSTextField(labelWithString: "")
  private let elapsedLabel = NSTextField(labelWithString: "")
  private let chevronLabel = NSTextField(labelWithString: "")
  private let detailLabel = NSTextField(labelWithString: "")
  private let barView = TimelineBarView()

  // Expanded content
  private let expandedScroll = NSScrollView()
  private let expandedDocView = NSView()

  // Expanded section views
  private let promptLabel = NSTextField(labelWithString: "")
  private let gitModelLabel = NSTextField(labelWithString: "")
  private let tokenTitleLabel = NSTextField(labelWithString: "")
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
  private let historySectionCard = NSView()
  private let historyTitleLabel = NSTextField(labelWithString: "")
  private let historyScrollView = NSScrollView()
  private let historyDocumentView = NSView()
  private var historyRunViews: [RunHistoryRowView] = []
  private let buttonSeparator = NSView()
  private let openFinderButton = NSButton(title: "Open in Finder", target: nil, action: nil)
  private let reconnectButton = NSButton(title: "Reconnect", target: nil, action: nil)

  private var defaultDetailText = "No detail"
  private var defaultTokenDetailText = ""
  private var barVisible = true
  private var isHovering = false
  private var rowTrackingArea: NSTrackingArea?
  private var computedExpandedHeight: CGFloat = 0
  private var collapsedHeight: CGFloat = 0

  init(
    endpointRow: EndpointRow,
    now: Date,
    isExpanded: Bool,
    expandedRunKeys: Set<String>,
    onToggle: ((String) -> Void)?,
    onToggleHistoryRun: ((String, String) -> Void)?,
    onReconnectEndpoint: ((String) -> Void)?,
    onOpenWorkspace: ((String) -> Void)? = nil
  ) {
    self.endpointRow = endpointRow
    self.isExpanded = isExpanded
    self.expandedRunKeys = expandedRunKeys
    self.onToggle = onToggle
    self.onToggleHistoryRun = onToggleHistoryRun
    self.onReconnectEndpoint = onReconnectEndpoint
    self.onOpenWorkspace = onOpenWorkspace
    self.collapsedHeight =
      endpointRow.activeTurn != nil ? Self.collapsedActiveHeight : Self.collapsedIdleHeight
    super.init(frame: NSRect(x: 0, y: 0, width: Self.rowWidth, height: collapsedHeight))
    ConfigureViews()
    Update(now: now)
    let height = isExpanded ? collapsedHeight + computedExpandedHeight : collapsedHeight
    frame.size.height = height
  }

  @available(*, unavailable)
  required init?(coder: NSCoder) {
    fatalError("init(coder:) has not been implemented")
  }

  override var intrinsicContentSize: NSSize {
    let height = isExpanded ? collapsedHeight + computedExpandedHeight : collapsedHeight
    return NSSize(width: Self.rowWidth, height: height)
  }

  // MARK: - Interaction

  override func mouseDown(with event: NSEvent) {
    if isExpanded {
      let pointInSelf = convert(event.locationInWindow, from: nil)
      if expandedScroll.frame.contains(pointInSelf) {
        let pointInScroll = expandedScroll.convert(pointInSelf, from: self)
        let pointInDoc = expandedDocView.convert(pointInScroll, from: expandedScroll)
        if openFinderButton.isHidden == false {
          let finderFrame = openFinderButton.convert(openFinderButton.bounds, to: expandedDocView)
          if finderFrame.contains(pointInDoc) {
            OnOpenFinderPressed()
            return
          }
        }
        let reconnectFrame = reconnectButton.convert(reconnectButton.bounds, to: expandedDocView)
        if reconnectFrame.contains(pointInDoc) {
          OnReconnectPressed()
          return
        }
        if historySectionCard.frame.contains(pointInDoc) {
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
      owner: self, userInfo: nil)
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

    let insets = NSEdgeInsets(top: 6, left: 12, bottom: 0, right: 12)
    let contentWidth = max(0, bounds.width - insets.left - insets.right)

    let dotSize: CGFloat = 8
    let dotX = insets.left
    let topY = bounds.height - insets.top

    statusDot.frame = NSRect(x: dotX, y: topY - 12, width: dotSize, height: dotSize)

    let chevronWidth: CGFloat = 14
    chevronLabel.frame = NSRect(
      x: bounds.width - insets.right - chevronWidth,
      y: topY - 13, width: chevronWidth, height: 14)

    let elapsedWidth: CGFloat = 90
    elapsedLabel.frame = NSRect(
      x: bounds.width - insets.right - chevronWidth - elapsedWidth,
      y: topY - 13, width: elapsedWidth, height: 14)

    let nameX = dotX + dotSize + 6
    let nameWidth = max(0, elapsedLabel.frame.minX - nameX - 4)
    nameLabel.frame = NSRect(x: nameX, y: topY - 13, width: nameWidth, height: 14)

    let detailWidth = max(0, contentWidth - (nameX - insets.left))
    let collapsedBottom = bounds.height - collapsedHeight

    if barVisible {
      let detailY = topY - 28
      detailLabel.frame = NSRect(x: nameX, y: detailY, width: detailWidth, height: 13)

      let barHeight: CGFloat = 8
      let barY = collapsedBottom + 4
      barView.frame = NSRect(x: nameX, y: barY, width: detailWidth, height: barHeight)
      barView.isHidden = false
    } else {
      let detailY = topY - 27
      detailLabel.frame = NSRect(x: nameX, y: detailY, width: detailWidth, height: 13)
      barView.isHidden = true
    }

    let expandedHeight = max(0, collapsedBottom)
    expandedScroll.frame = NSRect(
      x: insets.left, y: 0, width: contentWidth, height: expandedHeight)
    expandedScroll.isHidden = !isExpanded

    if isExpanded {
      LayoutExpandedContent(availableWidth: contentWidth)
    }
  }

  private func LayoutExpandedContent(availableWidth: CGFloat) {
    let pad: CGFloat = 8
    let spc: CGFloat = 10
    let w = max(0, availableWidth - pad * 2)
    var y: CGFloat = pad

    // Buttons at the very bottom with separator
    let buttonHeight: CGFloat = 24
    let buttonWidth: CGFloat = 110
    let buttonSpacing: CGFloat = 8

    buttonSeparator.frame = NSRect(x: pad, y: y, width: w, height: 1)
    y += 8

    let totalButtonWidth =
      (endpointRow.cwd != nil ? buttonWidth + buttonSpacing : 0) + buttonWidth
    let buttonStartX = pad + (w - totalButtonWidth) / 2
    openFinderButton.frame = NSRect(
      x: buttonStartX, y: y, width: buttonWidth, height: buttonHeight)
    openFinderButton.isHidden = endpointRow.cwd == nil
    let reconnectX =
      endpointRow.cwd != nil ? buttonStartX + buttonWidth + buttonSpacing : buttonStartX
    reconnectButton.frame = NSRect(
      x: reconnectX, y: y, width: buttonWidth, height: buttonHeight)
    y += buttonHeight + spc

    // History section (only if there are runs)
    let hasHistory = !historyRunViews.isEmpty
    historySectionCard.isHidden = !hasHistory
    if hasHistory {
      let historyInner: CGFloat = 6
      let titleH: CGFloat = 14
      let runH = HistoryScrollHeight()
      let sectionH = titleH + 4 + runH + historyInner * 2

      historySectionCard.frame = NSRect(x: pad, y: y, width: w, height: sectionH)
      historyTitleLabel.frame = NSRect(
        x: historyInner, y: sectionH - historyInner - titleH,
        width: w - historyInner * 2, height: titleH)
      historyScrollView.frame = NSRect(
        x: historyInner, y: historyInner,
        width: w - historyInner * 2, height: runH)
      LayoutHistoryRows()
      y += sectionH + spc
    }

    // Workspace path
    if endpointRow.cwd != nil {
      cwdLabel.frame = NSRect(x: pad, y: y, width: w, height: 12)
      cwdLabel.isHidden = false
      y += 16
    } else {
      cwdLabel.isHidden = true
    }

    // Commands section
    if !endpointRow.commands.isEmpty {
      commandsTitleLabel.frame = NSRect(x: pad, y: y, width: w, height: 12)
      commandsTitleLabel.isHidden = false
      y += 15
      let cmdLines = min(endpointRow.commands.count, 5)
      let cmdHeight = CGFloat(cmdLines) * 14 + 2
      commandsContentLabel.frame = NSRect(x: pad, y: y, width: w, height: cmdHeight)
      commandsContentLabel.isHidden = false
      y += cmdHeight + spc
    } else {
      commandsTitleLabel.isHidden = true
      commandsContentLabel.isHidden = true
    }

    // Files section
    if !endpointRow.fileChanges.isEmpty {
      filesTitleLabel.frame = NSRect(x: pad, y: y, width: w, height: 12)
      filesTitleLabel.isHidden = false
      y += 15
      let fileLines = min(endpointRow.fileChanges.count, 8)
      let filesHeight = CGFloat(fileLines) * 14 + 2
      filesContentLabel.frame = NSRect(x: pad, y: y, width: w, height: filesHeight)
      filesContentLabel.isHidden = false
      y += filesHeight + spc
    } else {
      filesTitleLabel.isHidden = true
      filesContentLabel.isHidden = true
    }

    // Plan section
    if !endpointRow.planSteps.isEmpty {
      planTitleLabel.frame = NSRect(x: pad, y: y, width: w, height: 12)
      planTitleLabel.isHidden = false
      y += 15
      let planLines = min(endpointRow.planSteps.count, 6)
      let planHeight = CGFloat(planLines) * 14 + 2
      planContentLabel.frame = NSRect(x: pad, y: y, width: w, height: planHeight)
      planContentLabel.isHidden = false
      y += planHeight + spc
    } else {
      planTitleLabel.isHidden = true
      planContentLabel.isHidden = true
    }

    // Error card
    if endpointRow.latestError != nil {
      let errorHeight: CGFloat = 34
      errorCard.frame = NSRect(x: pad, y: y, width: w, height: errorHeight)
      errorLabel.frame = NSRect(
        x: 8, y: 4, width: max(0, w - 16), height: errorHeight - 8)
      errorCard.isHidden = false
      y += errorHeight + spc
    } else {
      errorCard.isHidden = true
    }

    // Token usage
    if EffectiveTokenUsage() != nil {
      tokenTitleLabel.frame = NSRect(x: pad, y: y, width: w, height: 12)
      tokenTitleLabel.isHidden = false
      y += 15
      tokenBarView.frame = NSRect(x: pad, y: y, width: w, height: 14)
      tokenBarView.isHidden = false
      y += 18
      tokenDetailLabel.frame = NSRect(x: pad, y: y, width: w, height: 12)
      tokenDetailLabel.isHidden = false
      y += 16
    } else {
      tokenTitleLabel.isHidden = true
      tokenBarView.isHidden = true
      tokenDetailLabel.isHidden = true
    }

    // Prompt preview
    if let promptText = PromptLabelText() {
      promptLabel.frame = NSRect(x: pad, y: y, width: w, height: 24)
      promptLabel.stringValue = promptText
      promptLabel.isHidden = false
      y += 28
    } else {
      promptLabel.isHidden = true
    }

    // Git + model line
    if HasGitOrModelInfo() {
      gitModelLabel.frame = NSRect(x: pad, y: y, width: w, height: 12)
      gitModelLabel.isHidden = false
      y += 16
    } else {
      gitModelLabel.isHidden = true
    }

    y += pad

    let docHeight = max(y, expandedScroll.bounds.height)
    expandedDocView.frame = NSRect(x: 0, y: 0, width: availableWidth, height: docHeight)
    expandedScroll.documentView?.scroll(
      NSPoint(x: 0, y: max(0, docHeight - expandedScroll.bounds.height)))
  }

  // MARK: - Configuration

  private func ConfigureViews() {
    wantsLayer = true

    statusDot.wantsLayer = true
    statusDot.layer?.cornerRadius = 4

    nameLabel.font = NSFont.systemFont(ofSize: 12, weight: .semibold)
    nameLabel.lineBreakMode = .byTruncatingTail
    nameLabel.maximumNumberOfLines = 1

    elapsedLabel.font = NSFont.monospacedDigitSystemFont(ofSize: 11, weight: .medium)
    elapsedLabel.textColor = .secondaryLabelColor
    elapsedLabel.alignment = .right
    elapsedLabel.lineBreakMode = .byTruncatingTail
    elapsedLabel.maximumNumberOfLines = 1

    chevronLabel.font = NSFont.systemFont(ofSize: 10, weight: .medium)
    chevronLabel.textColor = .tertiaryLabelColor
    chevronLabel.alignment = .center
    chevronLabel.maximumNumberOfLines = 1
    chevronLabel.stringValue = isExpanded ? "▾" : "▸"

    detailLabel.font = NSFont.systemFont(ofSize: 11, weight: .regular)
    detailLabel.textColor = .secondaryLabelColor
    detailLabel.lineBreakMode = .byTruncatingTail
    detailLabel.maximumNumberOfLines = 1

    // Expanded scroll container
    expandedScroll.drawsBackground = false
    expandedScroll.borderType = .noBorder
    expandedScroll.hasVerticalScroller = true
    expandedScroll.autohidesScrollers = true
    expandedScroll.documentView = expandedDocView
    expandedDocView.wantsLayer = true
    expandedDocView.layer?.cornerRadius = 6
    expandedDocView.layer?.backgroundColor =
      NSColor.controlBackgroundColor.withAlphaComponent(0.35).cgColor

    // Prompt line
    promptLabel.font = NSFont.systemFont(ofSize: 10, weight: .regular)
    promptLabel.textColor = .secondaryLabelColor
    promptLabel.lineBreakMode = .byTruncatingTail
    promptLabel.maximumNumberOfLines = 2

    // Git/model line
    gitModelLabel.font = NSFont.systemFont(ofSize: 10, weight: .medium)
    gitModelLabel.textColor = .secondaryLabelColor
    gitModelLabel.lineBreakMode = .byTruncatingTail
    gitModelLabel.maximumNumberOfLines = 1

    // Token usage
    tokenTitleLabel.font = NSFont.systemFont(ofSize: 10, weight: .semibold)
    tokenTitleLabel.textColor = .secondaryLabelColor
    tokenTitleLabel.maximumNumberOfLines = 1

    tokenDetailLabel.font = NSFont.monospacedDigitSystemFont(ofSize: 10, weight: .regular)
    tokenDetailLabel.textColor = .tertiaryLabelColor
    tokenDetailLabel.lineBreakMode = .byTruncatingTail
    tokenDetailLabel.maximumNumberOfLines = 1

    // Error card
    errorCard.wantsLayer = true
    errorCard.layer?.cornerRadius = 6
    errorCard.layer?.backgroundColor = NSColor.systemRed.withAlphaComponent(0.08).cgColor
    errorCard.layer?.borderWidth = 0.5
    errorCard.layer?.borderColor = NSColor.systemRed.withAlphaComponent(0.25).cgColor

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

    filesContentLabel.font = NSFont.systemFont(ofSize: 10, weight: .regular)
    filesContentLabel.textColor = .secondaryLabelColor
    filesContentLabel.lineBreakMode = .byTruncatingTail
    filesContentLabel.maximumNumberOfLines = 8

    // Commands section
    commandsTitleLabel.font = NSFont.systemFont(ofSize: 10, weight: .semibold)
    commandsTitleLabel.textColor = .secondaryLabelColor
    commandsTitleLabel.maximumNumberOfLines = 1

    commandsContentLabel.font = NSFont.systemFont(ofSize: 10, weight: .regular)
    commandsContentLabel.textColor = .secondaryLabelColor
    commandsContentLabel.lineBreakMode = .byTruncatingTail
    commandsContentLabel.maximumNumberOfLines = 5

    // Workspace path
    cwdLabel.font = NSFont.systemFont(ofSize: 10, weight: .regular)
    cwdLabel.textColor = .tertiaryLabelColor
    cwdLabel.lineBreakMode = .byTruncatingMiddle
    cwdLabel.maximumNumberOfLines = 1
    cwdLabel.allowsDefaultTighteningForTruncation = true

    // History section card
    historySectionCard.wantsLayer = true
    historySectionCard.layer?.cornerRadius = 6
    historySectionCard.layer?.backgroundColor =
      NSColor.controlBackgroundColor.withAlphaComponent(0.5).cgColor
    historySectionCard.layer?.borderWidth = 0.5
    historySectionCard.layer?.borderColor =
      NSColor.separatorColor.withAlphaComponent(0.3).cgColor

    historyTitleLabel.font = NSFont.systemFont(ofSize: 10, weight: .semibold)
    historyTitleLabel.textColor = .secondaryLabelColor
    historyTitleLabel.maximumNumberOfLines = 1

    historyScrollView.drawsBackground = false
    historyScrollView.borderType = .noBorder
    historyScrollView.hasVerticalScroller = true
    historyScrollView.autohidesScrollers = true
    historyScrollView.documentView = historyDocumentView
    historyDocumentView.wantsLayer = false

    // Button separator + buttons
    buttonSeparator.wantsLayer = true
    buttonSeparator.layer?.backgroundColor =
      NSColor.separatorColor.withAlphaComponent(0.3).cgColor

    openFinderButton.target = self
    openFinderButton.action = #selector(OnOpenFinderPressed)
    openFinderButton.bezelStyle = .rounded
    openFinderButton.controlSize = .small
    openFinderButton.font = NSFont.systemFont(ofSize: 11, weight: .regular)

    reconnectButton.target = self
    reconnectButton.action = #selector(OnReconnectPressed)
    reconnectButton.bezelStyle = .rounded
    reconnectButton.controlSize = .small
    reconnectButton.font = NSFont.systemFont(ofSize: 11, weight: .regular)

    // Timeline bar hover: update detail label text inline
    barView.OnHoveredSegmentChanged = { [weak self] segment in
      self?.HandleBarHover(segment: segment)
    }

    // Token bar hover: update token detail label text inline
    tokenBarView.OnHoveredSegmentChanged = { [weak self] segmentLabel in
      self?.HandleTokenBarHover(segmentLabel: segmentLabel)
    }

    // View hierarchy
    addSubview(statusDot)
    addSubview(nameLabel)
    addSubview(elapsedLabel)
    addSubview(chevronLabel)
    addSubview(barView)
    addSubview(detailLabel)
    addSubview(expandedScroll)
    expandedDocView.addSubview(promptLabel)
    expandedDocView.addSubview(gitModelLabel)
    expandedDocView.addSubview(tokenTitleLabel)
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
    expandedDocView.addSubview(historySectionCard)
    historySectionCard.addSubview(historyTitleLabel)
    historySectionCard.addSubview(historyScrollView)
    expandedDocView.addSubview(buttonSeparator)
    expandedDocView.addSubview(openFinderButton)
    expandedDocView.addSubview(reconnectButton)
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
    let nameAttrString = BuildNameAttributedString()
    guard let turn = endpointRow.activeTurn else {
      barVisible = false
      collapsedHeight = Self.collapsedIdleHeight
      nameLabel.attributedStringValue = nameAttrString
      elapsedLabel.stringValue = "Idle"
      statusDot.layer?.backgroundColor = NSColor.systemGray.withAlphaComponent(0.5).cgColor
      defaultDetailText = endpointRow.lastTraceLabel ?? "No active run"
      barView.Configure(segments: [])
      detailLabel.stringValue = defaultDetailText
      UpdateExpandedFields()
      ComputeExpandedHeight()
      needsLayout = true
      return
    }

    barVisible = true
    collapsedHeight = Self.collapsedActiveHeight
    nameLabel.attributedStringValue = nameAttrString
    statusDot.layer?.backgroundColor = StatusDotColor(turn.status).cgColor

    elapsedLabel.stringValue = "\(StatusLabel(turn.status)) \(turn.ElapsedString(now: now))"

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
    detailLabel.stringValue = defaultDetailText
    UpdateExpandedFields()
    ComputeExpandedHeight()
    needsLayout = true
  }

  private func UpdateExpandedFields() {
    // Git + Model line
    var gitModelParts: [String] = []
    if let branch = endpointRow.gitInfo?.branch {
      var part = branch
      if let sha = endpointRow.gitInfo?.sha {
        part += " · \(String(sha.prefix(7)))"
      }
      gitModelParts.append(part)
    }
    if let modelInfo = ModelSummary() {
      gitModelParts.append(modelInfo)
    }
    gitModelLabel.stringValue = gitModelParts.joined(separator: "   ")

    // Token usage
    if let usage = EffectiveTokenUsage() {
      if let cw = usage.contextWindow {
        tokenTitleLabel.stringValue =
          "Token Usage — \(FormatTokenCount(usage.totalTokens)) / \(FormatTokenCount(cw))"
      } else {
        tokenTitleLabel.stringValue = "Token Usage — \(FormatTokenCount(usage.totalTokens))"
      }
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
      defaultTokenDetailText = parts.joined(separator: " · ")
      tokenDetailLabel.stringValue = defaultTokenDetailText
    } else {
      defaultTokenDetailText = ""
    }

    if let promptText = PromptLabelText() {
      promptLabel.stringValue = promptText
    }

    // Error
    if let error = endpointRow.latestError {
      var errorText = error.message
      if error.willRetry { errorText += " (retrying…)" }
      errorLabel.stringValue = errorText
    }

    // Plan
    if !endpointRow.planSteps.isEmpty {
      let completed = endpointRow.planSteps.filter { $0.status == .completed }.count
      planTitleLabel.stringValue = "Plan (\(completed)/\(endpointRow.planSteps.count))"
      let lines = endpointRow.planSteps.prefix(6).map { step -> String in
        let icon: String
        switch step.status {
        case .completed: icon = "✓"
        case .inProgress: icon = "●"
        case .pending: icon = "○"
        }
        return " \(icon)  \(Truncate(step.description, limit: 52))"
      }
      planContentLabel.stringValue = lines.joined(separator: "\n")
    }

    // File changes
    if !endpointRow.fileChanges.isEmpty {
      filesTitleLabel.stringValue = "Files (\(endpointRow.fileChanges.count))"
      let lines = endpointRow.fileChanges.prefix(8).map { change -> String in
        let filename = (change.path as NSString).lastPathComponent
        let dir = (change.path as NSString).deletingLastPathComponent
        let shortDir = dir.isEmpty ? "" : "\(dir)/"
        return " \(change.kind.label)  \(shortDir)\(filename)"
      }
      filesContentLabel.stringValue = lines.joined(separator: "\n")
    }

    // Commands
    if !endpointRow.commands.isEmpty {
      commandsTitleLabel.stringValue = "Commands (\(endpointRow.commands.count))"
      let lines = endpointRow.commands.suffix(5).map { cmd -> String in
        let shortCmd = Truncate(cmd.command, limit: 38)
        var meta: [String] = []
        if let exitCode = cmd.exitCode { meta.append("exit \(exitCode)") }
        if let ms = cmd.durationMs {
          meta.append(String(format: "%.1fs", Double(ms) / 1000.0))
        }
        let suffix = meta.isEmpty ? "" : "  \(meta.joined(separator: "  "))"
        return " \(shortCmd)\(suffix)"
      }
      commandsContentLabel.stringValue = lines.joined(separator: "\n")
    }

    // Workspace path
    if let cwd = endpointRow.cwd {
      let shortPath = cwd.replacingOccurrences(of: NSHomeDirectory(), with: "~")
      cwdLabel.stringValue = "Workspace: \(shortPath)"
    }

    // History
    let runCount = endpointRow.recentRuns.count
    historyTitleLabel.stringValue = "Past Runs (\(runCount))"
    RebuildHistoryRows()
  }

  private func ComputeExpandedHeight() {
    guard isExpanded else {
      computedExpandedHeight = 0
      return
    }
    var h: CGFloat = 8
    let spc: CGFloat = 10

    // Git/model
    if HasGitOrModelInfo() { h += 16 }
    // Token usage
    if EffectiveTokenUsage() != nil { h += 15 + 18 + 16 }
    // Prompt
    if PromptLabelText() != nil { h += 28 }
    // Error
    if endpointRow.latestError != nil { h += 34 + spc }
    // Plan
    if !endpointRow.planSteps.isEmpty {
      h += 15 + CGFloat(min(endpointRow.planSteps.count, 6)) * 14 + 2 + spc
    }
    // Files
    if !endpointRow.fileChanges.isEmpty {
      h += 15 + CGFloat(min(endpointRow.fileChanges.count, 8)) * 14 + 2 + spc
    }
    // Commands
    if !endpointRow.commands.isEmpty {
      h += 15 + CGFloat(min(endpointRow.commands.count, 5)) * 14 + 2 + spc
    }
    // CWD
    if endpointRow.cwd != nil { h += 16 }
    // History
    if !historyRunViews.isEmpty {
      let runH = HistoryScrollHeight()
      h += 14 + 4 + runH + 12 + spc
    }
    // Separator + buttons
    h += 1 + 8 + 24 + spc
    h += 8

    computedExpandedHeight = min(h, Self.maxExpandedHeight - collapsedHeight)
  }

  private func RebuildHistoryRows() {
    for row in historyRunViews { row.removeFromSuperview() }
    historyRunViews.removeAll(keepingCapacity: true)
    for (index, run) in endpointRow.recentRuns.enumerated() {
      let historyRow = RunHistoryRowView(frame: .zero)
      let runKey = run.runKey
      historyRow.Configure(
        run: run,
        isLastRun: index == 0,
        isExpanded: expandedRunKeys.contains(runKey),
        onToggle: { [weak self] in
          guard let self else { return }
          self.onToggleHistoryRun?(self.endpointRow.endpointId, runKey)
        })
      historyDocumentView.addSubview(historyRow)
      historyRunViews.append(historyRow)
    }
    needsLayout = true
  }

  private func LayoutHistoryRows() {
    let rowSpacing: CGFloat = 4
    let contentWidth = max(0, historyScrollView.bounds.width)
    var y: CGFloat = rowSpacing
    for row in historyRunViews.reversed() {
      row.frame = NSRect(x: 0, y: y, width: contentWidth, height: row.preferredHeight)
      y += row.preferredHeight + rowSpacing
    }
    let contentHeight = max(historyScrollView.bounds.height, y)
    historyDocumentView.frame = NSRect(x: 0, y: 0, width: contentWidth, height: contentHeight)
  }

  private func HistoryContentHeight() -> CGFloat {
    let rowSpacing: CGFloat = 4
    var height = rowSpacing
    for row in historyRunViews {
      height += row.preferredHeight + rowSpacing
    }
    return max(0, height)
  }

  private func HistoryScrollHeight() -> CGFloat {
    min(220, HistoryContentHeight())
  }

  private func EffectiveTokenUsage() -> TokenUsageInfo? {
    if let usage = endpointRow.tokenUsage, usage.totalTokens > 0 {
      return usage
    }
    if endpointRow.activeTurn != nil {
      return TokenUsageInfo()
    }
    return nil
  }

  private func PromptLabelText() -> String? {
    if let promptPreview = endpointRow.promptPreview, !promptPreview.isEmpty {
      return "Prompt: \(Truncate(promptPreview, limit: 130))"
    }
    if endpointRow.activeTurn != nil {
      return "Prompt: waiting for first user message"
    }
    return nil
  }

  private func ModelSummary() -> String? {
    let model = endpointRow.model?.trimmingCharacters(in: .whitespacesAndNewlines)
    let provider = endpointRow.modelProvider?.trimmingCharacters(in: .whitespacesAndNewlines)
    if let model, !model.isEmpty, let provider, !provider.isEmpty {
      return "Model: \(model) (\(provider))"
    }
    if let model, !model.isEmpty {
      return "Model: \(model)"
    }
    if let provider, !provider.isEmpty {
      return "Provider: \(provider)"
    }
    return nil
  }

  private func HasGitOrModelInfo() -> Bool {
    endpointRow.gitInfo?.branch != nil || ModelSummary() != nil
  }

  // MARK: - Hover handlers

  private func HandleBarHover(segment: TimelineSegment?) {
    guard barVisible else {
      detailLabel.stringValue = defaultDetailText
      detailLabel.textColor = .secondaryLabelColor
      return
    }
    guard let segment else {
      detailLabel.stringValue = defaultDetailText
      detailLabel.textColor = .secondaryLabelColor
      return
    }
    let color = SegmentFillColor(segment.kind)
    detailLabel.textColor = color
    detailLabel.stringValue = HoverText(segment: segment)
  }

  private func HandleTokenBarHover(segmentLabel: String?) {
    guard let segmentLabel else {
      tokenDetailLabel.stringValue = defaultTokenDetailText
      tokenDetailLabel.textColor = .tertiaryLabelColor
      return
    }
    tokenDetailLabel.stringValue = segmentLabel
    tokenDetailLabel.textColor = .secondaryLabelColor
  }

  // MARK: - Formatting helpers

  private func HoverText(segment: TimelineSegment) -> String {
    let category = SegmentKindLabel(segment.kind)
    let duration = FormatDuration(segment.duration)
    let start = FormatClockTime(segment.startedAt)
    let end = FormatClockTime(segment.endedAt)
    if let label = segment.label, !label.isEmpty {
      return "\(category) · \(duration) · \(start)–\(end) · \(label)"
    }
    return "\(category) · \(duration) · \(start)–\(end)"
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
    case .inProgress: return .systemGreen
    case .completed: return .systemGray
    case .interrupted: return .systemOrange
    case .failed: return .systemRed
    }
  }

  private func BuildNameAttributedString() -> NSAttributedString {
    let name = endpointRow.displayName
    let shortId = endpointRow.shortId
    let nameFont = NSFont.systemFont(ofSize: 12, weight: .semibold)

    let hasCwd = endpointRow.cwd != nil
    let hasTitle = endpointRow.chatTitle != nil && !endpointRow.chatTitle!.isEmpty
    let showIdSuffix = hasCwd || hasTitle

    if showIdSuffix {
      let result = NSMutableAttributedString(
        string: name,
        attributes: [.font: nameFont, .foregroundColor: NSColor.labelColor])
      let idFont = NSFont.systemFont(ofSize: 11, weight: .regular)
      let idPart = NSAttributedString(
        string: " (\(shortId))",
        attributes: [.font: idFont, .foregroundColor: NSColor.tertiaryLabelColor])
      result.append(idPart)
      return result
    }

    return NSAttributedString(
      string: name,
      attributes: [.font: nameFont, .foregroundColor: NSColor.labelColor])
  }

  private func Truncate(_ value: String, limit: Int) -> String {
    if value.count <= limit { return value }
    return "\(value.prefix(max(0, limit - 1)))…"
  }
}

// MARK: - RunHistoryRowView

final class RunHistoryRowView: NSView {
  static let collapsedHeight: CGFloat = 20
  static let expandedHeightWithoutTokens: CGFloat = 78
  static let expandedHeightWithTokens: CGFloat = 98

  private(set) var preferredHeight: CGFloat = collapsedHeight

  private let statusDot = NSView()
  private let titleLabel = NSTextField(labelWithString: "")
  private let chevronLabel = NSTextField(labelWithString: "")
  private let promptLabel = NSTextField(labelWithString: "")
  private let modelLabel = NSTextField(labelWithString: "")
  private let timelineBarView = TimelineBarView()
  private let tokenBarView = TokenUsageBarView()
  private let tokenDetailLabel = NSTextField(labelWithString: "")
  private var defaultTitleText = ""
  private var defaultTokenDetailText = ""
  private var isExpandedRow = false
  private var hasTokenUsage = false
  private var onToggle: (() -> Void)?

  override init(frame frameRect: NSRect) {
    super.init(frame: frameRect)
    wantsLayer = true
    layer?.cornerRadius = 4
    layer?.backgroundColor = NSColor.controlBackgroundColor.withAlphaComponent(0.35).cgColor

    statusDot.wantsLayer = true
    statusDot.layer?.cornerRadius = 3

    titleLabel.font = NSFont.monospacedDigitSystemFont(ofSize: 10, weight: .regular)
    titleLabel.textColor = .secondaryLabelColor
    titleLabel.lineBreakMode = .byTruncatingTail
    titleLabel.maximumNumberOfLines = 1

    chevronLabel.font = NSFont.systemFont(ofSize: 9, weight: .medium)
    chevronLabel.textColor = .tertiaryLabelColor
    chevronLabel.alignment = .center
    chevronLabel.maximumNumberOfLines = 1

    promptLabel.font = NSFont.systemFont(ofSize: 10, weight: .regular)
    promptLabel.textColor = .secondaryLabelColor
    promptLabel.lineBreakMode = .byTruncatingTail
    promptLabel.maximumNumberOfLines = 2

    modelLabel.font = NSFont.systemFont(ofSize: 10, weight: .medium)
    modelLabel.textColor = .tertiaryLabelColor
    modelLabel.lineBreakMode = .byTruncatingTail
    modelLabel.maximumNumberOfLines = 1

    timelineBarView.OnHoveredSegmentChanged = { [weak self] segment in
      self?.HandleTimelineHover(segment: segment)
    }

    tokenBarView.OnHoveredSegmentChanged = { [weak self] segmentLabel in
      self?.HandleTokenBarHover(segmentLabel: segmentLabel)
    }

    tokenDetailLabel.font = NSFont.monospacedDigitSystemFont(ofSize: 10, weight: .regular)
    tokenDetailLabel.textColor = .tertiaryLabelColor
    tokenDetailLabel.lineBreakMode = .byTruncatingTail
    tokenDetailLabel.maximumNumberOfLines = 1

    addSubview(statusDot)
    addSubview(titleLabel)
    addSubview(chevronLabel)
    addSubview(promptLabel)
    addSubview(modelLabel)
    addSubview(timelineBarView)
    addSubview(tokenBarView)
    addSubview(tokenDetailLabel)
  }

  @available(*, unavailable)
  required init?(coder: NSCoder) {
    fatalError("init(coder:) has not been implemented")
  }

  override func mouseDown(with event: NSEvent) {
    _ = event
    onToggle?()
  }

  override func layout() {
    super.layout()

    let dotSize: CGFloat = 6
    statusDot.frame = NSRect(x: 2, y: bounds.height - 12, width: dotSize, height: dotSize)

    let chevronWidth: CGFloat = 12
    chevronLabel.frame = NSRect(
      x: bounds.width - chevronWidth - 2,
      y: bounds.height - 15,
      width: chevronWidth,
      height: 12
    )

    titleLabel.frame = NSRect(
      x: dotSize + 6,
      y: bounds.height - 15,
      width: max(0, bounds.width - dotSize - chevronWidth - 10),
      height: 12
    )

    guard isExpandedRow else {
      promptLabel.isHidden = true
      modelLabel.isHidden = true
      timelineBarView.isHidden = true
      tokenBarView.isHidden = true
      tokenDetailLabel.isHidden = true
      return
    }

    let inset: CGFloat = 2
    let contentWidth = max(0, bounds.width - inset * 2)
    var y = bounds.height - 19

    promptLabel.isHidden = false
    promptLabel.frame = NSRect(x: inset, y: y - 24, width: contentWidth, height: 24)
    y -= 28

    modelLabel.isHidden = false
    modelLabel.frame = NSRect(x: inset, y: y - 12, width: contentWidth, height: 12)
    y -= 16

    timelineBarView.isHidden = false
    timelineBarView.frame = NSRect(x: inset, y: y - 8, width: contentWidth, height: 8)
    y -= 12

    if hasTokenUsage {
      tokenBarView.isHidden = false
      tokenDetailLabel.isHidden = false
      tokenBarView.frame = NSRect(x: inset, y: y - 10, width: contentWidth, height: 10)
      y -= 13
      tokenDetailLabel.frame = NSRect(x: inset, y: y - 12, width: contentWidth, height: 12)
    } else {
      tokenBarView.isHidden = true
      tokenDetailLabel.isHidden = true
    }
  }

  func Configure(
    run: CompletedRun,
    isLastRun: Bool,
    isExpanded: Bool,
    onToggle: (() -> Void)?
  ) {
    self.onToggle = onToggle
    isExpandedRow = isExpanded
    hasTokenUsage = run.tokenUsage?.totalTokens ?? 0 > 0
    preferredHeight =
      isExpanded
      ? (hasTokenUsage ? Self.expandedHeightWithTokens : Self.expandedHeightWithoutTokens)
      : Self.collapsedHeight
    chevronLabel.stringValue = isExpanded ? "▾" : "▸"

    let elapsed = run.ElapsedString()
    let status = StatusText(run.status)
    let ranAt = run.RanAtString()
    let suffix = isLastRun ? " · latest" : ""
    defaultTitleText = "\(status) · \(elapsed) · \(ranAt)\(suffix)"
    titleLabel.stringValue = defaultTitleText
    statusDot.layer?.backgroundColor = StatusColor(run.status).cgColor

    let promptText: String
    if let promptPreview = run.promptPreview, !promptPreview.isEmpty {
      promptText = promptPreview
    } else {
      promptText = "Prompt unavailable"
    }
    promptLabel.stringValue = "Prompt: \(promptText)"
    modelLabel.stringValue = ModelText(run: run)

    timelineBarView.Configure(segments: run.TimelineSegments())

    if let usage = run.tokenUsage, usage.totalTokens > 0 {
      tokenBarView.Configure(usage: usage)
      var tokenParts = ["In: \(FormatTokenCount(usage.inputTokens))"]
      tokenParts.append("Out: \(FormatTokenCount(usage.outputTokens))")
      if usage.reasoningTokens > 0 {
        tokenParts.append("Reasoning: \(FormatTokenCount(usage.reasoningTokens))")
      }
      defaultTokenDetailText = tokenParts.joined(separator: " · ")
      tokenDetailLabel.stringValue = defaultTokenDetailText
    } else {
      defaultTokenDetailText = ""
      tokenDetailLabel.stringValue = defaultTokenDetailText
    }

    needsLayout = true
    invalidateIntrinsicContentSize()
  }

  private func HandleTimelineHover(segment: TimelineSegment?) {
    guard isExpandedRow else { return }
    guard let segment else {
      titleLabel.stringValue = defaultTitleText
      titleLabel.textColor = .secondaryLabelColor
      return
    }
    let color = SegmentFillColor(segment.kind)
    titleLabel.textColor = color
    titleLabel.stringValue = HoverText(segment: segment)
  }

  private func HoverText(segment: TimelineSegment) -> String {
    let category = SegmentKindLabel(segment.kind)
    let duration = FormatDuration(segment.duration)
    if let label = segment.label, !label.isEmpty {
      return "\(category) · \(duration) · \(label)"
    }
    return "\(category) · \(duration)"
  }

  private func HandleTokenBarHover(segmentLabel: String?) {
    guard isExpandedRow, hasTokenUsage else {
      tokenDetailLabel.stringValue = defaultTokenDetailText
      tokenDetailLabel.textColor = .tertiaryLabelColor
      return
    }

    guard let segmentLabel else {
      tokenDetailLabel.stringValue = defaultTokenDetailText
      tokenDetailLabel.textColor = .tertiaryLabelColor
      return
    }

    tokenDetailLabel.stringValue = segmentLabel
    tokenDetailLabel.textColor = .secondaryLabelColor
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

  private func ModelText(run: CompletedRun) -> String {
    let model = run.model?.trimmingCharacters(in: .whitespacesAndNewlines)
    let provider = run.modelProvider?.trimmingCharacters(in: .whitespacesAndNewlines)
    if let model, !model.isEmpty, let provider, !provider.isEmpty {
      return "Model: \(model) (\(provider))"
    }
    if let model, !model.isEmpty {
      return "Model: \(model)"
    }
    if let provider, !provider.isEmpty {
      return "Provider: \(provider)"
    }
    return "Model: unavailable"
  }
}

// MARK: - TokenUsageBarView

final class TokenUsageBarView: NSView {
  var OnHoveredSegmentChanged: ((String?) -> Void)?

  private var usage: TokenUsageInfo?
  private var segmentRects: [(label: String, count: Int, color: NSColor, rect: CGRect)] = []
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

  func Configure(usage: TokenUsageInfo) {
    self.usage = usage
    hoverIndex = nil
    segmentRects = []
    OnHoveredSegmentChanged?(nil)
    needsDisplay = true
  }

  override func updateTrackingAreas() {
    super.updateTrackingAreas()
    if let trackingArea { removeTrackingArea(trackingArea) }
    let area = NSTrackingArea(
      rect: bounds,
      options: [.activeAlways, .mouseEnteredAndExited, .mouseMoved, .inVisibleRect],
      owner: self, userInfo: nil)
    addTrackingArea(area)
    trackingArea = area
  }

  override func mouseMoved(with event: NSEvent) {
    let loc = convert(event.locationInWindow, from: nil)
    let nextIndex = segmentRects.firstIndex(where: { $0.rect.contains(loc) })
    if nextIndex == hoverIndex { return }
    hoverIndex = nextIndex
    if let nextIndex {
      let seg = segmentRects[nextIndex]
      OnHoveredSegmentChanged?("\(seg.label): \(FormatTokenCount(seg.count))")
    } else {
      OnHoveredSegmentChanged?(nil)
    }
    needsDisplay = true
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
    guard let usage else { return }
    let trackRect = bounds.insetBy(dx: 0.5, dy: 0.5)
    guard trackRect.width > 0, trackRect.height > 0 else { return }

    let trackPath = NSBezierPath(roundedRect: trackRect, xRadius: 4, yRadius: 4)
    NSColor.controlBackgroundColor.withAlphaComponent(0.8).setFill()
    trackPath.fill()

    let segments = BuildUsageSegments(usage)
    let total = segments.reduce(0.0) { $0 + $1.1 }

    segmentRects = []

    if total > 0 {
      NSGraphicsContext.saveGraphicsState()
      trackPath.addClip()

      let maxWidth: CGFloat
      if let cw = usage.contextWindow, cw > 0 {
        maxWidth = trackRect.width * CGFloat(min(1.0, Double(usage.totalTokens) / Double(cw)))
      } else {
        maxWidth = trackRect.width
      }

      var x = trackRect.minX
      for (label, value, color) in segments {
        let w = maxWidth * CGFloat(value / total)
        let rect = NSRect(x: x, y: trackRect.minY, width: max(w, 0), height: trackRect.height)
        segmentRects.append((label: label, count: Int(value), color: color, rect: rect))
        if w > 0.5 {
          color.setFill()
          NSBezierPath(rect: rect).fill()
        }
        x += w
      }

      NSGraphicsContext.restoreGraphicsState()
    }

    NSColor.separatorColor.withAlphaComponent(0.5).setStroke()
    trackPath.lineWidth = 0.5
    trackPath.stroke()

    if let hoverIndex, hoverIndex < segmentRects.count {
      let rect = segmentRects[hoverIndex].rect
      if rect.width > 0.5 {
        NSColor.labelColor.withAlphaComponent(0.4).setStroke()
        let hl = NSBezierPath(
          roundedRect: rect.insetBy(dx: 0.5, dy: 0.5), xRadius: 2, yRadius: 2)
        hl.lineWidth = 1.0
        hl.stroke()
      }
    }
  }

  private func BuildUsageSegments(_ usage: TokenUsageInfo) -> [(String, Double, NSColor)] {
    var segments: [(String, Double, NSColor)] = []
    let cached = usage.cachedInputTokens
    let freshInput = max(0, usage.inputTokens - cached)
    if cached > 0 {
      segments.append(("Cached Input", Double(cached), NSColor.systemGray.withAlphaComponent(0.5)))
    }
    if freshInput > 0 {
      segments.append(
        ("Input", Double(freshInput), NSColor.controlAccentColor.withAlphaComponent(0.45)))
    }
    if usage.reasoningTokens > 0 {
      segments.append(
        ("Reasoning", Double(usage.reasoningTokens), NSColor.systemPink.withAlphaComponent(0.55)))
    }
    let regularOutput = max(0, usage.outputTokens - usage.reasoningTokens)
    if regularOutput > 0 {
      segments.append(
        ("Output", Double(regularOutput), NSColor.systemGreen.withAlphaComponent(0.55)))
    }
    return segments
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
    if let trackingArea { removeTrackingArea(trackingArea) }
    let area = NSTrackingArea(
      rect: bounds,
      options: [.activeAlways, .mouseEnteredAndExited, .mouseMoved, .inVisibleRect],
      owner: self, userInfo: nil)
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
      NSColor.separatorColor.withAlphaComponent(0.4).setStroke()
      for index in 1..<widths.count {
        let prev = segmentRects[index - 1]
        let curr = segmentRects[index]
        if prev.isNull || curr.isNull { continue }
        let sep = NSBezierPath()
        sep.move(to: CGPoint(x: curr.minX, y: trackRect.minY))
        sep.line(to: CGPoint(x: curr.minX, y: trackRect.maxY))
        sep.lineWidth = 0.5
        sep.stroke()
      }
    }

    NSGraphicsContext.restoreGraphicsState()

    NSColor.separatorColor.withAlphaComponent(0.5).setStroke()
    trackPath.lineWidth = 0.5
    trackPath.stroke()

    if let hoverIndex, hoverIndex < segmentRects.count {
      let rect = segmentRects[hoverIndex]
      if !rect.isNull {
        NSColor.labelColor.withAlphaComponent(0.4).setStroke()
        let hl = NSBezierPath(
          roundedRect: rect.insetBy(dx: 0.5, dy: 0.5), xRadius: 2, yRadius: 2)
        hl.lineWidth = 1.0
        hl.stroke()
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
    for i in widths.indices where widths[i] < minimumWidths[i] { widths[i] = minimumWidths[i] }
    var assigned = widths.reduce(0, +)
    if assigned > totalWidth {
      var reducible = widths.indices.filter { widths[$0] > minimumWidths[$0] }
      while assigned > totalWidth && !reducible.isEmpty {
        reducible.sort {
          remainders[$0] != remainders[$1]
            ? remainders[$0] < remainders[$1] : widths[$0] > widths[$1]
        }
        guard let i = reducible.first else { break }
        widths[i] -= 1; assigned -= 1
        reducible = widths.indices.filter { widths[$0] > minimumWidths[$0] }
      }
      if assigned > totalWidth {
        var positive = widths.indices.filter { widths[$0] > 0 }
        while assigned > totalWidth && !positive.isEmpty {
          positive.sort { widths[$0] > widths[$1] }
          guard let i = positive.first else { break }
          widths[i] -= 1; assigned -= 1
          positive = widths.indices.filter { widths[$0] > 0 }
        }
      }
    }
    if assigned < totalWidth {
      let order = widths.indices.sorted {
        remainders[$0] != remainders[$1]
          ? remainders[$0] > remainders[$1] : durations[$0] > durations[$1]
      }
      if !order.isEmpty {
        var c = 0
        while assigned < totalWidth {
          widths[order[c % order.count]] += 1; assigned += 1; c += 1
        }
      }
    }
    return widths
  }
}

// MARK: - Shared formatting

private let durationFormatter: DateComponentsFormatter = {
  let f = DateComponentsFormatter()
  f.allowedUnits = [.hour, .minute, .second]
  f.unitsStyle = .abbreviated
  f.maximumUnitCount = 2
  f.zeroFormattingBehavior = [.dropLeading]
  return f
}()

private let clockTimeFormatter: DateFormatter = {
  let f = DateFormatter()
  f.timeStyle = .medium
  f.dateStyle = .none
  return f
}()

private func SegmentFillColor(_ kind: TimelineSegmentKind) -> NSColor {
  switch kind {
  case .category(let c):
    switch c {
    case .tool: return NSColor.systemIndigo.withAlphaComponent(0.85)
    case .edit: return NSColor.systemPurple.withAlphaComponent(0.85)
    case .waiting: return NSColor.systemRed.withAlphaComponent(0.85)
    case .network: return NSColor.systemBlue.withAlphaComponent(0.85)
    case .prefill: return NSColor.systemOrange.withAlphaComponent(0.85)
    case .reasoning: return NSColor.systemPink.withAlphaComponent(0.85)
    case .gen: return NSColor.systemGreen.withAlphaComponent(0.85)
    }
  case .idle:
    return NSColor.systemGray.withAlphaComponent(0.3)
  }
}

private func SegmentKindLabel(_ kind: TimelineSegmentKind) -> String {
  switch kind {
  case .category(let c):
    switch c {
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
