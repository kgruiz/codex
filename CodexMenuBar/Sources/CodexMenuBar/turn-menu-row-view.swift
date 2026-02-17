import AppKit
import Foundation

final class TurnMenuRowView: NSView {
  private static let rowWidth: CGFloat = 420
  private static let rowHeight: CGFloat = 64

  private let topLabel = NSTextField(labelWithString: "")
  private let detailLabel = NSTextField(labelWithString: "")
  private let barView = TimelineBarView()
  private let hoverCard = NSView()
  private let hoverColorSwatch = NSView()
  private let hoverLabel = NSTextField(labelWithString: "")
  private var defaultDetailText = "No detail"

  init(turn: ActiveTurn, now: Date) {
    super.init(frame: NSRect(x: 0, y: 0, width: Self.rowWidth, height: Self.rowHeight))
    ConfigureViews()
    Update(turn: turn, now: now)
  }

  @available(*, unavailable)
  required init?(coder: NSCoder) {
    fatalError("init(coder:) has not been implemented")
  }

  override var intrinsicContentSize: NSSize {
    NSSize(width: Self.rowWidth, height: Self.rowHeight)
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

    topLabel.frame = NSRect(
      x: contentRect.minX,
      y: contentRect.maxY - topHeight,
      width: contentRect.width,
      height: topHeight
    )

    barView.frame = NSRect(
      x: contentRect.minX,
      y: topLabel.frame.minY - verticalSpacing - barHeight,
      width: contentRect.width,
      height: barHeight
    )

    detailLabel.frame = NSRect(
      x: contentRect.minX,
      y: contentRect.minY,
      width: contentRect.width,
      height: detailHeight
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
  }

  private func ConfigureViews() {
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

    barView.OnHoveredSegmentChanged = { [weak self] segment in
      self?.UpdateHoverText(segment: segment)
    }

    addSubview(topLabel)
    addSubview(barView)
    addSubview(detailLabel)
    addSubview(hoverCard)
    hoverCard.addSubview(hoverColorSwatch)
    hoverCard.addSubview(hoverLabel)
  }

  private func Update(turn: ActiveTurn, now: Date) {
    let shortThreadId = String(turn.threadId.prefix(8))
    topLabel.stringValue =
      "\(StatusLabel(turn.status)) · \(turn.ElapsedString(now: now)) · [\(shortThreadId)/\(turn.turnId)]"

    defaultDetailText = turn.latestLabel ?? "No detail"
    ShowDefaultDetail()
    barView.Configure(segments: turn.TimelineSegments(now: now))
  }

  private func UpdateHoverText(segment: TimelineSegment?) {
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
