import Darwin
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
  let pid: Int?
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
  private var isInitialized = false
  private var nextRequestId = 1
  private var pendingResponses: [Int: ([String: Any]) -> Void] = [:]
  private var lastSnapshotRequestAt = Date.distantPast
  private let snapshotRefreshInterval: TimeInterval = 0.5

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

    StartReceiveLoopOnQueue()
    SendInitializeHandshakeOnQueue()
  }

  func Stop() {
    guard task != nil || isConnected else {
      return
    }
    DisconnectOnQueue(notify: true)
  }

  func RefreshSnapshotIfNeeded() {
    guard isConnected, isInitialized else {
      return
    }

    let now = Date()
    if now.timeIntervalSince(lastSnapshotRequestAt) < snapshotRefreshInterval {
      return
    }

    RequestTurnActiveSnapshotOnQueue()
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
    let handler = pendingResponses.removeValue(forKey: id)
    if let result = dict["result"] as? [String: Any] {
      handler?(result)
      return
    }
    handler?([:])
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
    let params: [String: Any] = [
      "clientInfo": [
        "name": "codex_menu_bar",
        "title": "Codex Menu Bar",
        "version": "0.1.0",
      ],
      "capabilities": [
        "experimentalApi": true
      ],
    ]
    SendRequestOnQueue(method: "initialize", params: params) { [weak self] _ in
      guard let self else {
        return
      }
      self.isInitialized = true
      let initialized: [String: Any] = [
        "method": "initialized"
      ]
      self.SendObjectOnQueue(initialized)
      self.RequestTurnActiveSnapshotOnQueue()
    }
  }

  private func RequestTurnActiveSnapshotOnQueue() {
    lastSnapshotRequestAt = Date()
    SendRequestOnQueue(method: "turn/active", params: [:]) { [weak self] result in
      guard let self else {
        return
      }
      guard let activeTurns = result["data"] as? [[String: Any]] else {
        return
      }

      var activeTurnKeys: [String] = []
      var threadIdsToFetch: Set<String> = []
      for activeTurn in activeTurns {
        guard
          let threadId = activeTurn["threadId"] as? String,
          let turnId = activeTurn["turnId"] as? String
        else {
          continue
        }
        activeTurnKeys.append("\(threadId):\(turnId)")
        threadIdsToFetch.insert(threadId)

        var params: [String: Any] = [
          "threadId": threadId,
          "turn": [
            "id": turnId,
            "status": "inProgress",
          ],
        ]
        params["endpointId"] = self.endpointId
        params["fromSnapshot"] = true
        self.OnNotification?("turn/started", params)
      }

      self.EmitSnapshotSummaryOnQueue(activeTurnKeys: activeTurnKeys.sorted())

      for threadId in threadIdsToFetch {
        self.RequestThreadReadOnQueue(threadId: threadId)
      }

      if threadIdsToFetch.isEmpty {
        self.RequestLoadedThreadsOnQueue()
      }
    }
  }

  private func RequestThreadReadOnQueue(threadId: String) {
    let params: [String: Any] = [
      "threadId": threadId,
      "includeTurns": true,
    ]
    SendRequestOnQueue(method: "thread/read", params: params) { [weak self] result in
      guard let self else { return }
      guard let thread = result["thread"] as? [String: Any] else { return }
      let notifParams: [String: Any] = [
        "thread": thread,
        "endpointId": self.endpointId,
      ]
      self.OnNotification?("thread/snapshot", notifParams)
    }
  }

  private func RequestLoadedThreadsOnQueue() {
    SendRequestOnQueue(method: "thread/loaded/list", params: [:]) { [weak self] result in
      guard let self else { return }
      guard let threadIds = result["data"] as? [String], let firstId = threadIds.first else {
        return
      }
      self.RequestThreadReadOnQueue(threadId: firstId)
    }
  }

  private func EmitSnapshotSummaryOnQueue(activeTurnKeys: [String]) {
    var params: [String: Any] = [
      "activeTurnKeys": activeTurnKeys
    ]
    params["endpointId"] = endpointId
    OnNotification?("thread/snapshotSummary", params)
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
    isInitialized = false

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
  private var endpointResyncTimer: DispatchSourceTimer?
  private var endpointSnapshotTimer: DispatchSourceTimer?
  private var rootDirectoryWatcher: DispatchSourceFileSystemObject?
  private var endpointDirectoryWatcher: DispatchSourceFileSystemObject?
  private var rootDirectoryWatchPath: String?
  private var endpointDirectoryWatchPath: String?
  private let resyncIntervalSeconds: TimeInterval = 15.0
  private let endpointLeaseFreshnessSeconds: TimeInterval = 15.0
  private let endpointStaleFileTTLSeconds: TimeInterval = 30 * 60
  private var shouldRun = false
  private var state: AppServerConnectionState = .disconnected
  private var lastEndpointIds: [String] = []
  private let timestampFormatterWithFractionalSeconds = ISO8601DateFormatter()
  private let timestampFormatter = ISO8601DateFormatter()

  init() {
    let config = URLSessionConfiguration.default
    config.timeoutIntervalForRequest = 30
    config.timeoutIntervalForResource = 30
    session = URLSession(configuration: config)
    timestampFormatterWithFractionalSeconds.formatOptions = [
      .withInternetDateTime,
      .withFractionalSeconds,
    ]
    timestampFormatter.formatOptions = [.withInternetDateTime]
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

  func ReconnectEndpoint(_ endpointId: String) {
    workQueue.async { [weak self] in
      self?.ReconnectEndpointOnQueue(endpointId)
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
    StartResyncTimerOnQueue()
    StartSnapshotTimerOnQueue()
    RefreshDirectoryWatchersOnQueue()
    ScanEndpointsOnQueue()
  }

  private func ReconnectEndpointOnQueue(_ endpointId: String) {
    guard shouldRun else {
      return
    }

    let connection = endpointConnections.removeValue(forKey: endpointId)
    connection?.Stop()
    ScanEndpointsOnQueue()
  }

  private func StopOnQueue(emitState: Bool) {
    shouldRun = false

    endpointResyncTimer?.cancel()
    endpointResyncTimer = nil
    endpointSnapshotTimer?.cancel()
    endpointSnapshotTimer = nil
    CancelRootDirectoryWatcherOnQueue()
    CancelEndpointDirectoryWatcherOnQueue()

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

  private func StartResyncTimerOnQueue() {
    endpointResyncTimer?.cancel()

    let timer = DispatchSource.makeTimerSource(queue: workQueue)
    timer.schedule(deadline: .now() + resyncIntervalSeconds, repeating: resyncIntervalSeconds)
    timer.setEventHandler { [weak self] in
      self?.ScanEndpointsOnQueue()
    }
    endpointResyncTimer = timer
    timer.resume()
  }

  private func StartSnapshotTimerOnQueue() {
    endpointSnapshotTimer?.cancel()

    let timer = DispatchSource.makeTimerSource(queue: workQueue)
    timer.schedule(deadline: .now() + 0.5, repeating: 0.5)
    timer.setEventHandler { [weak self] in
      self?.RefreshSnapshotsOnQueue()
    }
    endpointSnapshotTimer = timer
    timer.resume()
  }

  private func RefreshSnapshotsOnQueue() {
    guard shouldRun else {
      return
    }
    for connection in endpointConnections.values {
      connection.RefreshSnapshotIfNeeded()
    }
  }

  private func ScanEndpointsOnQueue() {
    guard shouldRun else {
      return
    }

    RefreshDirectoryWatchersOnQueue()
    let endpoints = ReadRuntimeEndpointsOnQueue()
    ReconcileEndpointsOnQueue(endpoints)
    RefreshStateOnQueue()
  }

  private func RefreshDirectoryWatchersOnQueue() {
    RefreshRootDirectoryWatcherOnQueue()
    RefreshEndpointDirectoryWatcherOnQueue()
  }

  private func RefreshRootDirectoryWatcherOnQueue() {
    let nextWatchPath = ResolveRootDirectoryWatchPathOnQueue()
    if nextWatchPath == rootDirectoryWatchPath, rootDirectoryWatcher != nil {
      return
    }

    CancelRootDirectoryWatcherOnQueue()

    guard let nextWatchPath else {
      return
    }
    guard let source = MakeDirectoryWatcherSourceOnQueue(path: nextWatchPath) else {
      return
    }

    source.setEventHandler { [weak self] in
      guard let self else {
        return
      }
      self.ScanEndpointsOnQueue()
    }
    source.resume()
    rootDirectoryWatcher = source
    rootDirectoryWatchPath = nextWatchPath
  }

  private func RefreshEndpointDirectoryWatcherOnQueue() {
    let endpointDirectoryPath = EndpointDirectoryURL().path
    guard DirectoryExists(path: endpointDirectoryPath) else {
      CancelEndpointDirectoryWatcherOnQueue()
      return
    }
    if endpointDirectoryPath == endpointDirectoryWatchPath, endpointDirectoryWatcher != nil {
      return
    }

    CancelEndpointDirectoryWatcherOnQueue()

    guard let source = MakeDirectoryWatcherSourceOnQueue(path: endpointDirectoryPath) else {
      return
    }
    source.setEventHandler { [weak self] in
      guard let self else {
        return
      }
      self.ScanEndpointsOnQueue()
    }
    source.resume()
    endpointDirectoryWatcher = source
    endpointDirectoryWatchPath = endpointDirectoryPath
  }

  private func CancelRootDirectoryWatcherOnQueue() {
    rootDirectoryWatcher?.setEventHandler(handler: nil)
    rootDirectoryWatcher?.cancel()
    rootDirectoryWatcher = nil
    rootDirectoryWatchPath = nil
  }

  private func CancelEndpointDirectoryWatcherOnQueue() {
    endpointDirectoryWatcher?.setEventHandler(handler: nil)
    endpointDirectoryWatcher?.cancel()
    endpointDirectoryWatcher = nil
    endpointDirectoryWatchPath = nil
  }

  private func MakeDirectoryWatcherSourceOnQueue(path: String) -> DispatchSourceFileSystemObject? {
    let descriptor = open(path, O_EVTONLY)
    if descriptor < 0 {
      return nil
    }

    let source = DispatchSource.makeFileSystemObjectSource(
      fileDescriptor: descriptor,
      eventMask: [.write, .rename, .delete, .attrib],
      queue: workQueue
    )
    source.setCancelHandler {
      close(descriptor)
    }
    return source
  }

  private func ResolveRootDirectoryWatchPathOnQueue() -> String? {
    let candidates = [
      MenubarRuntimeDirectoryURL().path,
      RuntimeDirectoryURL().path,
      CodexHomeDirectoryURL().path,
      URL(fileURLWithPath: NSHomeDirectory()).path,
    ]
    for path in candidates where DirectoryExists(path: path) {
      return path
    }
    return nil
  }

  private func CodexHomeDirectoryURL() -> URL {
    URL(fileURLWithPath: NSHomeDirectory()).appendingPathComponent(".codex")
  }

  private func RuntimeDirectoryURL() -> URL {
    CodexHomeDirectoryURL().appendingPathComponent("runtime")
  }

  private func MenubarRuntimeDirectoryURL() -> URL {
    RuntimeDirectoryURL().appendingPathComponent("menubar")
  }

  private func EndpointDirectoryURL() -> URL {
    MenubarRuntimeDirectoryURL().appendingPathComponent("endpoints")
  }

  private func DirectoryExists(path: String) -> Bool {
    var isDirectory: ObjCBool = false
    let exists = FileManager.default.fileExists(atPath: path, isDirectory: &isDirectory)
    return exists && isDirectory.boolValue
  }

  private func ReadRuntimeEndpointsOnQueue() -> [String: RuntimeEndpoint] {
    let endpointDir = EndpointDirectoryURL()
    let now = Date()

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
      let pid =
        (dict["pid"] as? Int)
        ?? ((dict["pid"] as? String).flatMap { Int($0) })
        ?? Int(endpointId)
      let lastHeartbeatAt = ParseTimestamp(dict["lastHeartbeatAt"])
      let startedAt = ParseTimestamp(dict["startedAt"])
      let isPidAlive = pid.map(IsProcessAlive) ?? false
      let isFreshLease =
        lastHeartbeatAt.map { now.timeIntervalSince($0) <= endpointLeaseFreshnessSeconds } ?? true
      let isActiveEndpoint = isPidAlive && isFreshLease

      let shouldDeleteForDeadPid = pid != nil && !isPidAlive
      let shouldDeleteForExpiredLease =
        lastHeartbeatAt.map { now.timeIntervalSince($0) > endpointStaleFileTTLSeconds } ?? false
      let shouldDeleteForAncientFile =
        lastHeartbeatAt == nil
        && startedAt.map { now.timeIntervalSince($0) > endpointStaleFileTTLSeconds } ?? false
        && (pid == nil || !isPidAlive)
      if shouldDeleteForDeadPid || shouldDeleteForExpiredLease || shouldDeleteForAncientFile {
        DeleteEndpointFileIfPresent(fileUrl)
      }

      if !isActiveEndpoint {
        continue
      }
      endpoints[endpointId] = RuntimeEndpoint(
        endpointId: endpointId,
        endpointUrl: endpointUrl,
        pid: pid
      )
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
          guard let self else {
            return
          }
          self.RefreshStateOnQueue()
          self.DispatchEndpointIds(self.ConnectedEndpointIdsOnQueue())
        }
      }
      connection.OnDisconnected = { [weak self] in
        self?.workQueue.async { [weak self] in
          guard let self else {
            return
          }
          self.RefreshStateOnQueue()
          self.DispatchEndpointIds(self.ConnectedEndpointIdsOnQueue())
        }
      }
      endpointConnections[endpoint.endpointId] = connection
      connection.Start()
    }

    let endpointIds = ConnectedEndpointIdsOnQueue()
    DispatchEndpointIds(endpointIds)
  }

  private func ConnectedEndpointIdsOnQueue() -> [String] {
    endpointConnections
      .filter { _, connection in connection.IsConnected }
      .map(\.key)
      .sorted()
  }

  private func IsProcessAlive(_ pid: Int) -> Bool {
    if pid <= 0 {
      return false
    }

    let result = kill(pid_t(pid), 0)
    if result == 0 {
      return true
    }

    if errno == EPERM {
      return true
    }

    return false
  }

  private func ParseTimestamp(_ rawValue: Any?) -> Date? {
    guard let rawValue = rawValue as? String, !rawValue.isEmpty else {
      return nil
    }
    if let parsed = timestampFormatterWithFractionalSeconds.date(from: rawValue) {
      return parsed
    }
    return timestampFormatter.date(from: rawValue)
  }

  private func DeleteEndpointFileIfPresent(_ fileUrl: URL) {
    do {
      try FileManager.default.removeItem(at: fileUrl)
    } catch {
      let nsError = error as NSError
      if nsError.domain == NSCocoaErrorDomain && nsError.code == NSFileNoSuchFileError {
        return
      }
    }
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
    if endpointIds == lastEndpointIds {
      return
    }
    lastEndpointIds = endpointIds
    DispatchQueue.main.async { [weak self] in
      self?.OnEndpointIdsChanged?(endpointIds)
    }
  }
}
