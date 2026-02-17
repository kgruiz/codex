import Foundation

final class TurnStore {
  private var turnsByKey: [String: ActiveTurn] = [:]
  private var metadataByEndpoint: [String: EndpointMetadata] = [:]
  private let completionRetentionSeconds: TimeInterval = 10

  private func TurnKey(endpointId: String, threadId: String, turnId: String) -> String {
    "\(endpointId):\(threadId):\(turnId)"
  }

  func UpsertTurnStarted(endpointId: String, threadId: String, turnId: String, at now: Date) {
    let key = TurnKey(endpointId: endpointId, threadId: threadId, turnId: turnId)
    if let existing = turnsByKey[key] {
      existing.ApplyStatus(.inProgress, at: now)
      UpdateTurnMetadata(
        endpointId: endpointId, threadId: threadId, turnId: turnId, turn: nil, at: now)
      return
    }
    turnsByKey[key] = ActiveTurn(
      endpointId: endpointId, threadId: threadId, turnId: turnId, startedAt: now)
    UpdateTurnMetadata(
      endpointId: endpointId, threadId: threadId, turnId: turnId, turn: nil, at: now)
  }

  func MarkTurnCompleted(
    endpointId: String,
    threadId: String,
    turnId: String,
    status: TurnExecutionStatus,
    at now: Date
  ) {
    let key = TurnKey(endpointId: endpointId, threadId: threadId, turnId: turnId)
    if let existing = turnsByKey[key] {
      existing.ApplyStatus(status, at: now)
      UpdateTurnMetadata(
        endpointId: endpointId, threadId: threadId, turnId: turnId, turn: nil, at: now)
      return
    }
    let turn = ActiveTurn(
      endpointId: endpointId, threadId: threadId, turnId: turnId, startedAt: now)
    turn.ApplyStatus(status, at: now)
    turnsByKey[key] = turn
    UpdateTurnMetadata(
      endpointId: endpointId, threadId: threadId, turnId: turnId, turn: nil, at: now)
  }

  func MarkTurnCompletedIfPresent(
    endpointId: String,
    threadId: String,
    turnId: String,
    status: TurnExecutionStatus,
    at now: Date
  ) {
    let key = TurnKey(endpointId: endpointId, threadId: threadId, turnId: turnId)
    guard let existing = turnsByKey[key] else {
      return
    }
    existing.ApplyStatus(status, at: now)
    UpdateTurnMetadata(
      endpointId: endpointId, threadId: threadId, turnId: turnId, turn: nil, at: now)
  }

  func RecordProgress(
    endpointId: String,
    threadId: String,
    turnId: String,
    category: ProgressCategory,
    state: ProgressState,
    label: String?,
    at now: Date
  ) {
    let key = TurnKey(endpointId: endpointId, threadId: threadId, turnId: turnId)
    let turn =
      turnsByKey[key]
      ?? ActiveTurn(endpointId: endpointId, threadId: threadId, turnId: turnId, startedAt: now)
    turnsByKey[key] = turn
    turn.ApplyProgress(category: category, state: state, label: label, at: now)

    var metadata = metadataByEndpoint[endpointId] ?? EndpointMetadata()
    metadata.threadId = threadId
    metadata.turnId = turnId
    metadata.lastTraceCategory = category
    if let label, !label.isEmpty {
      metadata.lastTraceLabel = label
    }
    metadata.lastEventAt = now
    metadataByEndpoint[endpointId] = metadata
  }

  func ApplyThreadSnapshot(endpointId: String, thread: [String: Any], at now: Date) {
    var metadata = metadataByEndpoint[endpointId] ?? EndpointMetadata()
    metadata.threadId = NonEmptyString(thread["id"]) ?? metadata.threadId
    metadata.chatTitle = NonEmptyString(thread["preview"]) ?? metadata.chatTitle
    metadata.cwd = NonEmptyString(thread["cwd"]) ?? metadata.cwd
    metadata.model =
      NonEmptyString(thread["model"])
      ?? NonEmptyString(thread["modelProvider"])
      ?? metadata.model

    if let turns = thread["turns"] as? [[String: Any]], let latestTurn = turns.last {
      metadata.turnId = NonEmptyString(latestTurn["id"]) ?? metadata.turnId
      if let promptPreview = ExtractPromptPreview(from: latestTurn) {
        metadata.promptPreview = promptPreview
      }
    }
    metadata.lastEventAt = now
    metadataByEndpoint[endpointId] = metadata
  }

  func UpdateTurnMetadata(
    endpointId: String,
    threadId: String,
    turnId: String,
    turn: [String: Any]?,
    at now: Date
  ) {
    var metadata = metadataByEndpoint[endpointId] ?? EndpointMetadata()
    metadata.threadId = threadId
    metadata.turnId = turnId
    if let turn, let promptPreview = ExtractPromptPreview(from: turn) {
      metadata.promptPreview = promptPreview
    }
    metadata.lastEventAt = now
    metadataByEndpoint[endpointId] = metadata
  }

  func ApplyItemMetadata(
    endpointId: String,
    threadId: String,
    turnId: String,
    item: [String: Any],
    at now: Date
  ) {
    var metadata = metadataByEndpoint[endpointId] ?? EndpointMetadata()
    metadata.threadId = threadId
    metadata.turnId = turnId

    if (item["type"] as? String) == "userMessage" {
      let pseudoTurn: [String: Any] = [
        "items": [item]
      ]
      if let promptPreview = ExtractPromptPreview(from: pseudoTurn) {
        metadata.promptPreview = promptPreview
      }
    }

    metadata.lastEventAt = now
    metadataByEndpoint[endpointId] = metadata
  }

  func Tick(now: Date) {
    let expiredKeys = turnsByKey.compactMap { key, turn -> String? in
      guard let endedAt = turn.endedAt else {
        return nil
      }
      if now.timeIntervalSince(endedAt) >= completionRetentionSeconds {
        return key
      }
      return nil
    }
    for key in expiredKeys {
      turnsByKey.removeValue(forKey: key)
    }
  }

  func Snapshot() -> [ActiveTurn] {
    turnsByKey.values.sorted { lhs, rhs in
      if lhs.status == .inProgress && rhs.status != .inProgress {
        return true
      }
      if lhs.status != .inProgress && rhs.status == .inProgress {
        return false
      }
      if lhs.startedAt != rhs.startedAt {
        return lhs.startedAt > rhs.startedAt
      }
      if lhs.threadId != rhs.threadId {
        return lhs.threadId < rhs.threadId
      }
      if lhs.endpointId != rhs.endpointId {
        return lhs.endpointId < rhs.endpointId
      }
      return lhs.turnId < rhs.turnId
    }
  }

  func RunningTurnCount() -> Int {
    turnsByKey.values.filter { $0.status == .inProgress }.count
  }

  func EndpointRows(activeEndpointIds: [String]) -> [EndpointRow] {
    var endpointIds = Set(activeEndpointIds)
    for turn in turnsByKey.values where turn.status == .inProgress {
      endpointIds.insert(turn.endpointId)
    }

    let sortedEndpointIds = endpointIds.sorted()
    return sortedEndpointIds.map { endpointId in
      let activeTurn = turnsByKey.values
        .filter { $0.endpointId == endpointId && $0.status == .inProgress }
        .sorted { lhs, rhs in
          if lhs.startedAt != rhs.startedAt {
            return lhs.startedAt > rhs.startedAt
          }
          if lhs.threadId != rhs.threadId {
            return lhs.threadId < rhs.threadId
          }
          return lhs.turnId < rhs.turnId
        }
        .first
      let metadata = metadataByEndpoint[endpointId]
      return EndpointRow(
        endpointId: endpointId,
        activeTurn: activeTurn,
        chatTitle: metadata?.chatTitle,
        promptPreview: metadata?.promptPreview,
        cwd: metadata?.cwd,
        model: metadata?.model,
        threadId: activeTurn?.threadId ?? metadata?.threadId,
        turnId: activeTurn?.turnId ?? metadata?.turnId,
        lastTraceCategory: metadata?.lastTraceCategory,
        lastTraceLabel: activeTurn?.latestLabel ?? metadata?.lastTraceLabel,
        lastEventAt: metadata?.lastEventAt
      )
    }
  }

  private func ExtractPromptPreview(from turn: [String: Any]) -> String? {
    guard let items = turn["items"] as? [[String: Any]] else {
      return nil
    }

    for item in items.reversed() {
      guard (item["type"] as? String) == "userMessage" else {
        continue
      }
      guard let content = item["content"] as? [[String: Any]] else {
        continue
      }
      let textParts = content.compactMap { input -> String? in
        guard (input["type"] as? String) == "text" else {
          return nil
        }
        return NonEmptyString(input["text"])
      }
      let combined = textParts.joined(separator: " ").trimmingCharacters(
        in: .whitespacesAndNewlines)
      if !combined.isEmpty {
        return combined
      }
    }

    return nil
  }

  private func NonEmptyString(_ value: Any?) -> String? {
    guard let value = value as? String else {
      return nil
    }
    let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
    return trimmed.isEmpty ? nil : trimmed
  }
}
