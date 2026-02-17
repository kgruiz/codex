import AppKit
import Foundation

final class AppDelegate: NSObject, NSApplicationDelegate {
  private let turnStore = TurnStore()
  private let statusMenu = StatusMenuController()
  private let appServerClient = AppServerClient()

  private var connectionState: AppServerConnectionState = .disconnected
  private var activeEndpointIds: [String] = []
  private var animationFrame = 0
  private var timer: Timer?

  func applicationDidFinishLaunching(_ notification: Notification) {
    NSApplication.shared.setActivationPolicy(.accessory)
    ConfigureStatusMenu()
    ConfigureClient()
    StartTimer()
    appServerClient.Start()
    Render()
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
    statusMenu.QuitHandler = {
      NSApplication.shared.terminate(nil)
    }
  }

  private func ConfigureClient() {
    appServerClient.OnStateChange = { [weak self] state in
      guard let self else {
        return
      }
      self.connectionState = state
      self.Render()
    }

    appServerClient.OnEndpointIdsChanged = { [weak self] endpointIds in
      guard let self else {
        return
      }
      self.activeEndpointIds = endpointIds
      self.Render()
    }

    appServerClient.OnNotification = { [weak self] method, params in
      guard let self else {
        return
      }
      self.HandleNotification(method: method, params: params)
      self.Render()
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
    animationFrame += 1
    turnStore.Tick(now: Date())
    Render()
  }

  private func Render() {
    statusMenu.Render(
      endpointRows: turnStore.EndpointRows(activeEndpointIds: activeEndpointIds),
      connectionState: connectionState,
      animationFrame: animationFrame,
      now: Date()
    )
  }

  private func HandleNotification(method: String, params: [String: Any]) {
    switch method {
    case "turn/started":
      HandleTurnStarted(params: params)
    case "turn/completed":
      HandleTurnCompleted(params: params)
    case "turn/progressTrace":
      HandleTurnProgressTrace(params: params)
    case "item/started":
      HandleItemLifecycle(params: params, state: .started)
    case "item/completed":
      HandleItemLifecycle(params: params, state: .completed)
    default:
      break
    }
  }

  private func HandleTurnStarted(params: [String: Any]) {
    guard
      let threadId = params["threadId"] as? String,
      let turn = params["turn"] as? [String: Any],
      let turnId = turn["id"] as? String
    else {
      return
    }
    let endpointId = params["endpointId"] as? String ?? "unknown"
    turnStore.UpsertTurnStarted(
      endpointId: endpointId, threadId: threadId, turnId: turnId, at: Date())
  }

  private func HandleTurnCompleted(params: [String: Any]) {
    guard
      let threadId = params["threadId"] as? String,
      let turn = params["turn"] as? [String: Any],
      let turnId = turn["id"] as? String
    else {
      return
    }
    let status = TurnExecutionStatus(serverValue: turn["status"] as? String ?? "failed")
    let endpointId = params["endpointId"] as? String ?? "unknown"
    let fromSnapshot = params["fromSnapshot"] as? Bool ?? false
    if fromSnapshot {
      turnStore.MarkTurnCompletedIfPresent(
        endpointId: endpointId,
        threadId: threadId,
        turnId: turnId,
        status: status,
        at: Date()
      )
      return
    }
    turnStore.MarkTurnCompleted(
      endpointId: endpointId,
      threadId: threadId,
      turnId: turnId,
      status: status,
      at: Date()
    )
  }

  private func HandleTurnProgressTrace(params: [String: Any]) {
    guard
      let threadId = params["threadId"] as? String,
      let turnId = params["turnId"] as? String,
      let categoryRaw = params["category"] as? String,
      let stateRaw = params["state"] as? String,
      let category = ProgressCategory(rawValue: categoryRaw),
      let state = ProgressState(rawValue: stateRaw)
    else {
      return
    }

    let label = params["label"] as? String
    let endpointId = params["endpointId"] as? String ?? "unknown"
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
    guard
      let threadId = params["threadId"] as? String,
      let turnId = params["turnId"] as? String,
      let item = params["item"] as? [String: Any],
      let itemType = item["type"] as? String,
      let category = CategoryFromItemType(itemType)
    else {
      return
    }

    let endpointId = params["endpointId"] as? String ?? "unknown"
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
}
