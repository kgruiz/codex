import AppKit
import Foundation

final class TurnMenuRowView: NSView {
  private static let rowWidth: CGFloat = 420
  private static let collapsedRowHeight: CGFloat = 64
  private static let expandedRowHeight: CGFloat = 172

  private let endpointRow: EndpointRow
  private let isExpanded: Bool
  private let onToggle: ((String) -> Void)?
  private let onReconnectEndpoint: ((String) -> Void)?

  private let topLabel = NSTextField(labelWithString: "")
  private let detailLabel = NSTextField(labelWithString: "")
  private let barView = TimelineBarView()
  private let hoverCard = NSView()
  private let hoverColorSwatch = NSView()
  private let hoverLabel = NSTextField(labelWithString: "")
  private let expandedContainer = NSView()
  private let chatLabel = NSTextField(labelWithString: "")
  private let promptLabel = NSTextField(labelWithString: "")
  private let metadataLabel = NSTextField(labelWithString: "")
  private let reconnectButton = NSButton(title: "Reconnect this endpoint", target: nil, action: nil)

  private var defaultDetailText = "No detail"
  private var barVisible = true

  init(
    endpointRow: EndpointRow,
    now: Date,
    isExpanded: Bool,
    onToggle: ((String) -> Void)?,
    onReconnectEndpoint: ((String) -> Void)?
  ) {
    self.endpointRow = endpointRow
    self.isExpanded = isExpanded
    self.onToggle = onToggle
    self.onReconnectEndpoint = onReconnectEndpoint
    let rowHeight = isExpanded ? Self.expandedRowHeight : Self.collapsedRowHeight
    super.init(frame: NSRect(x: 0, y: 0, width: Self.rowWidth, height: rowHeight))
    ConfigureViews()
    Update(now: now)
  }

  @available(*, unavailable)
  required init?(coder: NSCoder) {
    fatalError("init(coder:) has not been implemented")
  }

  override var intrinsicContentSize: NSSize {
    NSSize(
      width: Self.rowWidth,
      height: isExpanded ? Self.expandedRowHeight : Self.collapsedRowHeight
    )
  }

  override func mouseDown(with event: NSEvent) {
    let pointInSelf = convert(event.locationInWindow, from: nil)
    if isExpanded {
      let pointInExpanded = convert(pointInSelf, to: expandedContainer)
      if reconnectButton.frame.contains(pointInExpanded) {
        super.mouseDown(with: event)
        return
      }
    }
    onToggle?(endpointRow.endpointId)
  }

  override func layout() {
    super.layout()

    let insets = NSEdgeInsets(top: 8, left: 12, bottom: 8, right: 12)
    let contentRect = NSRect(
      x: insets.left,
      y: insets.bottom,
      width: max(0, bounds.width - insets.left - insets.right),
      height: max(0, bounds.height - insets.top - insets.bottom)
    )

    let topHeight: CGFloat = 14
    let barHeight: CGFloat = 12
    let detailHeight: CGFloat = 16
    let verticalSpacing: CGFloat = 4
    let expandedHeight: CGFloat = isExpanded ? 98 : 0

    detailLabel.frame = NSRect(
      x: contentRect.minX,
      y: contentRect.minY + expandedHeight,
      width: contentRect.width,
      height: detailHeight
    )

    barView.frame = NSRect(
      x: contentRect.minX,
      y: detailLabel.frame.maxY + verticalSpacing,
      width: contentRect.width,
      height: barHeight
    )
    barView.isHidden = !barVisible

    topLabel.frame = NSRect(
      x: contentRect.minX,
      y: barView.frame.maxY + verticalSpacing,
      width: contentRect.width,
      height: topHeight
    )

    hoverCard.frame = detailLabel.frame
    let swatchSize: CGFloat = 7
    hoverColorSwatch.frame = NSRect(
      x: 8,
      y: (hoverCard.bounds.height - swatchSize) / 2,
      width: swatchSize,
      height: swatchSize
    )
    hoverLabel.frame = NSRect(
      x: hoverColorSwatch.frame.maxX + 6,
      y: 0,
      width: max(0, hoverCard.bounds.width - hoverColorSwatch.frame.maxX - 14),
      height: hoverCard.bounds.height
    )

    expandedContainer.frame = NSRect(
      x: contentRect.minX,
      y: contentRect.minY,
      width: contentRect.width,
      height: expandedHeight
    )
    expandedContainer.isHidden = !isExpanded

    if isExpanded {
      let innerInset: CGFloat = 8
      let availableWidth = max(0, expandedContainer.bounds.width - (innerInset * 2))

      chatLabel.frame = NSRect(
        x: innerInset,
        y: expandedContainer.bounds.height - 16,
        width: availableWidth,
        height: 14
      )
      promptLabel.frame = NSRect(
        x: innerInset,
        y: expandedContainer.bounds.height - 47,
        width: availableWidth,
        height: 28
      )
      metadataLabel.frame = NSRect(
        x: innerInset,
        y: 22,
        width: availableWidth,
        height: 36
      )
      reconnectButton.frame = NSRect(
        x: max(innerInset, availableWidth - 170 + innerInset),
        y: 2,
        width: 170,
        height: 18
      )
    }
  }

  private func ConfigureViews() {
    wantsLayer = true

    topLabel.font = NSFont.systemFont(ofSize: 12, weight: .semibold)
    topLabel.lineBreakMode = .byTruncatingTail
    topLabel.maximumNumberOfLines = 1

    detailLabel.font = NSFont.systemFont(ofSize: 11, weight: .regular)
    detailLabel.textColor = .secondaryLabelColor
    detailLabel.lineBreakMode = .byTruncatingTail
    detailLabel.maximumNumberOfLines = 1

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

    expandedContainer.wantsLayer = true
    expandedContainer.layer?.cornerRadius = 4
    expandedContainer.layer?.backgroundColor =
      NSColor.controlBackgroundColor.withAlphaComponent(0.5).cgColor

    chatLabel.font = NSFont.systemFont(ofSize: 11, weight: .semibold)
    chatLabel.lineBreakMode = .byTruncatingTail
    chatLabel.maximumNumberOfLines = 1

    promptLabel.font = NSFont.systemFont(ofSize: 11, weight: .regular)
    promptLabel.textColor = .secondaryLabelColor
    promptLabel.lineBreakMode = .byTruncatingTail
    promptLabel.maximumNumberOfLines = 2

    metadataLabel.font = NSFont.systemFont(ofSize: 10, weight: .regular)
    metadataLabel.textColor = .secondaryLabelColor
    metadataLabel.lineBreakMode = .byTruncatingTail
    metadataLabel.maximumNumberOfLines = 3

    reconnectButton.target = self
    reconnectButton.action = #selector(OnReconnectPressed)
    reconnectButton.bezelStyle = .rounded
    reconnectButton.font = NSFont.systemFont(ofSize: 10, weight: .medium)

    barView.OnHoveredSegmentChanged = { [weak self] segment in
      self?.UpdateHoverText(segment: segment)
    }

    addSubview(topLabel)
    addSubview(barView)
    addSubview(detailLabel)
    addSubview(hoverCard)
    hoverCard.addSubview(hoverColorSwatch)
    hoverCard.addSubview(hoverLabel)
    addSubview(expandedContainer)
    expandedContainer.addSubview(chatLabel)
    expandedContainer.addSubview(promptLabel)
    expandedContainer.addSubview(metadataLabel)
    expandedContainer.addSubview(reconnectButton)
  }

  @objc
  private func OnReconnectPressed() {
    onReconnectEndpoint?(endpointRow.endpointId)
  }

  private func Update(now: Date) {
    let shortEndpointId = String(endpointRow.endpointId.prefix(8))
    guard let turn = endpointRow.activeTurn else {
      barVisible = false
      topLabel.stringValue = "Codex \(shortEndpointId) · Idle"
      defaultDetailText = endpointRow.lastTraceLabel ?? "No active run"
      barView.Configure(segments: [])
      ShowDefaultDetail()
      UpdateExpandedFields(now: now)
      needsLayout = true
      return
    }

    barVisible = true
    let shortThreadId = String(turn.threadId.prefix(8))
    topLabel.stringValue =
      "Codex \(shortEndpointId) · \(StatusLabel(turn.status)) \(turn.ElapsedString(now: now)) · [\(shortThreadId)/\(turn.turnId)]"
    defaultDetailText = endpointRow.lastTraceLabel ?? turn.latestLabel ?? "No detail"
    barView.Configure(segments: turn.TimelineSegments(now: now))
    ShowDefaultDetail()
    UpdateExpandedFields(now: now)
    needsLayout = true
  }

  private func UpdateExpandedFields(now: Date) {
    let chatTitle = endpointRow.chatTitle ?? endpointRow.threadId ?? "Unknown chat"
    chatLabel.stringValue = "Chat: \(Truncate(chatTitle, limit: 72))"

    let promptPreview = endpointRow.promptPreview ?? "No prompt available"
    promptLabel.stringValue = "Prompt: \(Truncate(promptPreview, limit: 180))"

    var metadataParts: [String] = []
    if let activeTurn = endpointRow.activeTurn {
      metadataParts.append(
        "Status: \(StatusLabel(activeTurn.status)) · \(activeTurn.ElapsedString(now: now))")
    } else {
      metadataParts.append("Status: Idle")
    }
    metadataParts.append("Workspace: \(Truncate(endpointRow.cwd ?? "unknown", limit: 58))")
    metadataParts.append("Model: \(Truncate(endpointRow.model ?? "unknown", limit: 36))")

    var traceSuffix = "none"
    if let category = endpointRow.lastTraceCategory {
      traceSuffix = SegmentKindLabel(.category(category))
      if let label = endpointRow.lastTraceLabel, !label.isEmpty {
        traceSuffix += " · \(Truncate(label, limit: 36))"
      }
    }
    metadataParts.append("Trace: \(traceSuffix)")

    let threadPart = endpointRow.threadId.map { String($0.prefix(8)) } ?? "n/a"
    let turnPart = endpointRow.turnId ?? "n/a"
    metadataParts.append("IDs: \(threadPart)/\(turnPart)")

    if let lastEventAt = endpointRow.lastEventAt {
      metadataParts.append("Updated: \(FormatClockTime(lastEventAt))")
    }

    metadataLabel.stringValue = metadataParts.joined(separator: "\n")
  }

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

  private func Truncate(_ value: String, limit: Int) -> String {
    if value.count <= limit {
      return value
    }
    let truncated = value.prefix(max(0, limit - 1))
    return "\(truncated)…"
  }
}

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
      .activeAlways,
      .mouseEnteredAndExited,
      .mouseMoved,
      .inVisibleRect,
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
    if trackRect.width <= 0 || trackRect.height <= 0 {
      return
    }

    let trackPath = NSBezierPath(roundedRect: trackRect, xRadius: 6, yRadius: 6)
    NSColor.controlBackgroundColor.withAlphaComponent(0.9).setFill()
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
        x: x,
        y: trackRect.minY,
        width: CGFloat(width),
        height: trackRect.height
      ).intersection(trackRect)
      segmentRects.append(segmentRect)
      x += CGFloat(width)

      SegmentFillColor(segments[index].kind).setFill()
      NSBezierPath(rect: segmentRect).fill()
    }

    if widths.count >= 2 {
      NSColor.separatorColor.withAlphaComponent(0.65).setStroke()
      for index in 1..<widths.count {
        let previousRect = segmentRects[index - 1]
        let currentRect = segmentRects[index]
        if previousRect.isNull || currentRect.isNull {
          continue
        }

        let boundaryX = currentRect.minX
        let separator = NSBezierPath()
        separator.move(to: CGPoint(x: boundaryX, y: trackRect.minY))
        separator.line(to: CGPoint(x: boundaryX, y: trackRect.maxY))
        separator.lineWidth = 1
        separator.stroke()
      }
    }

    NSGraphicsContext.restoreGraphicsState()

    NSColor.separatorColor.withAlphaComponent(0.9).setStroke()
    trackPath.lineWidth = 1
    trackPath.stroke()

    if let hoverIndex, hoverIndex < segmentRects.count {
      let rect = segmentRects[hoverIndex]
      if !rect.isNull {
        NSColor.controlAccentColor.withAlphaComponent(0.95).setStroke()
        let highlightPath = NSBezierPath(
          roundedRect: rect.insetBy(dx: 0.5, dy: 0.5), xRadius: 4, yRadius: 4)
        highlightPath.lineWidth = 1.2
        highlightPath.stroke()
      }
    }
  }

  private func UpdateHoverIndex(location: CGPoint) {
    let nextIndex = segmentRects.firstIndex(where: { !$0.isNull && $0.contains(location) })
    if nextIndex == hoverIndex {
      return
    }

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
      return durations.indices.map { index in
        base + (index < remainder ? 1 : 0)
      }
    }

    let exactWidths = durations.map { ($0 / totalDuration) * Double(totalWidth) }
    var widths = exactWidths.map { Int($0.rounded(.down)) }
    let remainders = exactWidths.map { $0 - Double(Int($0.rounded(.down))) }
    let minimumWidths = exactWidths.map { $0 > 0 ? 1 : 0 }

    for index in widths.indices {
      if widths[index] < minimumWidths[index] {
        widths[index] = minimumWidths[index]
      }
    }

    var assigned = widths.reduce(0, +)

    if assigned > totalWidth {
      var reducible = widths.indices.filter { widths[$0] > minimumWidths[$0] }
      while assigned > totalWidth && !reducible.isEmpty {
        reducible.sort { lhs, rhs in
          if remainders[lhs] != remainders[rhs] {
            return remainders[lhs] < remainders[rhs]
          }
          return widths[lhs] > widths[rhs]
        }

        guard let index = reducible.first else {
          break
        }

        widths[index] -= 1
        assigned -= 1
        reducible = widths.indices.filter { widths[$0] > minimumWidths[$0] }
      }

      if assigned > totalWidth {
        var positive = widths.indices.filter { widths[$0] > 0 }
        while assigned > totalWidth && !positive.isEmpty {
          positive.sort { widths[$0] > widths[$1] }
          guard let index = positive.first else {
            break
          }
          widths[index] -= 1
          assigned -= 1
          positive = widths.indices.filter { widths[$0] > 0 }
        }
      }
    }

    if assigned < totalWidth {
      let order = widths.indices.sorted { lhs, rhs in
        if remainders[lhs] != remainders[rhs] {
          return remainders[lhs] > remainders[rhs]
        }
        return durations[lhs] > durations[rhs]
      }

      if order.isEmpty {
        return widths
      }

      var cursor = 0
      while assigned < totalWidth {
        widths[order[cursor % order.count]] += 1
        assigned += 1
        cursor += 1
      }
    }

    return widths
  }
}

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
    case .tool:
      return NSColor.systemIndigo.withAlphaComponent(0.95)
    case .edit:
      return NSColor.systemPurple.withAlphaComponent(0.95)
    case .waiting:
      return NSColor.systemRed.withAlphaComponent(0.95)
    case .network:
      return NSColor.systemBlue.withAlphaComponent(0.95)
    case .prefill:
      return NSColor.systemOrange.withAlphaComponent(0.95)
    case .reasoning:
      return NSColor.systemPink.withAlphaComponent(0.95)
    case .gen:
      return NSColor.systemGreen.withAlphaComponent(0.95)
    }
  case .idle:
    return NSColor.systemGray.withAlphaComponent(0.55)
  }
}

private func SegmentKindLabel(_ kind: TimelineSegmentKind) -> String {
  switch kind {
  case .category(let category):
    switch category {
    case .tool:
      return "Tool"
    case .edit:
      return "Edit"
    case .waiting:
      return "Waiting"
    case .network:
      return "Network"
    case .prefill:
      return "Prefill"
    case .reasoning:
      return "Reasoning"
    case .gen:
      return "Generation"
    }
  case .idle:
    return "Idle"
  }
}

private func FormatClockTime(_ date: Date) -> String {
  clockTimeFormatter.string(from: date)
}

private func FormatDuration(_ duration: TimeInterval) -> String {
  if duration <= 0 {
    return "0s"
  }
  return durationFormatter.string(from: duration) ?? "0s"
}
