import Foundation
import Observation

@Observable
final class MenuBarViewModel {
  let turnStore: TurnStore

  var connectionState: AppServerConnectionState = .disconnected
  var now: Date = Date()
  var expandedEndpointIds: Set<String> = []
  var expandedRunKeysByEndpoint: [String: Set<String>] = [:]

  init(turnStore: TurnStore) {
    self.turnStore = turnStore
  }

  var endpointRows: [EndpointRow] {
    turnStore.EndpointRows
  }

  var runningCount: Int {
    endpointRows.filter { $0.activeTurn != nil }.count
  }

  var headerTitle: String {
    switch connectionState {
    case .connected:
      if runningCount == 0 {
        return "Codex - connected"
      }
      return "Codex - \(runningCount) active"
    case .connecting:
      return "Codex - connecting..."
    case .reconnecting:
      return "Codex - reconnecting..."
    case .failed(let message):
      return "Codex - error: \(message)"
    case .disconnected:
      return "Codex - disconnected"
    }
  }

  func SetEndpointIds(_ endpointIds: [String]) {
    turnStore.SetActiveEndpointIds(endpointIds)
    let endpointSet = Set(endpointIds)
    expandedEndpointIds = expandedEndpointIds.intersection(endpointSet)
    expandedRunKeysByEndpoint = expandedRunKeysByEndpoint.filter { endpointSet.contains($0.key) }
  }

  func ToggleEndpoint(_ endpointId: String) {
    if expandedEndpointIds.contains(endpointId) {
      expandedEndpointIds.remove(endpointId)
    } else {
      expandedEndpointIds.insert(endpointId)
    }
  }

  func ToggleRun(endpointId: String, runKey: String) {
    var runKeys = expandedRunKeysByEndpoint[endpointId] ?? []
    if runKeys.contains(runKey) {
      runKeys.remove(runKey)
    } else {
      runKeys.insert(runKey)
    }
    expandedRunKeysByEndpoint[endpointId] = runKeys
  }

  func ClearExpandedState() {
    expandedEndpointIds.removeAll()
    expandedRunKeysByEndpoint.removeAll()
  }
}
