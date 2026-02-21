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
  let isFilesExpanded: Bool
  let isCommandsExpanded: Bool
  let isPastRunsExpanded: Bool
  let onToggleFiles: () -> Void
  let onToggleCommands: () -> Void
  let onTogglePastRuns: () -> Void
  let onOpenInTerminal: (String) -> Void

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
        .frame(maxWidth: .infinity, alignment: .leading)
        .contentShape(Rectangle())
      }
      .buttonStyle(.plain)
      .focusable(false)

      if activeTurn != nil {
        Text(TimelineSummaryText())
          .font(.system(size: 11))
          .foregroundStyle(.secondary)
          .lineLimit(1)

        TimelineBarView(segments: activeTurn?.TimelineSegments(now: now) ?? [])
          .frame(height: 8)
      } else {
        if isExpanded, let cwd = endpointRow.cwd {
          Text(cwd.replacingOccurrences(of: NSHomeDirectory(), with: "~"))
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
    .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 8, style: .continuous))
    .overlay(
      RoundedRectangle(cornerRadius: 8, style: .continuous)
        .stroke(Color(nsColor: NSColor.separatorColor).opacity(0.2), lineWidth: 0.5)
    )
    .overlay {
      if !isExpanded {
        Rectangle()
          .fill(.clear)
          .contentShape(Rectangle())
          .onTapGesture {
            onToggle()
          }
      }
    }
    .animation(.spring(response: 0.3, dampingFraction: 0.85), value: isExpanded)
  }

  @ViewBuilder
  private var ExpandedBody: some View {
    VStack(alignment: .leading, spacing: 10) {
      if let prompt = PromptDisplayText() {
        SectionCard {
          HStack(spacing: 6) {
            Label("Prompt", systemImage: "text.bubble.fill")
              .font(.system(size: 10, weight: .semibold))
              .foregroundStyle(.secondary)

            Spacer(minLength: 4)

            Button(action: { CopyToClipboard(prompt) }) {
              Image(systemName: "doc.on.doc")
                .font(.system(size: 10, weight: .semibold))
            }
            .buttonStyle(.plain)
            .help("Copy prompt")
          }
        } content: {
          Text(prompt)
            .font(.system(size: 10))
            .foregroundStyle(.secondary)
            .lineLimit(2)
        }
      }

      if HasGitOrModelInfo() {
        Label(GitModelLine(), systemImage: "folder.fill")
          .font(.system(size: 10, weight: .medium))
          .foregroundStyle(.secondary)
          .lineLimit(1)
      }

      if let usage = EffectiveTokenUsage() {
        SectionCard {
          Label(TokenTitle(usage: usage), systemImage: "chart.bar.fill")
            .font(.system(size: 10, weight: .semibold))
            .foregroundStyle(.secondary)
        } content: {
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
        SectionCard {
          HStack(spacing: 6) {
            Label("Error", systemImage: "exclamationmark.circle.fill")
              .font(.system(size: 10, weight: .semibold))
              .foregroundStyle(.red)

            Spacer(minLength: 4)

            Button(action: { CopyToClipboard(ErrorCopyText(latestError)) }) {
              Image(systemName: "doc.on.doc")
                .font(.system(size: 10, weight: .semibold))
            }
            .buttonStyle(.plain)
            .help("Copy error")
          }
        } content: {
          VStack(alignment: .leading, spacing: 2) {
            Text(
              latestError.willRetry ? "\(latestError.message) (retrying...)" : latestError.message
            )
            .font(.system(size: 10, weight: .medium))
            .foregroundStyle(.red)
            .lineLimit(2)

            if let details = latestError.details, !details.isEmpty {
              Text(details)
                .font(.system(size: 10))
                .foregroundStyle(.secondary)
                .lineLimit(3)
            }
          }
        }
      }

      if !endpointRow.planSteps.isEmpty {
        SectionCard {
          Label(PlanTitle(), systemImage: "checklist")
            .font(.system(size: 10, weight: .semibold))
            .foregroundStyle(.secondary)
        } content: {
          VStack(alignment: .leading, spacing: 2) {
            ForEach(Array(endpointRow.planSteps.prefix(6).enumerated()), id: \.offset) { _, step in
              Text("\(PlanIcon(step.status))  \(Truncate(step.description, limit: 52))")
                .font(.system(size: 10))
                .foregroundStyle(.secondary)
                .lineLimit(1)
            }
          }
        }
      }

      if !VisibleFileChanges().isEmpty {
        AccordionSectionCard(
          title: "Files (\(VisibleFileChanges().count))",
          systemImage: "doc.text.fill",
          isExpanded: isFilesExpanded,
          onToggle: onToggleFiles
        ) {
          VStack(alignment: .leading, spacing: 2) {
            ForEach(Array(VisibleFileChanges().prefix(8).enumerated()), id: \.offset) { _, change in
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
      }

      if !VisibleCommands().isEmpty {
        AccordionSectionCard(
          title: "Commands / Tools Run (\(VisibleCommands().count))",
          systemImage: "terminal.fill",
          isExpanded: isCommandsExpanded,
          onToggle: onToggleCommands
        ) {
          VStack(alignment: .leading, spacing: 2) {
            ForEach(Array(VisibleCommands().suffix(5).enumerated()), id: \.offset) { _, command in
              Text(CommandLine(command: command))
                .font(.system(size: 10))
                .foregroundStyle(.secondary)
                .lineLimit(1)
            }
          }
        }
      }

      if !endpointRow.recentRuns.isEmpty {
        AccordionSectionCard(
          title: "Past Runs (\(endpointRow.recentRuns.count))",
          systemImage: "clock.arrow.circlepath",
          isExpanded: isPastRunsExpanded,
          onToggle: onTogglePastRuns
        ) {
          VStack(spacing: 4) {
            ForEach(endpointRow.recentRuns, id: \.runKey) { run in
              RunHistoryRowView(
                run: run,
                fallbackModel: endpointRow.model,
                fallbackModelProvider: endpointRow.modelProvider,
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

          Button("Open in Terminal") {
            onOpenInTerminal(cwd)
          }
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

  private func TimelineSummaryText() -> String {
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

  private func PromptDisplayText() -> String? {
    guard endpointRow.activeTurn != nil else { return nil }
    if let promptPreview = endpointRow.promptPreview, !promptPreview.isEmpty {
      return promptPreview
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
      return
        "Token Usage - \(FormatTokenCount(usage.totalTokens)) / \(FormatTokenCount(contextWindow))"
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

  private func VisibleFileChanges() -> [FileChangeSummary] {
    guard endpointRow.activeTurn != nil else {
      return []
    }

    if !endpointRow.fileChanges.isEmpty {
      return endpointRow.fileChanges
    }

    return []
  }

  private func VisibleCommands() -> [CommandSummary] {
    guard endpointRow.activeTurn != nil else {
      return []
    }

    if !endpointRow.commands.isEmpty {
      return endpointRow.commands
    }

    return []
  }

  private func ErrorCopyText(_ error: ErrorInfo) -> String {
    var lines = [error.message]

    if let details = error.details, !details.isEmpty {
      lines.append(details)
    }

    if error.willRetry {
      lines.append("retrying...")
    }

    return lines.joined(separator: "\n")
  }

  private func CopyToClipboard(_ value: String) {
    let pasteboard = NSPasteboard.general
    pasteboard.clearContents()
    pasteboard.setString(value, forType: .string)
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
  let fallbackModel: String?
  let fallbackModelProvider: String?
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
        .frame(maxWidth: .infinity, alignment: .leading)
        .contentShape(Rectangle())
      }
      .buttonStyle(.plain)
      .focusable(false)

      if isExpanded {
        VStack(alignment: .leading, spacing: 4) {
          Text("Prompt: \(run.promptPreview ?? "Prompt unavailable")")
            .font(.system(size: 10))
            .foregroundStyle(.secondary)
            .lineLimit(2)

          if let modelLine = ModelLine() {
            Text(modelLine)
              .font(.system(size: 10, weight: .medium))
              .foregroundStyle(.tertiary)
              .lineLimit(1)
          }

          TimelineBarView(segments: run.TimelineSegments())
            .frame(height: 8)

          if !run.fileChanges.isEmpty {
            VStack(alignment: .leading, spacing: 2) {
              Text("Files touched:")
                .font(.system(size: 9, weight: .semibold))
                .foregroundStyle(.tertiary)

              ForEach(Array(run.fileChanges.prefix(5).enumerated()), id: \.offset) { _, change in
                Text("\(change.kind.label)  \((change.path as NSString).lastPathComponent)")
                  .font(.system(size: 10))
                  .foregroundStyle(.secondary)
                  .lineLimit(1)
              }
            }
          }

          if !run.commands.isEmpty {
            VStack(alignment: .leading, spacing: 2) {
              Text("Commands:")
                .font(.system(size: 9, weight: .semibold))
                .foregroundStyle(.tertiary)

              ForEach(Array(run.commands.prefix(5).enumerated()), id: \.offset) { _, cmd in
                Text("• \(cmd.command)")
                  .font(.system(size: 10))
                  .foregroundStyle(.secondary)
                  .lineLimit(1)
              }
            }
          }

          if let usage = run.tokenUsage, usage.totalTokens > 0 {
            TokenUsageBarView(usage: usage)
              .frame(height: 10)
          }
        }
      }
    }
    .padding(.horizontal, 6)
    .padding(.vertical, 4)
    .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 4, style: .continuous))
    .overlay {
      if !isExpanded {
        Rectangle()
          .fill(.clear)
          .contentShape(Rectangle())
          .onTapGesture {
            onToggle()
          }
      }
    }
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

  private func ModelLine() -> String? {
    let model = NonEmpty(run.model) ?? NonEmpty(fallbackModel)
    let provider = NonEmpty(run.modelProvider) ?? NonEmpty(fallbackModelProvider)

    if let model, let provider {
      return "Model: \(model) (\(provider))"
    }

    if let model {
      return "Model: \(model)"
    }

    if let provider {
      return "Model Provider: \(provider)"
    }

    return nil
  }

  private func NonEmpty(_ value: String?) -> String? {
    guard let value else {
      return nil
    }

    let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
    return trimmed.isEmpty ? nil : trimmed
  }
}

private struct SectionCard<Header: View, Content: View>: View {
  @ViewBuilder let header: Header
  @ViewBuilder let content: Content

  init(@ViewBuilder header: () -> Header, @ViewBuilder content: () -> Content) {
    self.header = header()
    self.content = content()
  }

  var body: some View {
    VStack(alignment: .leading, spacing: 6) {
      header
      content
    }
    .padding(6)
    .frame(maxWidth: .infinity, alignment: .leading)
    .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 6, style: .continuous))
    .overlay(
      RoundedRectangle(cornerRadius: 6, style: .continuous)
        .stroke(Color(nsColor: NSColor.separatorColor).opacity(0.3), lineWidth: 0.5)
    )
  }
}

private struct AccordionSectionCard<Content: View>: View {
  let title: String
  let systemImage: String
  let isExpanded: Bool
  let onToggle: () -> Void
  @ViewBuilder let content: Content

  var body: some View {
    SectionCard {
      Button(action: onToggle) {
        HStack(spacing: 6) {
          Label(title, systemImage: systemImage)
            .font(.system(size: 10, weight: .semibold))
            .foregroundStyle(.secondary)

          Spacer(minLength: 4)

          Text(isExpanded ? "▾" : "▸")
            .font(.system(size: 10, weight: .medium))
            .foregroundStyle(.tertiary)
        }
      }
      .buttonStyle(.plain)
    } content: {
      if isExpanded {
        content
      }
    }
  }
}

private struct TimelineBarView: View {
  let segments: [TimelineSegment]

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
      .overlay(alignment: .topLeading) {
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
          let tooltipMaxWidth = max(120, min(geometry.size.width - 8, 260))
          let tooltipX = max(
            0,
            min(
              xOffset + (width / 2) - (tooltipMaxWidth / 2), geometry.size.width - tooltipMaxWidth))

          Text(SegmentTooltipText(segment: filtered[hoveredIndex]))
            .font(.system(size: 9, weight: .medium))
            .lineLimit(1)
            .truncationMode(.tail)
            .padding(.horizontal, 6)
            .padding(.vertical, 3)
            .frame(width: tooltipMaxWidth, alignment: .leading)
            .background(
              Color(nsColor: .windowBackgroundColor), in: RoundedRectangle(cornerRadius: 4)
            )
            .overlay(
              RoundedRectangle(cornerRadius: 4)
                .stroke(Color.gray.opacity(0.25), lineWidth: 0.5)
            )
            .shadow(color: .black.opacity(0.08), radius: 1.5, y: 1)
            .offset(x: tooltipX, y: -18)
            .allowsHitTesting(false)
            .zIndex(2)
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

  @State private var hoveredIndex: Int?

  var body: some View {
    GeometryReader { geometry in
      let segments = BuildUsageSegments(usage)
      let total = segments.reduce(0.0) { $0 + $1.count }
      let maxFraction =
        usage.contextWindow.map { contextWindow in
          contextWindow > 0
            ? CGFloat(min(1.0, Double(usage.totalTokens) / Double(contextWindow)))
            : CGFloat(1.0)
        } ?? 1.0
      let availableWidth = geometry.size.width * maxFraction

      ZStack(alignment: .leading) {
        RoundedRectangle(cornerRadius: 4, style: .continuous)
          .fill(Color(nsColor: NSColor.controlBackgroundColor).opacity(0.8))

        HStack(spacing: 0) {
          ForEach(Array(segments.enumerated()), id: \.offset) { index, segment in
            let width =
              total > 0
              ? availableWidth * CGFloat(segment.count / total)
              : 0

            Rectangle()
              .fill(segment.color)
              .frame(width: width)
              .onHover { hovering in
                hoveredIndex = hovering ? index : nil
              }
          }
        }
        .clipShape(RoundedRectangle(cornerRadius: 4, style: .continuous))
      }
      .overlay(
        RoundedRectangle(cornerRadius: 4, style: .continuous)
          .stroke(Color(nsColor: NSColor.separatorColor).opacity(0.5), lineWidth: 0.5)
      )
      .overlay(alignment: .topLeading) {
        if let hoveredIndex,
          hoveredIndex < segments.count
        {
          let xOffset = TokenHoverOffset(
            availableWidth: availableWidth,
            segments: segments,
            index: hoveredIndex,
            total: total
          )
          let width = TokenSegmentWidth(
            availableWidth: availableWidth,
            segmentCount: segments[hoveredIndex].count,
            total: total
          )
          let tooltipMaxWidth = max(100, min(geometry.size.width - 8, 220))
          let tooltipX = max(
            0,
            min(
              xOffset + (width / 2) - (tooltipMaxWidth / 2), geometry.size.width - tooltipMaxWidth))

          Text(TokenSegmentTooltip(segment: segments[hoveredIndex], total: total))
            .font(.system(size: 9, weight: .medium))
            .lineLimit(1)
            .truncationMode(.tail)
            .padding(.horizontal, 6)
            .padding(.vertical, 3)
            .frame(width: tooltipMaxWidth, alignment: .leading)
            .background(
              Color(nsColor: .windowBackgroundColor), in: RoundedRectangle(cornerRadius: 4)
            )
            .overlay(
              RoundedRectangle(cornerRadius: 4)
                .stroke(Color.gray.opacity(0.25), lineWidth: 0.5)
            )
            .shadow(color: .black.opacity(0.08), radius: 1.5, y: 1)
            .offset(x: tooltipX, y: -18)
            .allowsHitTesting(false)
            .zIndex(2)
        }
      }
    }
  }

  private func TokenSegmentTooltip(
    segment: (label: String, count: Double, color: Color),
    total: Double
  ) -> String {
    let countText = FormatTokenCount(Int(segment.count))
    guard total > 0 else {
      return "\(segment.label): \(countText)"
    }
    let fraction = Int((segment.count / total * 100).rounded())
    return "\(segment.label): \(countText) (\(fraction)%)"
  }

  private func BuildUsageSegments(_ usage: TokenUsageInfo) -> [(
    label: String, count: Double, color: Color
  )] {
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
      segments.append(
        ("Reasoning", Double(usage.reasoningTokens), Color(nsColor: .systemPink).opacity(0.55)))
    }

    let regularOutput = max(0, usage.outputTokens - usage.reasoningTokens)
    if regularOutput > 0 {
      segments.append(("Output", Double(regularOutput), Color(nsColor: .systemGreen).opacity(0.55)))
    }

    return segments
  }

  private func TokenSegmentWidth(
    availableWidth: CGFloat,
    segmentCount: Double,
    total: Double
  ) -> CGFloat {
    guard total > 0 else {
      return 0
    }

    return availableWidth * CGFloat(segmentCount / total)
  }

  private func TokenHoverOffset(
    availableWidth: CGFloat,
    segments: [(label: String, count: Double, color: Color)],
    index: Int,
    total: Double
  ) -> CGFloat {
    guard index > 0 else {
      return 0
    }

    return segments.prefix(index).reduce(0) { value, segment in
      value
        + TokenSegmentWidth(
          availableWidth: availableWidth, segmentCount: segment.count, total: total)
    }
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

private func SegmentTooltipText(segment: TimelineSegment) -> String {
  let category = SegmentKindLabel(segment.kind)
  let duration = FormatDuration(segment.duration)
  let start = FormatClockTime(segment.startedAt)
  let end = FormatClockTime(segment.endedAt)

  if let label = segment.label, !label.isEmpty {
    return "\(category) · \(duration) · \(start)-\(end) · \(label)"
  }
  return "\(category) · \(duration) · \(start)-\(end)"
}

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
