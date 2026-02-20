import AppKit
import Foundation
import SwiftUI

struct TurnMenuRowView: View {
  let endpointRow: EndpointRow
  let now: Date
  let isExpanded: Bool
  let expandedRunKeys: Set<String>
  let onToggle: () -> Void
  let onToggleHistoryRun: (String) -> Void
  let onReconnectEndpoint: () -> Void

  @State private var hoveredTimelineSegment: TimelineSegment?

  var body: some View {
    VStack(alignment: .leading, spacing: 6) {
      Button(action: onToggle) {
        HStack(alignment: .center, spacing: 6) {
          Circle()
            .fill(StatusDotColor(activeTurn?.status ?? .completed))
            .frame(width: 8, height: 8)

          Text(NameText())
            .font(.system(size: 12, weight: .semibold))
            .lineLimit(1)

          Spacer(minLength: 8)

          Text(ElapsedText())
            .font(.system(size: 11, weight: .medium, design: .monospaced))
            .foregroundStyle(.secondary)

          Text(isExpanded ? "▾" : "▸")
            .font(.system(size: 10, weight: .medium))
            .foregroundStyle(.tertiary)
        }
      }
      .buttonStyle(.plain)

      if activeTurn != nil {
        Text(TimelineDetailText())
          .font(.system(size: 11))
          .foregroundStyle(.secondary)
          .lineLimit(1)

        TimelineBarView(
          segments: activeTurn?.TimelineSegments(now: now) ?? [],
          onHoveredSegmentChanged: { segment in
            hoveredTimelineSegment = segment
          }
        )
        .frame(height: 8)
      } else {
        if isExpanded, let cwd = endpointRow.cwd {
          Text("Workspace: \(cwd.replacingOccurrences(of: NSHomeDirectory(), with: "~"))")
            .font(.system(size: 10))
            .foregroundStyle(.tertiary)
            .lineLimit(1)
            .truncationMode(.middle)
        }

        Text(endpointRow.lastTraceLabel ?? "No active run")
          .font(.system(size: 11))
          .foregroundStyle(.secondary)
          .lineLimit(1)
      }

      if isExpanded {
        ExpandedBody
          .transition(.opacity.combined(with: .move(edge: .top)))
      }
    }
    .padding(.horizontal, 10)
    .padding(.vertical, 8)
    .background(
      RoundedRectangle(cornerRadius: 8, style: .continuous)
        .fill(Color(nsColor: NSColor.controlBackgroundColor).opacity(0.45))
    )
    .overlay(
      RoundedRectangle(cornerRadius: 8, style: .continuous)
        .stroke(Color(nsColor: NSColor.separatorColor).opacity(0.2), lineWidth: 0.5)
    )
    .animation(.spring(response: 0.3, dampingFraction: 0.85), value: isExpanded)
  }

  @ViewBuilder
  private var ExpandedBody: some View {
    VStack(alignment: .leading, spacing: 10) {
      if let prompt = PromptLabelText() {
        Text("Prompt: \(prompt)")
          .font(.system(size: 10))
          .foregroundStyle(.secondary)
          .lineLimit(2)
      }

      if HasGitOrModelInfo() {
        Text(GitModelLine())
          .font(.system(size: 10, weight: .medium))
          .foregroundStyle(.secondary)
          .lineLimit(1)
      }

      if let usage = EffectiveTokenUsage() {
        SectionCard(title: TokenTitle(usage: usage)) {
          VStack(alignment: .leading, spacing: 4) {
            TokenUsageBarView(usage: usage)
              .frame(height: 12)

            Text(TokenDetail(usage: usage))
              .font(.system(size: 10, design: .monospaced))
              .foregroundStyle(.tertiary)
              .lineLimit(1)
          }
        }
      }

      if let latestError = endpointRow.latestError {
        VStack(alignment: .leading, spacing: 4) {
          Text(latestError.willRetry ? "\(latestError.message) (retrying...)" : latestError.message)
            .font(.system(size: 10, weight: .medium))
            .foregroundStyle(.red)
            .lineLimit(2)
        }
        .padding(8)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(
          RoundedRectangle(cornerRadius: 6, style: .continuous)
            .fill(Color.red.opacity(0.08))
        )
        .overlay(
          RoundedRectangle(cornerRadius: 6, style: .continuous)
            .stroke(Color.red.opacity(0.25), lineWidth: 0.5)
        )
      }

      if !endpointRow.planSteps.isEmpty {
        VStack(alignment: .leading, spacing: 2) {
          Text(PlanTitle())
            .font(.system(size: 10, weight: .semibold))
            .foregroundStyle(.secondary)

          ForEach(Array(endpointRow.planSteps.prefix(6).enumerated()), id: \.offset) { _, step in
            Text("\(PlanIcon(step.status))  \(Truncate(step.description, limit: 52))")
              .font(.system(size: 10))
              .foregroundStyle(.secondary)
              .lineLimit(1)
          }
        }
      }

      if !endpointRow.fileChanges.isEmpty {
        VStack(alignment: .leading, spacing: 2) {
          Text("Files (\(endpointRow.fileChanges.count))")
            .font(.system(size: 10, weight: .semibold))
            .foregroundStyle(.secondary)

          ForEach(Array(endpointRow.fileChanges.prefix(8).enumerated()), id: \.offset) { _, change in
            let filename = (change.path as NSString).lastPathComponent
            let dir = (change.path as NSString).deletingLastPathComponent
            let shortDir = dir.isEmpty ? "" : "\(dir)/"
            Text("\(change.kind.label)  \(shortDir)\(filename)")
              .font(.system(size: 10))
              .foregroundStyle(.secondary)
              .lineLimit(1)
          }
        }
      }

      if !endpointRow.commands.isEmpty {
        SectionCard(title: "Commands / Tools Run (\(endpointRow.commands.count))") {
          VStack(alignment: .leading, spacing: 2) {
            ForEach(Array(endpointRow.commands.suffix(5).enumerated()), id: \.offset) { _, command in
              Text(CommandLine(command: command))
                .font(.system(size: 10))
                .foregroundStyle(.secondary)
                .lineLimit(1)
            }
          }
        }
      }

      if !endpointRow.recentRuns.isEmpty {
        SectionCard(title: "Past Runs (\(endpointRow.recentRuns.count))") {
          VStack(spacing: 4) {
            ForEach(endpointRow.recentRuns, id: \.runKey) { run in
              RunHistoryRowView(
                run: run,
                isLastRun: run.turnId == endpointRow.recentRuns.first?.turnId,
                isExpanded: expandedRunKeys.contains(run.runKey),
                onToggle: { onToggleHistoryRun(run.runKey) }
              )
            }
          }
        }
      }

      Divider()

      HStack(spacing: 8) {
        if let cwd = endpointRow.cwd {
          Button("Open in Finder") {
            NSWorkspace.shared.open(URL(fileURLWithPath: cwd))
          }
        }

        Button("Reconnect") {
          onReconnectEndpoint()
        }
      }
      .buttonStyle(.bordered)
      .controlSize(.small)
    }
    .padding(.top, 2)
  }

  private var activeTurn: ActiveTurn? {
    endpointRow.activeTurn
  }

  private func NameText() -> String {
    let hasCwd = endpointRow.cwd != nil
    let hasTitle = endpointRow.chatTitle != nil && !(endpointRow.chatTitle?.isEmpty ?? true)
    if hasCwd || hasTitle {
      return "\(endpointRow.displayName) (\(endpointRow.shortId))"
    }
    return endpointRow.displayName
  }

  private func ElapsedText() -> String {
    guard let activeTurn else {
      return "Idle"
    }
    return "\(StatusLabel(activeTurn.status)) \(activeTurn.ElapsedString(now: now))"
  }

  private func TimelineDetailText() -> String {
    if let hoveredTimelineSegment {
      return HoverText(segment: hoveredTimelineSegment)
    }

    var summaryParts: [String] = []
    if let traceLabel = endpointRow.lastTraceLabel ?? activeTurn?.latestLabel {
      summaryParts.append(traceLabel)
    }

    if endpointRow.fileChanges.count > 0 {
      let fileCount = endpointRow.fileChanges.count
      summaryParts.append("\(fileCount) file\(fileCount == 1 ? "" : "s")")
    }

    if endpointRow.commands.count > 0 {
      let commandCount = endpointRow.commands.count
      summaryParts.append("\(commandCount) cmd\(commandCount == 1 ? "" : "s")")
    }

    return summaryParts.isEmpty ? "Working..." : summaryParts.joined(separator: " · ")
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

  private func PromptLabelText() -> String? {
    guard endpointRow.activeTurn != nil else { return nil }
    if let promptPreview = endpointRow.promptPreview, !promptPreview.isEmpty {
      return Truncate(promptPreview, limit: 130)
    }
    return "waiting for first user message"
  }

  private func EffectiveTokenUsage() -> TokenUsageInfo? {
    guard endpointRow.activeTurn != nil else { return nil }
    if let usage = endpointRow.tokenUsage, usage.totalTokens > 0 {
      return usage
    }
    return TokenUsageInfo()
  }

  private func HasGitOrModelInfo() -> Bool {
    endpointRow.activeTurn != nil && (endpointRow.gitInfo?.branch != nil || ModelSummary() != nil)
  }

  private func GitModelLine() -> String {
    var values: [String] = []

    if let branch = endpointRow.gitInfo?.branch {
      var value = branch
      if let sha = endpointRow.gitInfo?.sha {
        value += " · \(String(sha.prefix(7)))"
      }
      values.append(value)
    }

    if let modelSummary = ModelSummary() {
      values.append(modelSummary)
    }

    return values.joined(separator: "   ")
  }

  private func ModelSummary() -> String? {
    guard endpointRow.activeTurn != nil else { return nil }
    let model = endpointRow.model?.trimmingCharacters(in: .whitespacesAndNewlines)
    if let model, !model.isEmpty {
      return "Model: \(model)"
    }
    return nil
  }

  private func TokenTitle(usage: TokenUsageInfo) -> String {
    if let contextWindow = usage.contextWindow {
      return "Token Usage - \(FormatTokenCount(usage.totalTokens)) / \(FormatTokenCount(contextWindow))"
    }
    return "Token Usage - \(FormatTokenCount(usage.totalTokens))"
  }

  private func TokenDetail(usage: TokenUsageInfo) -> String {
    var values = ["In: \(FormatTokenCount(usage.inputTokens))"]
    if usage.cachedInputTokens > 0 {
      values[0] += " (\(FormatTokenCount(usage.cachedInputTokens)) cached)"
    }
    values.append("Out: \(FormatTokenCount(usage.outputTokens))")
    if usage.reasoningTokens > 0 {
      values.append("Reasoning: \(FormatTokenCount(usage.reasoningTokens))")
    }
    return values.joined(separator: " · ")
  }

  private func PlanTitle() -> String {
    let completed = endpointRow.planSteps.filter { $0.status == .completed }.count
    return "Plan (\(completed)/\(endpointRow.planSteps.count))"
  }

  private func PlanIcon(_ status: PlanStepStatus) -> String {
    switch status {
    case .completed: return "✓"
    case .inProgress: return "●"
    case .pending: return "○"
    }
  }

  private func CommandLine(command: CommandSummary) -> String {
    var metadata: [String] = []

    if let exitCode = command.exitCode {
      metadata.append("exit \(exitCode)")
    }

    if let ms = command.durationMs {
      metadata.append(String(format: "%.1fs", Double(ms) / 1000.0))
    }

    let suffix = metadata.isEmpty ? "" : "  \(metadata.joined(separator: "  "))"
    return "• \(Truncate(command.command, limit: 38))\(suffix)"
  }

  private func StatusLabel(_ status: TurnExecutionStatus) -> String {
    switch status {
    case .inProgress: return "Working"
    case .completed: return "Done"
    case .interrupted: return "Interrupted"
    case .failed: return "Failed"
    }
  }

  private func StatusDotColor(_ status: TurnExecutionStatus) -> Color {
    switch status {
    case .inProgress: return .green
    case .completed: return Color(nsColor: .systemGray)
    case .interrupted: return .orange
    case .failed: return .red
    }
  }

  private func Truncate(_ value: String, limit: Int) -> String {
    if value.count <= limit { return value }
    return "\(value.prefix(max(0, limit - 1)))…"
  }
}

private struct RunHistoryRowView: View {
  let run: CompletedRun
  let isLastRun: Bool
  let isExpanded: Bool
  let onToggle: () -> Void

  var body: some View {
    VStack(alignment: .leading, spacing: 5) {
      Button(action: onToggle) {
        HStack(spacing: 6) {
          Circle()
            .fill(StatusColor(run.status))
            .frame(width: 6, height: 6)

          Text(TitleText())
            .font(.system(size: 10, design: .monospaced))
            .foregroundStyle(.secondary)
            .lineLimit(1)

          Spacer(minLength: 4)

          Text(isExpanded ? "▾" : "▸")
            .font(.system(size: 9, weight: .medium))
            .foregroundStyle(.tertiary)
        }
      }
      .buttonStyle(.plain)

      if isExpanded {
        VStack(alignment: .leading, spacing: 4) {
          Text("Prompt: \(run.promptPreview ?? "Prompt unavailable")")
            .font(.system(size: 10))
            .foregroundStyle(.secondary)
            .lineLimit(2)

          if let model = run.model, !model.isEmpty {
            Text("Model: \(model)")
              .font(.system(size: 10, weight: .medium))
              .foregroundStyle(.tertiary)
              .lineLimit(1)
          }

          TimelineBarView(segments: run.TimelineSegments(), onHoveredSegmentChanged: { _ in })
            .frame(height: 8)

          if let usage = run.tokenUsage, usage.totalTokens > 0 {
            TokenUsageBarView(usage: usage)
              .frame(height: 10)
          }
        }
      }
    }
    .padding(.horizontal, 6)
    .padding(.vertical, 4)
    .background(
      RoundedRectangle(cornerRadius: 4, style: .continuous)
        .fill(Color(nsColor: NSColor.controlBackgroundColor).opacity(0.35))
    )
  }

  private func TitleText() -> String {
    let suffix = isLastRun ? " · latest" : ""
    return "\(StatusText(run.status)) · \(run.ElapsedString()) · \(run.RanAtString())\(suffix)"
  }

  private func StatusText(_ status: TurnExecutionStatus) -> String {
    switch status {
    case .inProgress: return "Working"
    case .completed: return "Completed"
    case .interrupted: return "Interrupted"
    case .failed: return "Failed"
    }
  }

  private func StatusColor(_ status: TurnExecutionStatus) -> Color {
    switch status {
    case .inProgress: return .green
    case .completed: return Color(nsColor: .systemGray)
    case .interrupted: return .orange
    case .failed: return .red
    }
  }
}

private struct SectionCard<Content: View>: View {
  let title: String
  @ViewBuilder let content: Content

  var body: some View {
    VStack(alignment: .leading, spacing: 6) {
      Text(title)
        .font(.system(size: 10, weight: .semibold))
        .foregroundStyle(.secondary)

      content
    }
    .padding(6)
    .frame(maxWidth: .infinity, alignment: .leading)
    .background(
      RoundedRectangle(cornerRadius: 6, style: .continuous)
        .fill(Color(nsColor: NSColor.controlBackgroundColor).opacity(0.5))
    )
    .overlay(
      RoundedRectangle(cornerRadius: 6, style: .continuous)
        .stroke(Color(nsColor: NSColor.separatorColor).opacity(0.3), lineWidth: 0.5)
    )
  }
}

private struct TimelineBarView: View {
  let segments: [TimelineSegment]
  let onHoveredSegmentChanged: (TimelineSegment?) -> Void

  @State private var hoveredIndex: Int?

  var body: some View {
    GeometryReader { geometry in
      let filtered = segments.filter { $0.duration > 0 }
      let totalDuration = filtered.reduce(0.0) { $0 + $1.duration }
      let segmentCount = max(1, filtered.count)

      ZStack(alignment: .leading) {
        RoundedRectangle(cornerRadius: 4, style: .continuous)
          .fill(Color(nsColor: NSColor.controlBackgroundColor).opacity(0.8))

        HStack(spacing: 0) {
          ForEach(Array(filtered.enumerated()), id: \.offset) { index, segment in
            let width = SegmentWidth(
              availableWidth: geometry.size.width,
              segmentDuration: segment.duration,
              totalDuration: totalDuration,
              segmentCount: segmentCount
            )

            Rectangle()
              .fill(SegmentFillColor(segment.kind))
              .frame(width: width)
              .overlay(alignment: .trailing) {
                if index < filtered.count - 1 {
                  Rectangle()
                    .fill(Color(nsColor: NSColor.separatorColor).opacity(0.4))
                    .frame(width: 0.5)
                }
              }
              .onHover { hovering in
                hoveredIndex = hovering ? index : nil
                onHoveredSegmentChanged(hovering ? segment : nil)
              }
          }
        }
        .clipShape(RoundedRectangle(cornerRadius: 4, style: .continuous))
      }
      .overlay(
        RoundedRectangle(cornerRadius: 4, style: .continuous)
          .stroke(Color(nsColor: NSColor.separatorColor).opacity(0.5), lineWidth: 0.5)
      )
      .overlay(alignment: .leading) {
        if let hoveredIndex,
          hoveredIndex < filtered.count
        {
          let xOffset = HoverOffset(
            availableWidth: geometry.size.width,
            segments: filtered,
            index: hoveredIndex,
            totalDuration: totalDuration,
            segmentCount: segmentCount
          )
          let width = SegmentWidth(
            availableWidth: geometry.size.width,
            segmentDuration: filtered[hoveredIndex].duration,
            totalDuration: totalDuration,
            segmentCount: segmentCount
          )

          RoundedRectangle(cornerRadius: 2, style: .continuous)
            .stroke(Color.primary.opacity(0.4), lineWidth: 1)
            .frame(width: max(0, width - 1), height: max(0, geometry.size.height - 1))
            .offset(x: xOffset + 0.5, y: 0)
        }
      }
    }
  }

  private func SegmentWidth(
    availableWidth: CGFloat,
    segmentDuration: TimeInterval,
    totalDuration: TimeInterval,
    segmentCount: Int
  ) -> CGFloat {
    guard availableWidth > 0 else { return 0 }

    if totalDuration <= 0 {
      return availableWidth / CGFloat(segmentCount)
    }

    return availableWidth * CGFloat(segmentDuration / totalDuration)
  }

  private func HoverOffset(
    availableWidth: CGFloat,
    segments: [TimelineSegment],
    index: Int,
    totalDuration: TimeInterval,
    segmentCount: Int
  ) -> CGFloat {
    guard index > 0 else { return 0 }

    return segments.prefix(index).reduce(0) { total, segment in
      total
        + SegmentWidth(
          availableWidth: availableWidth,
          segmentDuration: segment.duration,
          totalDuration: totalDuration,
          segmentCount: segmentCount)
    }
  }
}

private struct TokenUsageBarView: View {
  let usage: TokenUsageInfo

  var body: some View {
    GeometryReader { geometry in
      let segments = BuildUsageSegments(usage)
      let total = segments.reduce(0.0) { $0 + $1.count }
      let maxFraction = usage.contextWindow.map { contextWindow in
        contextWindow > 0
          ? CGFloat(min(1.0, Double(usage.totalTokens) / Double(contextWindow)))
          : CGFloat(1.0)
      } ?? 1.0
      let availableWidth = geometry.size.width * maxFraction

      ZStack(alignment: .leading) {
        RoundedRectangle(cornerRadius: 4, style: .continuous)
          .fill(Color(nsColor: NSColor.controlBackgroundColor).opacity(0.8))

        HStack(spacing: 0) {
          ForEach(Array(segments.enumerated()), id: \.offset) { _, segment in
            Rectangle()
              .fill(segment.color)
              .frame(
                width: total > 0
                  ? availableWidth * CGFloat(segment.count / total)
                  : 0)
          }
        }
        .clipShape(RoundedRectangle(cornerRadius: 4, style: .continuous))
      }
      .overlay(
        RoundedRectangle(cornerRadius: 4, style: .continuous)
          .stroke(Color(nsColor: NSColor.separatorColor).opacity(0.5), lineWidth: 0.5)
      )
    }
  }

  private func BuildUsageSegments(_ usage: TokenUsageInfo) -> [(label: String, count: Double, color: Color)] {
    var segments: [(String, Double, Color)] = []

    let cached = usage.cachedInputTokens
    let freshInput = max(0, usage.inputTokens - cached)

    if cached > 0 {
      segments.append(("Cached Input", Double(cached), Color(nsColor: .systemGray).opacity(0.5)))
    }

    if freshInput > 0 {
      segments.append(("Input", Double(freshInput), Color.accentColor.opacity(0.45)))
    }

    if usage.reasoningTokens > 0 {
      segments.append(("Reasoning", Double(usage.reasoningTokens), Color(nsColor: .systemPink).opacity(0.55)))
    }

    let regularOutput = max(0, usage.outputTokens - usage.reasoningTokens)
    if regularOutput > 0 {
      segments.append(("Output", Double(regularOutput), Color(nsColor: .systemGreen).opacity(0.55)))
    }

    return segments
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

private func SegmentFillColor(_ kind: TimelineSegmentKind) -> Color {
  switch kind {
  case .category(let category):
    switch category {
    case .tool: return Color(nsColor: .systemIndigo).opacity(0.85)
    case .edit: return Color(nsColor: .systemPurple).opacity(0.85)
    case .waiting: return Color(nsColor: .systemRed).opacity(0.85)
    case .network: return Color(nsColor: .systemBlue).opacity(0.85)
    case .prefill: return Color(nsColor: .systemOrange).opacity(0.85)
    case .reasoning: return Color(nsColor: .systemPink).opacity(0.85)
    case .gen: return Color(nsColor: .systemGreen).opacity(0.85)
    }
  case .idle:
    return Color(nsColor: .systemGray).opacity(0.3)
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
  if duration <= 0 {
    return "0s"
  }
  return durationFormatter.string(from: duration) ?? "0s"
}
