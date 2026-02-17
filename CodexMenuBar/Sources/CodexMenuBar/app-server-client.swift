import Foundation

enum AppServerConnectionState: Equatable {
  case disconnected
  case connecting
  case connected
  case reconnecting
  case failed(String)
}

private struct RuntimeEndpoint {
  let endpointId: String
  let endpointUrl: URL
}

private final class EndpointConnection {
  let endpointId: String
  let endpointUrl: URL

  var IsConnected: Bool {
    isConnected
  }

  var OnNotification: ((String, [String: Any]) -> Void)?
  var OnConnected: (() -> Void)?
  var OnDisconnected: (() -> Void)?

  private let queue: DispatchQueue
  private let session: URLSession
  private var task: URLSessionWebSocketTask?
  private var isConnected = false
  private var nextRequestId = 1
  private var pendingResponses: [Int: ([String: Any]) -> Void] = [:]
  private var lastSnapshotRequestAt = Date.distantPast
  private let snapshotRefreshInterval: TimeInterval = 1.0

  init(endpointId: String, endpointUrl: URL, queue: DispatchQueue, session: URLSession) {
    self.endpointId = endpointId
    self.endpointUrl = endpointUrl
    self.queue = queue
    self.session = session
  }

  func Start() {
    guard task == nil else {
      return
    }

    let webSocketTask = session.webSocketTask(with: endpointUrl)
    task = webSocketTask
    webSocketTask.resume()

    isConnected = true
    OnConnected?()

    SendInitializeHandshakeOnQueue()
    RequestThreadSnapshotOnQueue()
    StartReceiveLoopOnQueue()
  }

  func Stop() {
    guard task != nil || isConnected else {
      return
    }
    DisconnectOnQueue(notify: true)
  }

  func RefreshSnapshotIfNeeded() {
    guard isConnected else {
      return
    }

    let now = Date()
    if now.timeIntervalSince(lastSnapshotRequestAt) < snapshotRefreshInterval {
      return
    }

    RequestThreadSnapshotOnQueue()
  }

  private func StartReceiveLoopOnQueue() {
    guard let webSocketTask = task else {
      return
    }

    webSocketTask.receive { [weak self] result in
      guard let self else {
        return
      }
      self.queue.async { [weak self] in
        guard let self else {
          return
        }
        self.HandleReceiveResultOnQueue(result)
      }
    }
  }

  private func HandleReceiveResultOnQueue(_ result: Result<URLSessionWebSocketTask.Message, Error>)
  {
    switch result {
    case .failure(let error):
      DisconnectOnQueue(notify: true)
      _ = error
    case .success(let message):
      switch message {
      case .string(let text):
        HandleIncomingTextOnQueue(text)
      case .data(let data):
        if let text = String(data: data, encoding: .utf8) {
          HandleIncomingTextOnQueue(text)
        }
      @unknown default:
        break
      }
      StartReceiveLoopOnQueue()
    }
  }

  private func HandleIncomingTextOnQueue(_ text: String) {
    guard
      let payload = text.data(using: .utf8),
      let object = try? JSONSerialization.jsonObject(with: payload),
      let dict = object as? [String: Any]
    else {
      return
    }

    if let method = dict["method"] as? String {
      let params = dict["params"] as? [String: Any] ?? [:]
      var augmented = params
      augmented["endpointId"] = endpointId
      OnNotification?(method, augmented)
      return
    }

    let responseId = ResponseIdFrom(dict)
    guard let id = responseId else {
      return
    }
    guard let result = dict["result"] as? [String: Any] else {
      pendingResponses.removeValue(forKey: id)
      return
    }
    let handler = pendingResponses.removeValue(forKey: id)
    handler?(result)
  }

  private func ResponseIdFrom(_ dict: [String: Any]) -> Int? {
    if let intId = dict["id"] as? Int {
      return intId
    }
    if let stringId = dict["id"] as? String {
      return Int(stringId)
    }
    return nil
  }

  private func SendInitializeHandshakeOnQueue() {
    let initialize: [String: Any] = [
      "id": 0,
      "method": "initialize",
      "params": [
        "clientInfo": [
          "name": "codex_menu_bar",
          "title": "Codex Menu Bar",
          "version": "0.1.0",
        ],
        "capabilities": [
          "experimentalApi": true
        ],
      ],
    ]
    SendObjectOnQueue(initialize)

    let initialized: [String: Any] = [
      "method": "initialized"
    ]
    SendObjectOnQueue(initialized)
  }

  private func RequestThreadSnapshotOnQueue() {
    lastSnapshotRequestAt = Date()
    SendRequestOnQueue(method: "thread/loaded/list", params: nil) { [weak self] result in
      guard let self else {
        return
      }
      guard let threadIds = result["data"] as? [String] else {
        return
      }
      for threadId in threadIds {
        self.RequestThreadReadOnQueue(threadId: threadId)
      }
    }
  }

  private func RequestThreadReadOnQueue(threadId: String) {
    let params: [String: Any] = [
      "threadId": threadId,
      "includeTurns": true,
    ]
    SendRequestOnQueue(method: "thread/read", params: params) { [weak self] result in
      guard let self else {
        return
      }
      guard
        let thread = result["thread"] as? [String: Any],
        let resolvedThreadId = thread["id"] as? String,
        let turns = thread["turns"] as? [[String: Any]]
      else {
        return
      }

      for turn in turns {
        guard turn["id"] is String else {
          continue
        }
        let status = turn["status"] as? String ?? "inProgress"
        if status == "inProgress" || status == "in_progress" {
          var params: [String: Any] = [
            "threadId": resolvedThreadId,
            "turn": turn,
          ]
          params["endpointId"] = self.endpointId
          params["fromSnapshot"] = true
          self.OnNotification?("turn/started", params)
        } else {
          var params: [String: Any] = [
            "threadId": resolvedThreadId,
            "turn": turn,
          ]
          params["endpointId"] = self.endpointId
          params["fromSnapshot"] = true
          self.OnNotification?("turn/completed", params)
        }
      }
    }
  }

  private func SendRequestOnQueue(
    method: String,
    params: [String: Any]?,
    onResult: (([String: Any]) -> Void)?
  ) {
    let requestId = nextRequestId
    nextRequestId += 1

    if let onResult {
      pendingResponses[requestId] = onResult
    }

    var request: [String: Any] = [
      "id": requestId,
      "method": method,
    ]
    if let params {
      request["params"] = params
    }

    SendObjectOnQueue(request)
  }

  private func SendObjectOnQueue(_ object: [String: Any]) {
    guard let webSocketTask = task else {
      return
    }
    guard
      let payload = try? JSONSerialization.data(withJSONObject: object),
      let text = String(data: payload, encoding: .utf8)
    else {
      return
    }

    webSocketTask.send(.string(text)) { [weak self] error in
      guard let self else {
        return
      }
      if error == nil {
        return
      }
      self.queue.async { [weak self] in
        self?.DisconnectOnQueue(notify: true)
      }
    }
  }

  private func DisconnectOnQueue(notify: Bool) {
    task?.cancel(with: .goingAway, reason: nil)
    task = nil
    pendingResponses.removeAll()

    let wasConnected = isConnected
    isConnected = false

    if notify && wasConnected {
      OnDisconnected?()
    }
  }
}

final class AppServerClient {
  var OnNotification: ((String, [String: Any]) -> Void)?
  var OnStateChange: ((AppServerConnectionState) -> Void)?
  var OnEndpointIdsChanged: (([String]) -> Void)?

  private let workQueue = DispatchQueue(label: "com.openai.codex.menubar.appserver")
  private let session: URLSession
  private var endpointConnections: [String: EndpointConnection] = [:]
  private var endpointScanTimer: DispatchSourceTimer?
  private var shouldRun = false
  private var state: AppServerConnectionState = .disconnected

  init() {
    let config = URLSessionConfiguration.default
    config.timeoutIntervalForRequest = 30
    config.timeoutIntervalForResource = 30
    session = URLSession(configuration: config)
  }

  deinit {
    session.invalidateAndCancel()
  }

  func Start() {
    workQueue.async { [weak self] in
      self?.StartOnQueue(initialState: .connecting)
    }
  }

  func Restart() {
    workQueue.async { [weak self] in
      guard let self else {
        return
      }
      self.StopOnQueue(emitState: false)
      self.StartOnQueue(initialState: .reconnecting)
    }
  }

  func Stop() {
    workQueue.async { [weak self] in
      self?.StopOnQueue(emitState: true)
    }
  }

  private func StartOnQueue(initialState: AppServerConnectionState) {
    shouldRun = true
    EmitState(initialState)
    StartScanTimerOnQueue()
    ScanEndpointsOnQueue()
  }

  private func StopOnQueue(emitState: Bool) {
    shouldRun = false

    endpointScanTimer?.cancel()
    endpointScanTimer = nil

    let existing = endpointConnections
    endpointConnections.removeAll()
    for connection in existing.values {
      connection.Stop()
    }

    DispatchEndpointIds([])

    if emitState {
      EmitState(.disconnected)
    }
  }

  private func StartScanTimerOnQueue() {
    endpointScanTimer?.cancel()

    let timer = DispatchSource.makeTimerSource(queue: workQueue)
    timer.schedule(deadline: .now(), repeating: 0.5)
    timer.setEventHandler { [weak self] in
      self?.ScanEndpointsOnQueue()
    }
    endpointScanTimer = timer
    timer.resume()
  }

  private func ScanEndpointsOnQueue() {
    guard shouldRun else {
      return
    }

    let endpoints = ReadRuntimeEndpointsOnQueue()
    ReconcileEndpointsOnQueue(endpoints)
    RefreshStateOnQueue()
  }

  private func ReadRuntimeEndpointsOnQueue() -> [String: RuntimeEndpoint] {
    let endpointDir = URL(fileURLWithPath: NSHomeDirectory())
      .appendingPathComponent(".codex")
      .appendingPathComponent("runtime")
      .appendingPathComponent("menubar")
      .appendingPathComponent("endpoints")

    guard
      let fileUrls = try? FileManager.default.contentsOfDirectory(
        at: endpointDir,
        includingPropertiesForKeys: nil,
        options: [.skipsHiddenFiles]
      )
    else {
      return [:]
    }

    var endpoints: [String: RuntimeEndpoint] = [:]

    for fileUrl in fileUrls where fileUrl.pathExtension == "json" {
      guard let payload = try? Data(contentsOf: fileUrl) else {
        continue
      }
      guard
        let object = try? JSONSerialization.jsonObject(with: payload),
        let dict = object as? [String: Any],
        let endpointUrlRaw = dict["endpointUrl"] as? String,
        let endpointUrl = URL(string: endpointUrlRaw)
      else {
        continue
      }

      let endpointId = fileUrl.deletingPathExtension().lastPathComponent
      endpoints[endpointId] = RuntimeEndpoint(endpointId: endpointId, endpointUrl: endpointUrl)
    }

    return endpoints
  }

  private func ReconcileEndpointsOnQueue(_ endpoints: [String: RuntimeEndpoint]) {
    let existingIds = Set(endpointConnections.keys)
    let discoveredIds = Set(endpoints.keys)

    let removedIds = existingIds.subtracting(discoveredIds)
    for endpointId in removedIds {
      let connection = endpointConnections.removeValue(forKey: endpointId)
      connection?.Stop()
    }

    for endpoint in endpoints.values {
      if let existing = endpointConnections[endpoint.endpointId] {
        if existing.endpointUrl == endpoint.endpointUrl {
          existing.RefreshSnapshotIfNeeded()
          continue
        }
        existing.Stop()
        endpointConnections.removeValue(forKey: endpoint.endpointId)
      }

      let connection = EndpointConnection(
        endpointId: endpoint.endpointId,
        endpointUrl: endpoint.endpointUrl,
        queue: workQueue,
        session: session
      )
      connection.OnNotification = { [weak self] method, params in
        self?.DispatchNotification(method: method, params: params)
      }
      connection.OnConnected = { [weak self] in
        self?.workQueue.async { [weak self] in
          self?.RefreshStateOnQueue()
        }
      }
      connection.OnDisconnected = { [weak self] in
        self?.workQueue.async { [weak self] in
          self?.RefreshStateOnQueue()
        }
      }
      endpointConnections[endpoint.endpointId] = connection
      connection.Start()
    }

    let endpointIds = endpointConnections.keys.sorted()
    DispatchEndpointIds(endpointIds)
  }

  private func DispatchNotification(method: String, params: [String: Any]) {
    DispatchQueue.main.async { [weak self] in
      self?.OnNotification?(method, params)
    }
  }

  private func RefreshStateOnQueue() {
    guard shouldRun else {
      EmitState(.disconnected)
      return
    }

    let connectedCount = endpointConnections.values.filter { $0.IsConnected }.count
    if connectedCount > 0 {
      EmitState(.connected)
      return
    }

    if endpointConnections.isEmpty {
      EmitState(.connecting)
    } else {
      EmitState(.reconnecting)
    }
  }

  private func EmitState(_ nextState: AppServerConnectionState) {
    if state == nextState {
      return
    }
    state = nextState
    DispatchQueue.main.async { [weak self] in
      self?.OnStateChange?(nextState)
    }
  }

  private func DispatchEndpointIds(_ endpointIds: [String]) {
    DispatchQueue.main.async { [weak self] in
      self?.OnEndpointIdsChanged?(endpointIds)
    }
  }
}
