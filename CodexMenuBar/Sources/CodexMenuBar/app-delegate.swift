import AppKit
import Foundation

final class AppDelegate: NSObject, NSApplicationDelegate {
  private let turnStore = TurnStore()
  private lazy var model = MenuBarViewModel(turnStore: turnStore)
  private lazy var statusMenu = StatusMenuController(model: model)
  private let appServerClient = AppServerClient()
  private let terminalLauncher = TerminalLauncher()

  private var timer: Timer?

  func applicationDidFinishLaunching(_ notification: Notification) {
    NSApplication.shared.setActivationPolicy(.accessory)
    ConfigureStatusMenu()
    ConfigureClient()
    StartTimer()
    appServerClient.Start()
  }

  func applicationWillTerminate(_ notification: Notification) {
    timer?.invalidate()
    timer = nil
    appServerClient.Stop()
  }

  private func ConfigureStatusMenu() {
    statusMenu.ReconnectHandler = { [weak self] in
      self?.appServerClient.Restart()
    }
    statusMenu.ReconnectEndpointHandler = { [weak self] endpointId in
      self?.appServerClient.ReconnectEndpoint(endpointId)
    }
    statusMenu.QuitHandler = {
      NSApplication.shared.terminate(nil)
    }
    statusMenu.QuickStartHandler = { [weak self] in
      self?.terminalLauncher.LaunchQuickStart()
    }
    statusMenu.OpenTerminalHandler = { [weak self] workingDirectory in
      self?.terminalLauncher.OpenTerminal(at: workingDirectory)
    }
  }

  private func ConfigureClient() {
    appServerClient.OnStateChange = { [weak self] state in
      guard let self else {
        return
      }
      self.model.connectionState = state
    }

    appServerClient.OnEndpointIdsChanged = { [weak self] endpointIds in
      guard let self else {
        return
      }
      self.model.SetEndpointIds(endpointIds)
    }

    appServerClient.OnNotification = { [weak self] method, params in
      guard let self else {
        return
      }
      self.HandleNotification(method: method, params: params)
    }
  }

  private func StartTimer() {
    timer = Timer.scheduledTimer(
      timeInterval: 1.0,
      target: self,
      selector: #selector(OnTimerTick),
      userInfo: nil,
      repeats: true
    )
  }

  @objc
  private func OnTimerTick() {
    let now = Date()
    turnStore.Tick(now: now)
    model.now = now
    model.SyncSectionDisclosureState()
  }

  private func HandleNotification(method: String, params: [String: Any]) {
    switch method {
    case "thread/snapshot":
      HandleThreadSnapshot(params: params)
    case "thread/snapshotSummary":
      HandleThreadSnapshotSummary(params: params)
    case "thread/started":
      HandleThreadStarted(params: params)
    case "thread/tokenUsage/updated":
      HandleTokenUsageUpdated(params: params)
    case "turn/started":
      HandleTurnStarted(params: params)
    case "turn/completed":
      HandleTurnCompleted(params: params)
    case "turn/progressTrace":
      HandleTurnProgressTrace(params: params)
    case "turn/plan/updated":
      HandleTurnPlanUpdated(params: params)
    case "item/started":
      HandleItemLifecycle(params: params, state: .started)
    case "item/completed":
      HandleItemLifecycle(params: params, state: .completed)
    case "error":
      HandleError(params: params)
    case "account/rateLimits/updated":
      HandleRateLimitsUpdated(params: params)
    default:
      break
    }
    model.SyncSectionDisclosureState()
  }

  private func HandleTurnStarted(params: [String: Any]) {
    let endpointId = params["endpointId"] as? String ?? "unknown"
    guard
      let turn = params["turn"] as? [String: Any],
      let turnId = turn["id"] as? String
    else {
      return
    }
    let threadId = ResolveThreadId(params: params, endpointId: endpointId, turnId: turnId)
    turnStore.ClearError(endpointId: endpointId)
    turnStore.UpsertTurnStarted(
      endpointId: endpointId, threadId: threadId, turnId: turnId, at: Date())
    turnStore.UpdateTurnMetadata(
      endpointId: endpointId, threadId: threadId, turnId: turnId, turn: turn, at: Date())
  }

  private func HandleTurnCompleted(params: [String: Any]) {
    let endpointId = params["endpointId"] as? String ?? "unknown"
    guard
      let turn = params["turn"] as? [String: Any],
      let turnId = turn["id"] as? String
    else {
      return
    }
    let threadId = ResolveThreadId(params: params, endpointId: endpointId, turnId: turnId)
    let status = CompletedStatusFromServerValue(turn["status"] as? String)
    let fromSnapshot = params["fromSnapshot"] as? Bool ?? false
    if fromSnapshot {
      turnStore.MarkTurnCompletedIfPresent(
        endpointId: endpointId,
        threadId: threadId,
        turnId: turnId,
        status: status,
        at: Date()
      )
      turnStore.UpdateTurnMetadata(
        endpointId: endpointId, threadId: threadId, turnId: turnId, turn: turn, at: Date())
      return
    }
    turnStore.MarkTurnCompleted(
      endpointId: endpointId,
      threadId: threadId,
      turnId: turnId,
      status: status,
      at: Date()
    )
    turnStore.UpdateTurnMetadata(
      endpointId: endpointId, threadId: threadId, turnId: turnId, turn: turn, at: Date())
  }

  private func HandleThreadSnapshot(params: [String: Any]) {
    guard
      let endpointId = params["endpointId"] as? String,
      let thread = params["thread"] as? [String: Any]
    else {
      return
    }
    turnStore.ApplyThreadSnapshot(endpointId: endpointId, thread: thread, at: Date())
  }

  private func HandleThreadSnapshotSummary(params: [String: Any]) {
    guard let endpointId = params["endpointId"] as? String else {
      return
    }

    let activeTurnKeys = params["activeTurnKeys"] as? [String] ?? []
    turnStore.ReconcileSnapshotActiveTurns(
      endpointId: endpointId,
      activeTurnKeys: activeTurnKeys,
      at: Date()
    )
  }

  private func HandleTurnProgressTrace(params: [String: Any]) {
    let endpointId = params["endpointId"] as? String ?? "unknown"

    guard
      let turnId = StringValue(params["turnId"]) ?? StringValue(params["turn_id"]),
      let categoryRaw = params["category"] as? String,
      let stateRaw = params["state"] as? String,
      let category = ProgressCategory(rawValue: categoryRaw),
      let state = ProgressState(rawValue: stateRaw)
    else {
      return
    }
    let threadId = ResolveThreadId(params: params, endpointId: endpointId, turnId: turnId)

    let label = params["label"] as? String
    turnStore.RecordProgress(
      endpointId: endpointId,
      threadId: threadId,
      turnId: turnId,
      category: category,
      state: state,
      label: label,
      at: Date()
    )
  }

  private func HandleItemLifecycle(params: [String: Any], state: ProgressState) {
    let endpointId = params["endpointId"] as? String ?? "unknown"

    guard
      let turnId = StringValue(params["turnId"]) ?? StringValue(params["turn_id"]),
      let item = params["item"] as? [String: Any],
      let itemType = item["type"] as? String
    else {
      return
    }
    let threadId = ResolveThreadId(params: params, endpointId: endpointId, turnId: turnId)

    turnStore.ApplyItemMetadata(
      endpointId: endpointId,
      threadId: threadId,
      turnId: turnId,
      item: item,
      at: Date()
    )

    ExtractItemDetails(
      endpointId: endpointId, turnId: turnId, item: item, itemType: itemType)

    guard let category = CategoryFromItemType(itemType) else {
      return
    }

    turnStore.RecordProgress(
      endpointId: endpointId,
      threadId: threadId,
      turnId: turnId,
      category: category,
      state: state,
      label: nil,
      at: Date()
    )
  }

  private func ExtractItemDetails(
    endpointId: String, turnId: String, item: [String: Any], itemType: String
  ) {
    switch itemType {
    case "commandExecution":
      let command = StringValue(item["command"]) ?? "unknown"
      let statusStr = (item["status"] as? String) ?? "inProgress"
      let exitCode = item["exitCode"] as? Int ?? item["exit_code"] as? Int
      let durationMs = item["durationMs"] as? Int ?? item["duration_ms"] as? Int
      turnStore.RecordCommand(
        endpointId: endpointId,
        turnId: turnId,
        command: CommandSummary(
          command: command,
          status: CommandExecutionState(serverValue: statusStr),
          exitCode: exitCode,
          durationMs: durationMs
        )
      )
    case "fileChange":
      guard let changes = item["changes"] as? [[String: Any]] else { return }
      for change in changes {
        guard let path = StringValue(change["path"]) else { continue }
        let kindStr: String
        if let kindDict = change["kind"] as? [String: Any], let type = kindDict["type"] as? String {
          kindStr = type
        } else if let kind = change["kind"] as? String {
          kindStr = kind
        } else {
          kindStr = "Update"
        }
        turnStore.RecordFileChange(
          endpointId: endpointId,
          turnId: turnId,
          change: FileChangeSummary(path: path, kind: FileChangeKind(serverValue: kindStr))
        )
      }
    default:
      break
    }
  }

  private func HandleThreadStarted(params: [String: Any]) {
    guard let endpointId = params["endpointId"] as? String else { return }
    guard let thread = params["thread"] as? [String: Any] else { return }

    if let gitInfoDict = thread["gitInfo"] as? [String: Any] {
      let branch = StringValue(gitInfoDict["branch"])
      let sha = StringValue(gitInfoDict["sha"])
      if branch != nil || sha != nil {
        turnStore.UpdateGitInfo(
          endpointId: endpointId, gitInfo: GitInfo(branch: branch, sha: sha))
      }
    }

    if let source = StringValue(thread["source"]) {
      turnStore.UpdateSessionSource(endpointId: endpointId, source: source)
    }

    turnStore.ApplyThreadSnapshot(endpointId: endpointId, thread: thread, at: Date())
  }

  private func HandleTokenUsageUpdated(params: [String: Any]) {
    guard let endpointId = params["endpointId"] as? String else { return }
    guard let usage = params["tokenUsage"] as? [String: Any] else { return }
    let threadId = StringValue(params["threadId"]) ?? StringValue(params["thread_id"])
    let turnId = StringValue(params["turnId"]) ?? StringValue(params["turn_id"])

    var info = TokenUsageInfo()

    if let total = usage["total"] as? [String: Any] {
      info.totalTokens = total["totalTokens"] as? Int ?? total["total_tokens"] as? Int ?? 0
      info.inputTokens = total["inputTokens"] as? Int ?? total["input_tokens"] as? Int ?? 0
      info.cachedInputTokens =
        total["cachedInputTokens"] as? Int ?? total["cached_input_tokens"] as? Int ?? 0
      info.outputTokens = total["outputTokens"] as? Int ?? total["output_tokens"] as? Int ?? 0
      info.reasoningTokens =
        total["reasoningOutputTokens"] as? Int ?? total["reasoning_output_tokens"] as? Int ?? 0
    }

    info.contextWindow =
      usage["modelContextWindow"] as? Int ?? usage["model_context_window"] as? Int

    turnStore.UpdateTokenUsage(
      endpointId: endpointId, threadId: threadId, turnId: turnId, tokenUsage: info)
  }

  private func HandleTurnPlanUpdated(params: [String: Any]) {
    let endpointId = params["endpointId"] as? String ?? "unknown"
    guard let turnId = StringValue(params["turnId"]) ?? StringValue(params["turn_id"]) else {
      return
    }

    let explanation = StringValue(params["explanation"])
    var steps: [PlanStepInfo] = []

    if let planArray = params["plan"] as? [[String: Any]] {
      for step in planArray {
        let desc =
          StringValue(step["description"]) ?? StringValue(step["title"]) ?? "Unknown step"
        let statusStr = (step["status"] as? String) ?? "pending"
        steps.append(PlanStepInfo(description: desc, status: PlanStepStatus(serverValue: statusStr)))
      }
    }

    turnStore.UpdatePlan(endpointId: endpointId, turnId: turnId, steps: steps, explanation: explanation)
  }

  private func HandleError(params: [String: Any]) {
    let endpointId = params["endpointId"] as? String ?? "unknown"

    guard let errorDict = params["error"] as? [String: Any] else { return }
    let message = StringValue(errorDict["message"]) ?? "Unknown error"
    let details = StringValue(errorDict["additionalDetails"])
      ?? StringValue(errorDict["additional_details"])
    let willRetry = params["willRetry"] as? Bool ?? params["will_retry"] as? Bool ?? false

    turnStore.RecordError(
      endpointId: endpointId,
      error: ErrorInfo(message: message, details: details, willRetry: willRetry, occurredAt: Date())
    )
  }

  private func HandleRateLimitsUpdated(params: [String: Any]) {
    guard let rateLimitsDict = params["rateLimits"] as? [String: Any] else { return }

    var info = RateLimitInfo()
    info.remaining = rateLimitsDict["remaining"] as? Int
    info.limit = rateLimitsDict["limit"] as? Int

    if let resetsAtRaw = rateLimitsDict["resetsAt"] as? Int
      ?? rateLimitsDict["resets_at"] as? Int
    {
      info.resetsAt = Date(timeIntervalSince1970: TimeInterval(resetsAtRaw))
    }

    turnStore.UpdateRateLimits(rateLimits: info)
  }

  private func CategoryFromItemType(_ itemType: String) -> ProgressCategory? {
    switch itemType {
    case "commandExecution", "mcpToolCall", "collabToolCall", "webSearch", "imageView":
      return .tool
    case "fileChange":
      return .edit
    case "reasoning":
      return .reasoning
    case "agentMessage":
      return .gen
    case "contextCompaction":
      return .waiting
    default:
      return nil
    }
  }

  private func CompletedStatusFromServerValue(_ serverValue: String?) -> TurnExecutionStatus {
    guard let serverValue else {
      return .completed
    }
    let parsed = TurnExecutionStatus(serverValue: serverValue)
    if parsed == .inProgress {
      return .completed
    }
    return parsed
  }

  private func ResolveThreadId(
    params: [String: Any],
    endpointId: String,
    turnId: String
  ) -> String? {
    if let threadId = StringValue(params["threadId"]) ?? StringValue(params["thread_id"]) {
      return threadId
    }

    return turnStore.ResolveThreadId(endpointId: endpointId, turnId: turnId)
  }

  private func StringValue(_ value: Any?) -> String? {
    guard let value = value as? String else {
      return nil
    }
    let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
    return trimmed.isEmpty ? nil : trimmed
  }
}
