import Darwin
import Foundation

enum AppServerConnectionState: Equatable {
  case disconnected
  case connecting
  case connected
  case reconnecting
  case failed(String)
}

final class AppServerClient {
  var OnNotification: ((String, [String: Any]) -> Void)?
  var OnStateChange: ((AppServerConnectionState) -> Void)?
  var OnEndpointIdsChanged: (([String]) -> Void)?

  private let workQueue = DispatchQueue(label: "com.openai.codex.menubar.codexd")

  private var socketFD: Int32 = -1
  private var readSource: DispatchSourceRead?
  private var socketReadBuffer = Data()

  private var reconnectTimer: DispatchSourceTimer?
  private var shouldRun = false
  private var hasConnectedOnce = false
  private var state: AppServerConnectionState = .disconnected

  private var nextRequestId = 1
  private var pendingResponses: [Int: ([String: Any]) -> Void] = [:]

  private var lastSeq: Int?
  private var knownEndpointIds = Set<String>()
  private var summaryKnownEndpointIds = Set<String>()
  private var lastDispatchedEndpointIds: [String] = []

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
    _ = endpointId
    Restart()
  }

  func Stop() {
    workQueue.async { [weak self] in
      self?.StopOnQueue(emitState: true)
    }
  }

  private func StartOnQueue(initialState: AppServerConnectionState) {
    shouldRun = true
    EmitState(initialState)
    StartReconnectTimerOnQueue()
    ConnectIfNeededOnQueue()
  }

  private func StopOnQueue(emitState: Bool) {
    shouldRun = false

    reconnectTimer?.cancel()
    reconnectTimer = nil

    DisconnectOnQueue(notify: false)

    knownEndpointIds.removeAll()
    summaryKnownEndpointIds.removeAll()
    DispatchEndpointIds([])

    if emitState {
      EmitState(.disconnected)
    }
  }

  private func StartReconnectTimerOnQueue() {
    reconnectTimer?.cancel()

    let timer = DispatchSource.makeTimerSource(queue: workQueue)
    timer.schedule(deadline: .now() + 1.0, repeating: 1.0)
    timer.setEventHandler { [weak self] in
      self?.ConnectIfNeededOnQueue()
    }
    reconnectTimer = timer
    timer.resume()
  }

  private func ConnectIfNeededOnQueue() {
    guard shouldRun else {
      return
    }

    guard socketFD < 0 else {
      return
    }

    let socketPath = CodexdSocketPath()
    guard let socketAddress = SocketAddress(path: socketPath) else {
      EmitState(.failed("Invalid codexd socket path"))
      return
    }

    let fd = socket(AF_UNIX, SOCK_STREAM, 0)
    if fd < 0 {
      EmitState(.failed("Unable to allocate Unix socket"))
      return
    }

    let connectResult = withUnsafePointer(to: socketAddress) { pointer in
      pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) { sockaddrPointer in
        connect(fd, sockaddrPointer, socklen_t(MemoryLayout<sockaddr_un>.stride))
      }
    }

    if connectResult != 0 {
      close(fd)
      if hasConnectedOnce {
        EmitState(.reconnecting)
      } else {
        EmitState(.connecting)
      }
      return
    }

    socketFD = fd
    socketReadBuffer.removeAll(keepingCapacity: false)

    let source = DispatchSource.makeReadSource(fileDescriptor: fd, queue: workQueue)
    source.setEventHandler { [weak self] in
      self?.HandleSocketReadableOnQueue()
    }
    source.setCancelHandler {
      close(fd)
    }
    source.resume()
    readSource = source

    hasConnectedOnce = true
    EmitState(.connected)
    RequestSnapshotOnQueue()
  }

  private func RequestSnapshotOnQueue() {
    SendRequestOnQueue(method: "codexd/snapshot", params: [:]) { [weak self] result in
      guard let self else {
        return
      }
      self.HandleSnapshotOnQueue(result)
    }
  }

  private func SubscribeAfterSnapshotOnQueue() {
    var params: [String: Any] = [:]
    if let lastSeq {
      params["afterSeq"] = lastSeq
    }

    SendRequestOnQueue(method: "codexd/subscribe", params: params) { [weak self] result in
      guard let self else {
        return
      }

      if let seq = IntValue(result["seq"]) {
        self.lastSeq = seq
      }
    }
  }

  private func HandleSnapshotOnQueue(_ result: [String: Any]) {
    if let seq = IntValue(result["seq"]) {
      lastSeq = seq
    }

    let runtimes = result["runtimes"] as? [[String: Any]] ?? []
    var endpointIds = Set<String>()
    var activeTurnKeysByEndpoint: [String: [String]] = [:]

    for runtime in runtimes {
      guard let runtimeId = NonEmptyString(runtime["runtimeId"]) ?? NonEmptyString(runtime["runtime_id"]) else {
        continue
      }

      endpointIds.insert(runtimeId)

      var metadataParams: [String: Any] = ["endpointId": runtimeId]
      if let cwd = runtime["cwd"] as? String {
        metadataParams["cwd"] = cwd
      }
      if let sessionSource = runtime["sessionSource"] as? String ?? runtime["session_source"] as? String {
        metadataParams["sessionSource"] = sessionSource
      }
      DispatchNotification(method: "runtime/metadata", params: metadataParams)

      let activeTurns = runtime["activeTurns"] as? [[String: Any]] ?? []
      for activeTurn in activeTurns {
        guard
          let threadId = NonEmptyString(activeTurn["threadId"]),
          let turnId = NonEmptyString(activeTurn["turnId"])
        else {
          continue
        }

        activeTurnKeysByEndpoint[runtimeId, default: []].append("\(runtimeId):\(turnId)")

        let params: [String: Any] = [
          "threadId": threadId,
          "turn": [
            "id": turnId,
            "status": "inProgress",
          ],
          "endpointId": runtimeId,
          "fromSnapshot": true,
        ]
        DispatchNotification(method: "turn/started", params: params)
      }
    }

    EmitSnapshotSummariesOnQueue(
      endpointIds: endpointIds,
      activeTurnKeysByEndpoint: activeTurnKeysByEndpoint
    )

    knownEndpointIds = endpointIds
    DispatchEndpointIds(Array(endpointIds).sorted())
    SubscribeAfterSnapshotOnQueue()
  }

  private func EmitSnapshotSummariesOnQueue(
    endpointIds: Set<String>,
    activeTurnKeysByEndpoint: [String: [String]]
  ) {
    let summaryEndpointIds = endpointIds.union(summaryKnownEndpointIds)

    for endpointId in summaryEndpointIds {
      let activeTurnKeys = (activeTurnKeysByEndpoint[endpointId] ?? []).sorted()
      DispatchNotification(
        method: "thread/snapshotSummary",
        params: [
          "endpointId": endpointId,
          "activeTurnKeys": activeTurnKeys,
        ])
    }

    summaryKnownEndpointIds = endpointIds
  }

  private func HandleCodexdEventOnQueue(_ params: [String: Any]) {
    if let seq = IntValue(params["seq"]) {
      lastSeq = seq
    }

    guard let event = params["event"] as? [String: Any],
      let eventType = event["type"] as? String
    else {
      return
    }

    switch eventType {
    case "runtimeUpsert":
      guard let runtime = event["runtime"] as? [String: Any],
        let runtimeId = NonEmptyString(runtime["runtimeId"]) ?? NonEmptyString(runtime["runtime_id"])
      else {
        return
      }

      knownEndpointIds.insert(runtimeId)
      summaryKnownEndpointIds.insert(runtimeId)
      DispatchEndpointIds(Array(knownEndpointIds).sorted())

      var metadataParams: [String: Any] = ["endpointId": runtimeId]
      if let cwd = runtime["cwd"] as? String {
        metadataParams["cwd"] = cwd
      }
      if let sessionSource = runtime["sessionSource"] as? String ?? runtime["session_source"] as? String {
        metadataParams["sessionSource"] = sessionSource
      }
      DispatchNotification(method: "runtime/metadata", params: metadataParams)

    case "runtimeRemoved":
      guard let runtimeId = NonEmptyString(event["runtimeId"]) ?? NonEmptyString(event["runtime_id"]) else {
        return
      }

      knownEndpointIds.remove(runtimeId)
      summaryKnownEndpointIds.remove(runtimeId)

      DispatchNotification(
        method: "thread/snapshotSummary",
        params: [
          "endpointId": runtimeId,
          "activeTurnKeys": [],
        ])

      DispatchEndpointIds(Array(knownEndpointIds).sorted())

    case "runtimeNotification":
      guard
        let runtimeId = NonEmptyString(event["runtimeId"]) ?? NonEmptyString(event["runtime_id"]),
        let notification = event["notification"] as? [String: Any],
        let method = notification["method"] as? String
      else {
        return
      }

      knownEndpointIds.insert(runtimeId)
      summaryKnownEndpointIds.insert(runtimeId)
      DispatchEndpointIds(Array(knownEndpointIds).sorted())

      var notificationParams = notification["params"] as? [String: Any] ?? [:]
      notificationParams["endpointId"] = runtimeId
      DispatchNotification(method: method, params: notificationParams)

    default:
      break
    }
  }

  private func HandleSocketReadableOnQueue() {
    guard socketFD >= 0 else {
      return
    }

    var buffer = [UInt8](repeating: 0, count: 8192)

    while true {
      let bytesRead = recv(socketFD, &buffer, buffer.count, 0)

      if bytesRead > 0 {
        socketReadBuffer.append(buffer, count: Int(bytesRead))
        HandleBufferedMessagesOnQueue()
        continue
      }

      if bytesRead == 0 {
        DisconnectOnQueue(notify: true)
        return
      }

      if errno == EAGAIN || errno == EWOULDBLOCK {
        return
      }

      DisconnectOnQueue(notify: true)
      return
    }
  }

  private func HandleBufferedMessagesOnQueue() {
    let newlineData = Data([0x0A])

    while let lineBreak = socketReadBuffer.range(of: newlineData) {
      let lineData = socketReadBuffer.subdata(in: socketReadBuffer.startIndex..<lineBreak.lowerBound)
      socketReadBuffer.removeSubrange(socketReadBuffer.startIndex..<lineBreak.upperBound)

      if lineData.isEmpty {
        continue
      }

      guard let line = String(data: lineData, encoding: .utf8) else {
        continue
      }

      HandleIncomingTextOnQueue(line)
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
      if method == "codexd/event" {
        HandleCodexdEventOnQueue(params)
      } else {
        DispatchNotification(method: method, params: params)
      }
      return
    }

    guard let id = ResponseIdFrom(dict) else {
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

  private func SendRequestOnQueue(
    method: String,
    params: [String: Any],
    onResult: (([String: Any]) -> Void)?
  ) {
    guard socketFD >= 0 else {
      return
    }

    let requestId = nextRequestId
    nextRequestId += 1

    if let onResult {
      pendingResponses[requestId] = onResult
    }

    var request: [String: Any] = [
      "id": requestId,
      "method": method,
    ]

    request["params"] = params
    SendObjectOnQueue(request)
  }

  private func SendObjectOnQueue(_ object: [String: Any]) {
    guard
      let payload = try? JSONSerialization.data(withJSONObject: object),
      var text = String(data: payload, encoding: .utf8)
    else {
      return
    }

    guard socketFD >= 0 else {
      return
    }

    text.append("\n")
    guard let data = text.data(using: .utf8) else {
      return
    }

    let sendSucceeded = data.withUnsafeBytes { bytes -> Bool in
      guard let baseAddress = bytes.baseAddress else {
        return false
      }

      var offset = 0
      while offset < bytes.count {
        let sent = Darwin.send(socketFD, baseAddress.advanced(by: offset), bytes.count - offset, 0)
        if sent > 0 {
          offset += sent
          continue
        }

        if sent < 0, errno == EINTR {
          continue
        }

        return false
      }

      return true
    }

    if !sendSucceeded {
      DisconnectOnQueue(notify: true)
    }
  }

  private func DisconnectOnQueue(notify: Bool) {
    if let readSource {
      readSource.cancel()
      self.readSource = nil
      socketFD = -1
    } else if socketFD >= 0 {
      close(socketFD)
      socketFD = -1
    }

    socketReadBuffer.removeAll(keepingCapacity: false)
    pendingResponses.removeAll()

    if notify && shouldRun {
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
    if endpointIds == lastDispatchedEndpointIds {
      return
    }

    lastDispatchedEndpointIds = endpointIds
    DispatchQueue.main.async { [weak self] in
      self?.OnEndpointIdsChanged?(endpointIds)
    }
  }

  private func DispatchNotification(method: String, params: [String: Any]) {
    DispatchQueue.main.async { [weak self] in
      self?.OnNotification?(method, params)
    }
  }

  private func SocketAddress(path: String) -> sockaddr_un? {
    var address = sockaddr_un()
    address.sun_len = UInt8(MemoryLayout<sockaddr_un>.stride)
    address.sun_family = sa_family_t(AF_UNIX)

    let maxLength = MemoryLayout.size(ofValue: address.sun_path)
    let pathBytes = path.utf8CString

    if pathBytes.count > maxLength {
      return nil
    }

    _ = withUnsafeMutablePointer(to: &address.sun_path.0) { pointer in
      path.withCString { stringPointer in
        strncpy(pointer, stringPointer, maxLength - 1)
      }
    }

    return address
  }

  private func CodexdSocketPath() -> String {
    let codexHome = URL(fileURLWithPath: NSHomeDirectory()).appendingPathComponent(".codex")
    return codexHome
      .appendingPathComponent("runtime")
      .appendingPathComponent("codexd")
      .appendingPathComponent("codexd.sock")
      .path
  }

  private func NonEmptyString(_ value: Any?) -> String? {
    guard let value = value as? String else {
      return nil
    }

    let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
    return trimmed.isEmpty ? nil : trimmed
  }

  private func IntValue(_ value: Any?) -> Int? {
    if let intValue = value as? Int {
      return intValue
    }

    if let stringValue = value as? String {
      return Int(stringValue)
    }

    return nil
  }
}