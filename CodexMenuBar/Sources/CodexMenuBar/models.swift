import Foundation
import Observation

enum TurnExecutionStatus: Equatable {
  case inProgress
  case completed
  case interrupted
  case failed

  init(serverValue: String) {
    switch serverValue {
    case "completed":
      self = .completed
    case "interrupted":
      self = .interrupted
    case "failed":
      self = .failed
    default:
      self = .inProgress
    }
  }
}

enum ProgressCategory: String, CaseIterable {
  case tool
  case edit
  case waiting
  case network
  case prefill
  case reasoning
  case gen

  var sortOrder: Int {
    switch self {
    case .tool:
      return 0
    case .edit:
      return 1
    case .waiting:
      return 2
    case .network:
      return 3
    case .prefill:
      return 4
    case .reasoning:
      return 5
    case .gen:
      return 6
    }
  }
}

enum ProgressState: String {
  case started
  case completed
}

struct ProgressTraceSnapshot: Equatable {
  let category: ProgressCategory
  let state: ProgressState
  let label: String?
  let timestamp: Date
}

enum TimelineSegmentKind: Equatable {
  case category(ProgressCategory)
  case idle
}

struct TimelineSegment: Equatable {
  let kind: TimelineSegmentKind
  let startedAt: Date
  let endedAt: Date
  let label: String?

  var duration: TimeInterval {
    max(0, endedAt.timeIntervalSince(startedAt))
  }
}

struct TokenUsageInfo: Equatable {
  var inputTokens: Int = 0
  var cachedInputTokens: Int = 0
  var outputTokens: Int = 0
  var reasoningTokens: Int = 0
  var totalTokens: Int = 0
  var contextWindow: Int?

  var contextUsageFraction: Double? {
    guard let contextWindow, contextWindow > 0 else { return nil }
    return min(1.0, Double(totalTokens) / Double(contextWindow))
  }
}

struct ErrorInfo: Equatable {
  let message: String
  let details: String?
  let willRetry: Bool
  let occurredAt: Date
}

struct FileChangeSummary: Equatable {
  let path: String
  let kind: FileChangeKind
}

enum FileChangeKind: String, Equatable {
  case add = "Add"
  case delete = "Delete"
  case update = "Update"

  init(serverValue: String) {
    switch serverValue.lowercased() {
    case "add":
      self = .add
    case "delete":
      self = .delete
    default:
      self = .update
    }
  }

  var label: String {
    switch self {
    case .add: return "A"
    case .delete: return "D"
    case .update: return "M"
    }
  }
}

struct CommandSummary: Equatable {
  let command: String
  let status: CommandExecutionState
  let exitCode: Int?
  let durationMs: Int?
}

enum CommandExecutionState: String, Equatable {
  case inProgress
  case completed
  case failed
  case declined

  init(serverValue: String) {
    switch serverValue.lowercased() {
    case "completed":
      self = .completed
    case "failed":
      self = .failed
    case "declined":
      self = .declined
    default:
      self = .inProgress
    }
  }
}

struct PlanStepInfo: Equatable {
  let description: String
  let status: PlanStepStatus
}

enum PlanStepStatus: String, Equatable {
  case pending
  case inProgress
  case completed

  init(serverValue: String) {
    switch serverValue.lowercased() {
    case "completed":
      self = .completed
    case "in_progress", "inprogress":
      self = .inProgress
    default:
      self = .pending
    }
  }
}

struct GitInfo: Equatable {
  var branch: String?
  var sha: String?
}

struct RateLimitInfo: Equatable {
  var remaining: Int?
  var limit: Int?
  var resetsAt: Date?
}

struct EndpointMetadata {
  var chatTitle: String?
  var promptPreview: String?
  var chatTurnCount: Int?
  var cwd: String?
  var model: String?
  var modelProvider: String?
  var thinkingLevel: String?
  var threadId: String?
  var turnId: String?
  var lastTraceCategory: ProgressCategory?
  var lastTraceLabel: String?
  var lastEventAt: Date?
  var tokenUsage: TokenUsageInfo?
  var latestError: ErrorInfo?
  var gitInfo: GitInfo?
  var rateLimits: RateLimitInfo?
  var sessionSource: String?
}

struct EndpointRow {
  let endpointId: String
  let activeTurn: ActiveTurn?
  let recentRuns: [CompletedRun]
  let chatTitle: String?
  let promptPreview: String?
  let chatTurnCount: Int?
  let cwd: String?
  let model: String?
  let modelProvider: String?
  let thinkingLevel: String?
  let threadId: String?
  let turnId: String?
  let lastTraceCategory: ProgressCategory?
  let lastTraceLabel: String?
  let lastEventAt: Date?
  let tokenUsage: TokenUsageInfo?
  let latestError: ErrorInfo?
  let fileChanges: [FileChangeSummary]
  let commands: [CommandSummary]
  let planSteps: [PlanStepInfo]
  let planExplanation: String?
  let gitInfo: GitInfo?
  let rateLimits: RateLimitInfo?
  let sessionSource: String?

  var displayName: String {
    if let cwd { return (cwd as NSString).lastPathComponent }
    if let title = chatTitle, !title.isEmpty { return title }
    return String(endpointId.prefix(8))
  }

  var shortId: String {
    String(endpointId.prefix(5))
  }
}

struct CompletedRun: Equatable {
  let endpointId: String
  let threadId: String?
  let turnId: String
  let startedAt: Date
  let endedAt: Date
  let status: TurnExecutionStatus
  let latestLabel: String?
  let promptPreview: String?
  let model: String?
  let modelProvider: String?
  let thinkingLevel: String?
  let tokenUsage: TokenUsageInfo?
  let fileChanges: [FileChangeSummary]
  let commands: [CommandSummary]
  let traceHistory: [ProgressTraceSnapshot]

  var runKey: String {
    let threadPart = threadId ?? "no-thread"
    let startedAtSeconds = Int(startedAt.timeIntervalSince1970)
    return "\(threadPart):\(turnId):\(startedAtSeconds)"
  }

  func ElapsedString() -> String {
    let elapsed = max(0, endedAt.timeIntervalSince(startedAt))
    return FormatElapsedDuration(elapsed)
  }

  func RanAtString() -> String {
    let calendar = Calendar.current
    if calendar.isDateInToday(endedAt) {
      return "Today \(FormatRunClockTime(endedAt))"
    }
    if calendar.isDateInYesterday(endedAt) {
      return "Yesterday \(FormatRunClockTime(endedAt))"
    }
    return FormatRunDateAndClockTime(endedAt)
  }

  func TimelineSegments() -> [TimelineSegment] {
    BuildTimelineSegments(
      startedAt: startedAt,
      endDate: endedAt,
      traceHistory: traceHistory
    )
  }
}

@Observable
final class ActiveTurn {
  let endpointId: String
  private(set) var threadId: String?
  let turnId: String
  let startedAt: Date
  private(set) var status: TurnExecutionStatus
  private(set) var endedAt: Date?
  private(set) var latestLabel: String?
  private var categoryCounts: [ProgressCategory: Int]
  private var seenCategories: [ProgressCategory]
  private(set) var traceHistory: [ProgressTraceSnapshot]
  private(set) var fileChanges: [FileChangeSummary] = []
  private(set) var commands: [CommandSummary] = []
  private(set) var planSteps: [PlanStepInfo] = []
  private(set) var planExplanation: String?

  init(endpointId: String, threadId: String?, turnId: String, startedAt: Date) {
    self.endpointId = endpointId
    self.threadId = threadId
    self.turnId = turnId
    self.startedAt = startedAt
    self.status = .inProgress
    self.endedAt = nil
    self.latestLabel = nil
    self.categoryCounts = [:]
    self.seenCategories = []
    self.traceHistory = []
  }

  func ApplyStatus(_ nextStatus: TurnExecutionStatus, at now: Date) {
    status = nextStatus
    if nextStatus == .inProgress {
      endedAt = nil
    } else {
      endedAt = now
    }
  }

  func UpdateThreadId(_ threadId: String?) {
    guard let threadId, !threadId.isEmpty else {
      return
    }
    self.threadId = threadId
  }

  func ApplyProgress(
    category: ProgressCategory,
    state: ProgressState,
    label: String?,
    at now: Date
  ) {
    if !seenCategories.contains(category) {
      seenCategories.append(category)
    }

    switch state {
    case .started:
      let count = categoryCounts[category] ?? 0
      categoryCounts[category] = count + 1
    case .completed:
      let count = categoryCounts[category] ?? 0
      categoryCounts[category] = max(0, count - 1)
    }

    if let labelValue = label, !labelValue.isEmpty {
      latestLabel = labelValue
    }

    traceHistory.append(
      ProgressTraceSnapshot(
        category: category,
        state: state,
        label: label,
        timestamp: now
      )
    )
    if traceHistory.count > 128 {
      traceHistory.removeFirst(traceHistory.count - 128)
    }
  }

  func UpsertFileChange(_ change: FileChangeSummary) {
    if let index = fileChanges.firstIndex(where: { $0.path == change.path }) {
      fileChanges[index] = change
    } else {
      fileChanges.append(change)
    }
  }

  func UpsertCommand(_ command: CommandSummary) {
    if let index = commands.firstIndex(where: { $0.command == command.command }) {
      commands[index] = command
    } else {
      commands.append(command)
    }
    if commands.count > 20 {
      commands.removeFirst(commands.count - 20)
    }
  }

  func UpdatePlan(steps: [PlanStepInfo], explanation: String?) {
    planSteps = steps
    if let explanation, !explanation.isEmpty {
      planExplanation = explanation
    }
  }

  func ActiveCategories() -> [ProgressCategory] {
    let running =
      categoryCounts
      .compactMap { category, count in count > 0 ? category : nil }
      .sorted { $0.sortOrder < $1.sortOrder }
    if !running.isEmpty {
      return running
    }

    let fallback = seenCategories.suffix(3)
    return Array(fallback)
  }

  func ElapsedString(now: Date) -> String {
    let endDate = endedAt ?? now
    let elapsed = max(0, endDate.timeIntervalSince(startedAt))
    return FormatElapsedDuration(elapsed)
  }

  func TimelineSegments(now: Date) -> [TimelineSegment] {
    let endDate = endedAt ?? now
    return BuildTimelineSegments(
      startedAt: startedAt,
      endDate: endDate,
      traceHistory: traceHistory
    )
  }

  private func AppendSegment(
    into segments: inout [TimelineSegment],
    start: Date,
    end: Date,
    activeCounts: [ProgressCategory: Int],
    activeStartedAt: [ProgressCategory: Date],
    activeLabels: [ProgressCategory: String]
  ) {
    if end <= start {
      return
    }

    let activeCategory =
      activeCounts
      .compactMap { category, count in count > 0 ? category : nil }
      .sorted { lhs, rhs in
        let lhsStartedAt = activeStartedAt[lhs] ?? Date.distantPast
        let rhsStartedAt = activeStartedAt[rhs] ?? Date.distantPast
        if lhsStartedAt != rhsStartedAt {
          return lhsStartedAt > rhsStartedAt
        }
        return lhs.sortOrder < rhs.sortOrder
      }
      .first

    let kind: TimelineSegmentKind
    let label: String?
    if let activeCategory {
      kind = .category(activeCategory)
      label = activeLabels[activeCategory]
    } else {
      kind = .idle
      label = nil
    }

    if var last = segments.last, last.kind == kind, last.label == label,
      abs(last.endedAt.timeIntervalSince(start)) < 0.001
    {
      last = TimelineSegment(
        kind: last.kind, startedAt: last.startedAt, endedAt: end, label: last.label)
      segments[segments.count - 1] = last
      return
    }

    segments.append(TimelineSegment(kind: kind, startedAt: start, endedAt: end, label: label))
  }
}

func FormatTokenCount(_ count: Int) -> String {
  if count >= 1_000_000 {
    let value = Double(count) / 1_000_000.0
    return String(format: "%.1fM", value)
  }
  if count >= 1_000 {
    let value = Double(count) / 1_000.0
    return String(format: "%.1fk", value)
  }
  return "\(count)"
}

private func FormatElapsedDuration(_ elapsed: TimeInterval) -> String {
  let totalSeconds = Int(elapsed)
  if totalSeconds < 60 {
    return "\(totalSeconds)s"
  }
  if totalSeconds < 3600 {
    let minutes = totalSeconds / 60
    let seconds = totalSeconds % 60
    return "\(minutes)m \(String(format: "%02d", seconds))s"
  }
  let hours = totalSeconds / 3600
  let minutes = (totalSeconds % 3600) / 60
  let seconds = totalSeconds % 60
  return "\(hours)h \(String(format: "%02d", minutes))m \(String(format: "%02d", seconds))s"
}

private let runClockTimeFormatter: DateFormatter = {
  let formatter = DateFormatter()
  formatter.timeStyle = .short
  formatter.dateStyle = .none
  return formatter
}()

private let runDateAndTimeFormatter: DateFormatter = {
  let formatter = DateFormatter()
  formatter.timeStyle = .short
  formatter.dateStyle = .medium
  return formatter
}()

private func FormatRunClockTime(_ date: Date) -> String {
  runClockTimeFormatter.string(from: date)
}

private func FormatRunDateAndClockTime(_ date: Date) -> String {
  runDateAndTimeFormatter.string(from: date)
}

private func BuildTimelineSegments(
  startedAt: Date,
  endDate: Date,
  traceHistory: [ProgressTraceSnapshot]
) -> [TimelineSegment] {
  if endDate <= startedAt {
    return []
  }

  var segments: [TimelineSegment] = []
  var activeCounts: [ProgressCategory: Int] = [:]
  var activeStartedAt: [ProgressCategory: Date] = [:]
  var activeLabels: [ProgressCategory: String] = [:]
  var cursor = startedAt

  for snapshot in traceHistory {
    let timestamp = min(max(snapshot.timestamp, startedAt), endDate)
    if timestamp > cursor {
      AppendSegment(
        into: &segments,
        start: cursor,
        end: timestamp,
        activeCounts: activeCounts,
        activeStartedAt: activeStartedAt,
        activeLabels: activeLabels
      )
      cursor = timestamp
    }

    switch snapshot.state {
    case .started:
      let count = activeCounts[snapshot.category] ?? 0
      activeCounts[snapshot.category] = count + 1
      activeStartedAt[snapshot.category] = timestamp
      if let label = snapshot.label, !label.isEmpty {
        activeLabels[snapshot.category] = label
      }
    case .completed:
      let count = activeCounts[snapshot.category] ?? 0
      let nextCount = max(0, count - 1)
      activeCounts[snapshot.category] = nextCount
      if nextCount == 0 {
        activeStartedAt.removeValue(forKey: snapshot.category)
        activeLabels.removeValue(forKey: snapshot.category)
      }
    }
  }

  if endDate > cursor {
    AppendSegment(
      into: &segments,
      start: cursor,
      end: endDate,
      activeCounts: activeCounts,
      activeStartedAt: activeStartedAt,
      activeLabels: activeLabels
    )
  }

  return segments
}

private func AppendSegment(
  into segments: inout [TimelineSegment],
  start: Date,
  end: Date,
  activeCounts: [ProgressCategory: Int],
  activeStartedAt: [ProgressCategory: Date],
  activeLabels: [ProgressCategory: String]
) {
  if end <= start {
    return
  }

  let activeCategory =
    activeCounts
    .compactMap { category, count in count > 0 ? category : nil }
    .sorted { lhs, rhs in
      let lhsStartedAt = activeStartedAt[lhs] ?? Date.distantPast
      let rhsStartedAt = activeStartedAt[rhs] ?? Date.distantPast
      if lhsStartedAt != rhsStartedAt {
        return lhsStartedAt > rhsStartedAt
      }
      return lhs.sortOrder < rhs.sortOrder
    }
    .first

  let kind: TimelineSegmentKind
  let label: String?
  if let activeCategory {
    kind = .category(activeCategory)
    label = activeLabels[activeCategory]
  } else {
    kind = .idle
    label = nil
  }

  if var last = segments.last, last.kind == kind, last.label == label,
    abs(last.endedAt.timeIntervalSince(start)) < 0.001
  {
    last = TimelineSegment(
      kind: last.kind, startedAt: last.startedAt, endedAt: end, label: last.label)
    segments[segments.count - 1] = last
    return
  }

  segments.append(TimelineSegment(kind: kind, startedAt: start, endedAt: end, label: label))
}
