import Foundation
import Observation

@Observable
final class TurnStore {
  private var turnsByKey: [String: ActiveTurn] = [:]
  private var completedRunsByEndpoint: [String: [CompletedRun]] = [:]
  private var metadataByEndpoint: [String: EndpointMetadata] = [:]
  private(set) var activeEndpointIds: [String] = []
  private let completionRetentionSeconds: TimeInterval = 10
  private let maxCompletedRunsPerEndpoint = 50

  private func TurnKey(endpointId: String, turnId: String) -> String {
    "\(endpointId):\(turnId)"
  }

  func UpsertTurnStarted(endpointId: String, threadId: String?, turnId: String, at now: Date) {
    let key = TurnKey(endpointId: endpointId, turnId: turnId)
    if let existing = turnsByKey[key] {
      existing.ApplyStatus(.inProgress, at: now)
      existing.UpdateThreadId(threadId)
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
    threadId: String?,
    turnId: String,
    status: TurnExecutionStatus,
    at now: Date
  ) {
    let key = TurnKey(endpointId: endpointId, turnId: turnId)
    if let existing = turnsByKey[key] {
      existing.ApplyStatus(status, at: now)
      existing.UpdateThreadId(threadId)
      ArchiveCompletedTurnIfNeeded(existing)
      UpdateTurnMetadata(
        endpointId: endpointId, threadId: threadId, turnId: turnId, turn: nil, at: now)
      return
    }
    let turn = ActiveTurn(
      endpointId: endpointId, threadId: threadId, turnId: turnId, startedAt: now)
    turn.ApplyStatus(status, at: now)
    turnsByKey[key] = turn
    ArchiveCompletedTurnIfNeeded(turn)
    UpdateTurnMetadata(
      endpointId: endpointId, threadId: threadId, turnId: turnId, turn: nil, at: now)
  }

  func MarkTurnCompletedIfPresent(
    endpointId: String,
    threadId: String?,
    turnId: String,
    status: TurnExecutionStatus,
    at now: Date
  ) {
    let key = TurnKey(endpointId: endpointId, turnId: turnId)
    guard let existing = turnsByKey[key] else {
      return
    }
    existing.ApplyStatus(status, at: now)
    existing.UpdateThreadId(threadId)
    ArchiveCompletedTurnIfNeeded(existing)
    UpdateTurnMetadata(
      endpointId: endpointId, threadId: threadId, turnId: turnId, turn: nil, at: now)
  }

  func RecordProgress(
    endpointId: String,
    threadId: String?,
    turnId: String,
    category: ProgressCategory,
    state: ProgressState,
    label: String?,
    at now: Date
  ) {
    let key = TurnKey(endpointId: endpointId, turnId: turnId)
    let turn = turnsByKey[key]
    if turn == nil && state == .completed {
      var metadata = metadataByEndpoint[endpointId] ?? EndpointMetadata()
      if let threadId {
        metadata.threadId = threadId
      }
      metadata.turnId = turnId
      metadata.lastTraceCategory = category
      if let label, !label.isEmpty {
        metadata.lastTraceLabel = label
      }
      metadata.lastEventAt = now
      metadataByEndpoint[endpointId] = metadata
      return
    }

    let activeTurn =
      turn ?? ActiveTurn(endpointId: endpointId, threadId: threadId, turnId: turnId, startedAt: now)
    turnsByKey[key] = activeTurn
    activeTurn.UpdateThreadId(threadId)
    activeTurn.ApplyProgress(category: category, state: state, label: label, at: now)

    var metadata = metadataByEndpoint[endpointId] ?? EndpointMetadata()
    if let threadId {
      metadata.threadId = threadId
    }
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
    metadata.chatTitle = NonEmptyString(thread["title"]) ?? metadata.chatTitle
    metadata.cwd = NonEmptyString(thread["cwd"]) ?? metadata.cwd
    metadata.model = ExtractModelIdentifier(from: thread) ?? metadata.model
    metadata.modelProvider = ExtractModelProvider(from: thread) ?? metadata.modelProvider

    if let fallbackPreview = NonEmptyString(thread["preview"]) {
      metadata.promptPreview = fallbackPreview
    }

    if let turns = thread["turns"] as? [[String: Any]] {
      metadata.chatTurnCount = turns.count
      if let latestTurn = turns.last {
        metadata.turnId = NonEmptyString(latestTurn["id"]) ?? metadata.turnId
        metadata.model = ExtractModelIdentifier(from: latestTurn) ?? metadata.model
        metadata.modelProvider = ExtractModelProvider(from: latestTurn) ?? metadata.modelProvider
        if let threadId = metadata.threadId,
          let turnId = metadata.turnId
        {
          let key = TurnKey(endpointId: endpointId, turnId: turnId)
          turnsByKey[key]?.UpdateThreadId(threadId)
        }
        if let promptPreview = ExtractPromptPreview(from: latestTurn) {
          metadata.promptPreview = promptPreview
        }
        if let cwd = ExtractLatestCwd(from: latestTurn) {
          metadata.cwd = cwd
        }
      }
    }
    metadata.lastEventAt = now
    metadataByEndpoint[endpointId] = metadata
  }

  func UpdateTurnMetadata(
    endpointId: String,
    threadId: String?,
    turnId: String,
    turn: [String: Any]?,
    at now: Date
  ) {
    var metadata = metadataByEndpoint[endpointId] ?? EndpointMetadata()
    if let threadId {
      metadata.threadId = threadId
    }
    metadata.turnId = turnId
    if let turn {
      if let promptPreview = ExtractPromptPreview(from: turn) {
        metadata.promptPreview = promptPreview
      }

      metadata.model = ExtractModelIdentifier(from: turn) ?? metadata.model
      metadata.modelProvider = ExtractModelProvider(from: turn) ?? metadata.modelProvider
    }
    metadata.lastEventAt = now
    metadataByEndpoint[endpointId] = metadata
  }

  func ApplyItemMetadata(
    endpointId: String,
    threadId: String?,
    turnId: String,
    item: [String: Any],
    at now: Date
  ) {
    var metadata = metadataByEndpoint[endpointId] ?? EndpointMetadata()
    if let threadId {
      metadata.threadId = threadId
    }
    metadata.turnId = turnId

    if (item["type"] as? String) == "userMessage" {
      let pseudoTurn: [String: Any] = [
        "items": [item]
      ]
      if let promptPreview = ExtractPromptPreview(from: pseudoTurn) {
        metadata.promptPreview = promptPreview
      }
    }

    if (item["type"] as? String) == "commandExecution" {
      if let cwd = NonEmptyString(item["cwd"]) {
        metadata.cwd = cwd
      }
    }

    metadata.lastEventAt = now
    metadataByEndpoint[endpointId] = metadata
  }

  func UpdateTokenUsage(
    endpointId: String,
    threadId: String?,
    turnId: String?,
    tokenUsage: TokenUsageInfo
  ) {
    var metadata = metadataByEndpoint[endpointId] ?? EndpointMetadata()
    if let threadId {
      metadata.threadId = threadId
    }
    if let turnId {
      metadata.turnId = turnId
    }
    metadata.tokenUsage = tokenUsage
    metadataByEndpoint[endpointId] = metadata

    guard let turnId else {
      return
    }

    guard var runs = completedRunsByEndpoint[endpointId] else {
      return
    }

    guard
      let index = runs.firstIndex(where: { run in
        guard run.turnId == turnId else {
          return false
        }
        if let threadId {
          return run.threadId == threadId
        }
        return true
      })
    else {
      return
    }

    let run = runs[index]
    runs[index] = CompletedRun(
      endpointId: run.endpointId,
      threadId: run.threadId,
      turnId: run.turnId,
      startedAt: run.startedAt,
      endedAt: run.endedAt,
      status: run.status,
      latestLabel: run.latestLabel,
      promptPreview: run.promptPreview,
      model: run.model,
      modelProvider: run.modelProvider,
      tokenUsage: tokenUsage,
      fileChanges: run.fileChanges,
      commands: run.commands,
      traceHistory: run.traceHistory
    )
    completedRunsByEndpoint[endpointId] = runs
  }

  func RecordError(endpointId: String, error: ErrorInfo) {
    var metadata = metadataByEndpoint[endpointId] ?? EndpointMetadata()
    metadata.latestError = error
    metadataByEndpoint[endpointId] = metadata
  }

  func ClearError(endpointId: String) {
    guard var metadata = metadataByEndpoint[endpointId] else { return }
    metadata.latestError = nil
    metadataByEndpoint[endpointId] = metadata
  }

  func UpdateGitInfo(endpointId: String, gitInfo: GitInfo) {
    var metadata = metadataByEndpoint[endpointId] ?? EndpointMetadata()
    metadata.gitInfo = gitInfo
    metadataByEndpoint[endpointId] = metadata
  }

  func UpdateRateLimits(rateLimits: RateLimitInfo) {
    for endpointId in metadataByEndpoint.keys {
      metadataByEndpoint[endpointId]?.rateLimits = rateLimits
    }
    globalRateLimits = rateLimits
  }

  func UpdateSessionSource(endpointId: String, source: String) {
    var metadata = metadataByEndpoint[endpointId] ?? EndpointMetadata()
    metadata.sessionSource = source
    metadataByEndpoint[endpointId] = metadata
  }

  func UpdatePlan(endpointId: String, turnId: String, steps: [PlanStepInfo], explanation: String?) {
    let key = TurnKey(endpointId: endpointId, turnId: turnId)
    turnsByKey[key]?.UpdatePlan(steps: steps, explanation: explanation)
  }

  func RecordFileChange(endpointId: String, turnId: String, change: FileChangeSummary) {
    let key = TurnKey(endpointId: endpointId, turnId: turnId)
    turnsByKey[key]?.UpsertFileChange(change)
  }

  func RecordCommand(endpointId: String, turnId: String, command: CommandSummary) {
    let key = TurnKey(endpointId: endpointId, turnId: turnId)
    turnsByKey[key]?.UpsertCommand(command)
  }

  var globalRateLimits: RateLimitInfo?

  func ReconcileSnapshotActiveTurns(endpointId: String, activeTurnKeys: [String], at now: Date) {
    let activeSet = Set(activeTurnKeys)
    for turn in turnsByKey.values {
      guard turn.endpointId == endpointId else {
        continue
      }
      guard turn.status == .inProgress else {
        continue
      }
      let key = TurnKey(endpointId: endpointId, turnId: turn.turnId)
      let isActive =
        activeSet.contains(key) || activeSet.contains { $0.hasSuffix(":\(turn.turnId)") }
      if isActive { continue }
      turn.ApplyStatus(.completed, at: now)
      ArchiveCompletedTurnIfNeeded(turn)
    }
  }

  func ResolveThreadId(endpointId: String, turnId: String) -> String? {
    let key = TurnKey(endpointId: endpointId, turnId: turnId)
    return turnsByKey[key]?.threadId ?? metadataByEndpoint[endpointId]?.threadId
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
      let lhsThreadId = lhs.threadId ?? ""
      let rhsThreadId = rhs.threadId ?? ""
      if lhsThreadId != rhsThreadId {
        return lhsThreadId < rhsThreadId
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
          let lhsThreadId = lhs.threadId ?? ""
          let rhsThreadId = rhs.threadId ?? ""
          if lhsThreadId != rhsThreadId {
            return lhsThreadId < rhsThreadId
          }
          return lhs.turnId < rhs.turnId
        }
        .first
      let metadata = metadataByEndpoint[endpointId]
      return EndpointRow(
        endpointId: endpointId,
        activeTurn: activeTurn,
        recentRuns: completedRunsByEndpoint[endpointId] ?? [],
        chatTitle: metadata?.chatTitle,
        promptPreview: metadata?.promptPreview,
        chatTurnCount: metadata?.chatTurnCount,
        cwd: metadata?.cwd,
        model: metadata?.model,
        modelProvider: metadata?.modelProvider,
        threadId: activeTurn?.threadId ?? metadata?.threadId,
        turnId: activeTurn?.turnId ?? metadata?.turnId,
        lastTraceCategory: metadata?.lastTraceCategory,
        lastTraceLabel: activeTurn?.latestLabel ?? metadata?.lastTraceLabel,
        lastEventAt: metadata?.lastEventAt,
        tokenUsage: metadata?.tokenUsage,
        latestError: metadata?.latestError,
        fileChanges: activeTurn?.fileChanges ?? [],
        commands: activeTurn?.commands ?? [],
        planSteps: activeTurn?.planSteps ?? [],
        planExplanation: activeTurn?.planExplanation,
        gitInfo: metadata?.gitInfo,
        rateLimits: metadata?.rateLimits ?? globalRateLimits,
        sessionSource: metadata?.sessionSource
      )
    }
  }

  func SetActiveEndpointIds(_ endpointIds: [String]) {
    activeEndpointIds = endpointIds
  }

  var EndpointRows: [EndpointRow] {
    EndpointRows(activeEndpointIds: activeEndpointIds)
  }

  private func ArchiveCompletedTurnIfNeeded(_ turn: ActiveTurn) {
    guard turn.status != .inProgress, let endedAt = turn.endedAt else {
      return
    }

    var runs = completedRunsByEndpoint[turn.endpointId] ?? []
    let alreadyArchived = runs.contains {
      $0.turnId == turn.turnId && $0.threadId == turn.threadId
    }
    if alreadyArchived {
      return
    }

    let metadata = metadataByEndpoint[turn.endpointId]
    runs.insert(
      CompletedRun(
        endpointId: turn.endpointId,
        threadId: turn.threadId,
        turnId: turn.turnId,
        startedAt: turn.startedAt,
        endedAt: endedAt,
        status: turn.status,
        latestLabel: turn.latestLabel,
        promptPreview: metadata?.promptPreview,
        model: metadata?.model,
        modelProvider: metadata?.modelProvider,
        tokenUsage: metadata?.tokenUsage,
        fileChanges: turn.fileChanges,
        commands: turn.commands,
        traceHistory: turn.traceHistory
      ),
      at: 0
    )

    if runs.count > maxCompletedRunsPerEndpoint {
      runs.removeLast(runs.count - maxCompletedRunsPerEndpoint)
    }
    completedRunsByEndpoint[turn.endpointId] = runs
  }

  private func ExtractPromptPreview(from turn: [String: Any]) -> String? {
    guard let items = turn["items"] as? [[String: Any]] else {
      return nil
    }

    for item in items.reversed() {
      guard (item["type"] as? String) == "userMessage" else {
        continue
      }

      if let preview = ExtractContentPreview(item["content"]) {
        return preview
      }
    }

    return nil
  }

  private func ExtractContentPreview(_ content: Any?) -> String? {
    if let value = NonEmptyString(content) {
      return value
    }

    if let dict = content as? [String: Any], let value = NonEmptyString(dict["text"]) {
      return value
    }

    guard let items = content as? [Any] else {
      return nil
    }

    let textParts = items.compactMap { item -> String? in
      if let text = NonEmptyString(item) {
        return text
      }

      if let dict = item as? [String: Any] {
        return NonEmptyString(dict["text"])
      }

      return nil
    }
    let combined = textParts.joined(separator: " ").trimmingCharacters(
      in: .whitespacesAndNewlines)
    return combined.isEmpty ? nil : combined
  }

  private func ExtractLatestCwd(from turn: [String: Any]) -> String? {
    guard let items = turn["items"] as? [[String: Any]] else {
      return nil
    }

    for item in items.reversed() {
      guard (item["type"] as? String) == "commandExecution" else {
        continue
      }
      if let cwd = NonEmptyString(item["cwd"]) {
        return cwd
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

  private func ExtractModelIdentifier(from payload: [String: Any]) -> String? {
    let directKeys = ["model", "modelSlug", "model_slug", "modelName", "model_name"]
    for key in directKeys {
      if let value = NonEmptyString(payload[key]) {
        return value
      }
    }

    if let model = payload["model"] as? [String: Any] {
      if let value = NonEmptyString(model["slug"]) ?? NonEmptyString(model["name"])
        ?? NonEmptyString(model["id"])
      {
        return value
      }
    }

    if let modelInfo = payload["modelInfo"] as? [String: Any] {
      if let value = NonEmptyString(modelInfo["slug"]) ?? NonEmptyString(modelInfo["name"])
        ?? NonEmptyString(modelInfo["id"])
      {
        return value
      }
    }

    return nil
  }

  private func ExtractModelProvider(from payload: [String: Any]) -> String? {
    let directKeys = ["modelProvider", "model_provider", "provider", "modelVendor", "model_vendor"]
    for key in directKeys {
      if let value = NonEmptyString(payload[key]) {
        return value
      }
    }

    if let model = payload["model"] as? [String: Any] {
      if let value = NonEmptyString(model["provider"]) ?? NonEmptyString(model["vendor"]) {
        return value
      }
    }

    if let modelInfo = payload["modelInfo"] as? [String: Any] {
      if let value = NonEmptyString(modelInfo["provider"]) ?? NonEmptyString(modelInfo["vendor"]) {
        return value
      }
    }

    return nil
  }
}
