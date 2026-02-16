import Foundation

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

struct ProgressTraceSnapshot {
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

final class ActiveTurn {
  let endpointId: String
  let threadId: String
  let turnId: String
  let startedAt: Date
  private(set) var status: TurnExecutionStatus
  private(set) var endedAt: Date?
  private(set) var latestLabel: String?
  private var categoryCounts: [ProgressCategory: Int]
  private var seenCategories: [ProgressCategory]
  private(set) var traceHistory: [ProgressTraceSnapshot]

  init(endpointId: String, threadId: String, turnId: String, startedAt: Date) {
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

  func TimelineSegments(now: Date) -> [TimelineSegment] {
    let endDate = endedAt ?? now
    if endDate <= startedAt {
      return []
    }

    var segments: [TimelineSegment] = []
    var activeCounts: [ProgressCategory: Int] = [:]
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
          activeLabels: activeLabels
        )
        cursor = timestamp
      }

      switch snapshot.state {
      case .started:
        let count = activeCounts[snapshot.category] ?? 0
        activeCounts[snapshot.category] = count + 1
        if let label = snapshot.label, !label.isEmpty {
          activeLabels[snapshot.category] = label
        }
      case .completed:
        let count = activeCounts[snapshot.category] ?? 0
        let nextCount = max(0, count - 1)
        activeCounts[snapshot.category] = nextCount
        if nextCount == 0 {
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
    activeLabels: [ProgressCategory: String]
  ) {
    if end <= start {
      return
    }

    let activeCategory =
      activeCounts
      .compactMap { category, count in count > 0 ? category : nil }
      .sorted { $0.sortOrder < $1.sortOrder }
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
