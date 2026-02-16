import Foundation

final class TurnStore {
  private var turnsByKey: [String: ActiveTurn] = [:]
  private let completionRetentionSeconds: TimeInterval = 10

  private func TurnKey(endpointId: String, threadId: String, turnId: String) -> String {
    "\(endpointId):\(threadId):\(turnId)"
  }

  func UpsertTurnStarted(endpointId: String, threadId: String, turnId: String, at now: Date) {
    let key = TurnKey(endpointId: endpointId, threadId: threadId, turnId: turnId)
    if let existing = turnsByKey[key] {
      existing.ApplyStatus(.inProgress, at: now)
      return
    }
    turnsByKey[key] = ActiveTurn(
      endpointId: endpointId, threadId: threadId, turnId: turnId, startedAt: now)
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
      return
    }
    let turn = ActiveTurn(
      endpointId: endpointId, threadId: threadId, turnId: turnId, startedAt: now)
    turn.ApplyStatus(status, at: now)
    turnsByKey[key] = turn
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
}
