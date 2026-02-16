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

    private let workQueue = DispatchQueue(label: "com.openai.codex.menubar.appserver")
    private var process: Process?
    private var stdinHandle: FileHandle?
    private var outputBuffer = Data()
    private var shouldRestart = true
    private var reconnectDelaySeconds: TimeInterval = 1
    private var state: AppServerConnectionState = .disconnected

    func Start() {
        workQueue.async { [weak self] in
            self?.StartOnQueue()
        }
    }

    func Restart() {
        workQueue.async { [weak self] in
            guard let self else {
                return
            }
            self.shouldRestart = true
            self.reconnectDelaySeconds = 1
            self.StopProcessOnQueue()
            self.StartOnQueue()
        }
    }

    func Stop() {
        workQueue.async { [weak self] in
            guard let self else {
                return
            }
            self.shouldRestart = false
            self.StopProcessOnQueue()
            self.EmitState(.disconnected)
        }
    }

    private func StartOnQueue() {
        guard process == nil else {
            return
        }
        EmitState(state == .disconnected ? .connecting : .reconnecting)

        let nextProcess = Process()
        let stdinPipe = Pipe()
        let stdoutPipe = Pipe()
        let stderrPipe = Pipe()

        nextProcess.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        nextProcess.arguments = ["codex", "app-server", "--listen", "stdio://"]
        nextProcess.standardInput = stdinPipe
        nextProcess.standardOutput = stdoutPipe
        nextProcess.standardError = stderrPipe

        stdoutPipe.fileHandleForReading.readabilityHandler = { [weak self] handle in
            let data = handle.availableData
            self?.workQueue.async { [weak self] in
                self?.HandleStdoutData(data)
            }
        }

        stderrPipe.fileHandleForReading.readabilityHandler = { [weak self] handle in
            let data = handle.availableData
            self?.workQueue.async { [weak self] in
                self?.HandleStderrData(data)
            }
        }

        nextProcess.terminationHandler = { [weak self] terminatedProcess in
            self?.workQueue.async { [weak self] in
                self?.HandleTermination(terminatedProcess)
            }
        }

        do {
            try nextProcess.run()
        } catch {
            EmitState(.failed("failed to launch codex app-server: \(error.localizedDescription)"))
            ScheduleReconnectOnQueue()
            return
        }

        process = nextProcess
        stdinHandle = stdinPipe.fileHandleForWriting
        outputBuffer.removeAll(keepingCapacity: true)
        reconnectDelaySeconds = 1
        EmitState(.connected)
        SendInitializeHandshakeOnQueue()
    }

    private func StopProcessOnQueue() {
        guard let runningProcess = process else {
            return
        }
        runningProcess.terminationHandler = nil
        if runningProcess.isRunning {
            runningProcess.terminate()
        }
        process = nil
        stdinHandle = nil
        outputBuffer.removeAll(keepingCapacity: true)
    }

    private func HandleTermination(_ terminatedProcess: Process) {
        if process === terminatedProcess {
            process = nil
            stdinHandle = nil
            outputBuffer.removeAll(keepingCapacity: true)
        }
        if !shouldRestart {
            EmitState(.disconnected)
            return
        }
        EmitState(.reconnecting)
        ScheduleReconnectOnQueue()
    }

    private func ScheduleReconnectOnQueue() {
        let delay = reconnectDelaySeconds
        reconnectDelaySeconds = min(reconnectDelaySeconds * 2, 30)
        workQueue.asyncAfter(deadline: .now() + delay) { [weak self] in
            self?.StartOnQueue()
        }
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
                    "experimentalApi": true,
                ],
            ],
        ]
        WriteJsonLineOnQueue(initialize)

        let initialized: [String: Any] = [
            "method": "initialized",
        ]
        WriteJsonLineOnQueue(initialized)
    }

    private func WriteJsonLineOnQueue(_ object: [String: Any]) {
        guard let handle = stdinHandle else {
            return
        }
        guard let payload = try? JSONSerialization.data(withJSONObject: object) else {
            return
        }
        var line = payload
        line.append(0x0A)
        do {
            try handle.write(contentsOf: line)
        } catch {
            EmitState(.failed("failed to write to app-server stdin: \(error.localizedDescription)"))
        }
    }

    private func HandleStdoutData(_ data: Data) {
        if data.isEmpty {
            return
        }
        outputBuffer.append(data)

        while let newlineRange = outputBuffer.firstRange(of: Data([0x0A])) {
            let lineData = outputBuffer[..<newlineRange.lowerBound]
            outputBuffer.removeSubrange(...newlineRange.lowerBound)
            ParseLineOnQueue(Data(lineData))
        }
    }

    private func ParseLineOnQueue(_ lineData: Data) {
        guard !lineData.isEmpty else {
            return
        }
        guard
            let object = try? JSONSerialization.jsonObject(with: lineData),
            let dict = object as? [String: Any],
            let method = dict["method"] as? String
        else {
            return
        }

        let params = dict["params"] as? [String: Any] ?? [:]
        DispatchQueue.main.async { [weak self] in
            self?.OnNotification?(method, params)
        }
    }

    private func HandleStderrData(_ data: Data) {
        if data.isEmpty {
            return
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
}
