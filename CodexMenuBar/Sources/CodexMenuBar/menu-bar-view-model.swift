import Foundation
import Observation

enum EndpointSection: Hashable {
  case files
  case commands
  case pastRuns
}

@Observable
final class MenuBarViewModel {
  let turnStore: TurnStore

  var connectionState: AppServerConnectionState = .disconnected
  var now: Date = Date()
  var expandedEndpointIds: Set<String> = []
  var expandedRunKeysByEndpoint: [String: Set<String>] = [:]
  private var expandedSectionsByEndpoint: [String: Set<EndpointSection>] = [:]
  private var manualSectionOverridesByEndpoint: [String: Set<EndpointSection>] = [:]
  private var observedTurnIdByEndpoint: [String: String] = [:]
  private var hadActiveTurnByEndpoint: [String: Bool] = [:]

  init(turnStore: TurnStore) {
    self.turnStore = turnStore
  }

  var endpointRows: [EndpointRow] {
    turnStore.EndpointRows
  }

  var runningCount: Int {
    endpointRows.filter { $0.activeTurn != nil }.count
  }

  var activeRateLimitInfo: RateLimitInfo? {
    endpointRows.first(where: { $0.rateLimits != nil })?.rateLimits
  }

  var lowRateLimitWarningText: String? {
    guard
      let rateLimits = activeRateLimitInfo,
      let remaining = rateLimits.remaining,
      let limit = rateLimits.limit,
      limit > 0
    else {
      return nil
    }

    let fraction = Double(remaining) / Double(limit)
    if fraction > 0.10 {
      return nil
    }
    return "Low rate limit (\(remaining)/\(limit))"
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
    expandedSectionsByEndpoint = expandedSectionsByEndpoint.filter { endpointSet.contains($0.key) }
    manualSectionOverridesByEndpoint =
      manualSectionOverridesByEndpoint.filter { endpointSet.contains($0.key) }
    observedTurnIdByEndpoint = observedTurnIdByEndpoint.filter { endpointSet.contains($0.key) }
    hadActiveTurnByEndpoint = hadActiveTurnByEndpoint.filter { endpointSet.contains($0.key) }
    SyncSectionDisclosureState()
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

  func IsSectionExpanded(endpointId: String, section: EndpointSection) -> Bool {
    expandedSectionsByEndpoint[endpointId]?.contains(section) ?? false
  }

  func ToggleSection(endpointId: String, section: EndpointSection) {
    var sectionState = expandedSectionsByEndpoint[endpointId] ?? []
    if sectionState.contains(section) {
      sectionState.remove(section)
    } else {
      sectionState.insert(section)
    }
    expandedSectionsByEndpoint[endpointId] = sectionState

    var overrides = manualSectionOverridesByEndpoint[endpointId] ?? []
    overrides.insert(section)
    manualSectionOverridesByEndpoint[endpointId] = overrides
  }

  func SyncSectionDisclosureState() {
    let rows = endpointRows
    let endpointIds = Set(rows.map(\.endpointId))

    expandedSectionsByEndpoint = expandedSectionsByEndpoint.filter { endpointIds.contains($0.key) }
    manualSectionOverridesByEndpoint =
      manualSectionOverridesByEndpoint.filter { endpointIds.contains($0.key) }
    observedTurnIdByEndpoint = observedTurnIdByEndpoint.filter { endpointIds.contains($0.key) }
    hadActiveTurnByEndpoint = hadActiveTurnByEndpoint.filter { endpointIds.contains($0.key) }

    for row in rows {
      SyncSectionDisclosureState(row: row)
    }
  }

  func ClearExpandedState() {
    expandedEndpointIds.removeAll()
    expandedRunKeysByEndpoint.removeAll()
    expandedSectionsByEndpoint.removeAll()
    manualSectionOverridesByEndpoint.removeAll()
    observedTurnIdByEndpoint.removeAll()
    hadActiveTurnByEndpoint.removeAll()
  }

  private func SyncSectionDisclosureState(row: EndpointRow) {
    let endpointId = row.endpointId
    let currentTurnId = row.turnId
    let previousTurnId = observedTurnIdByEndpoint[endpointId]

    if let currentTurnId, !currentTurnId.isEmpty {
      if let previousTurnId, previousTurnId != currentTurnId {
        manualSectionOverridesByEndpoint[endpointId] = []
      }
      observedTurnIdByEndpoint[endpointId] = currentTurnId
    }

    let hadActiveTurn = hadActiveTurnByEndpoint[endpointId] ?? false
    let hasActiveTurn = row.activeTurn != nil

    let isRunningCommand = row.commands.contains { $0.status == .inProgress }
    let hasFailedCommand = row.commands.last?.status == .failed
    let commandsShouldExpand = hasActiveTurn && (isRunningCommand || hasFailedCommand)
    ApplyAutoSectionState(
      endpointId: endpointId, section: .commands, isExpandedByDefault: commandsShouldExpand)

    let isEditingFiles = hasActiveTurn && row.lastTraceCategory == .edit
    let justCompletedWithFiles =
      hadActiveTurn && !hasActiveTurn && !(row.recentRuns.first?.fileChanges.isEmpty ?? true)
    let filesShouldExpand = isEditingFiles || justCompletedWithFiles
    ApplyAutoSectionState(
      endpointId: endpointId, section: .files, isExpandedByDefault: filesShouldExpand)

    let pastRunsShouldExpand = !hasActiveTurn && !row.recentRuns.isEmpty
    ApplyAutoSectionState(
      endpointId: endpointId, section: .pastRuns, isExpandedByDefault: pastRunsShouldExpand)

    hadActiveTurnByEndpoint[endpointId] = hasActiveTurn
  }

  private func ApplyAutoSectionState(
    endpointId: String,
    section: EndpointSection,
    isExpandedByDefault: Bool
  ) {
    guard !(manualSectionOverridesByEndpoint[endpointId]?.contains(section) ?? false) else {
      return
    }
    SetSectionExpanded(endpointId: endpointId, section: section, isExpanded: isExpandedByDefault)
  }

  private func SetSectionExpanded(endpointId: String, section: EndpointSection, isExpanded: Bool) {
    var sections = expandedSectionsByEndpoint[endpointId] ?? []
    if isExpanded {
      sections.insert(section)
    } else {
      sections.remove(section)
    }
    expandedSectionsByEndpoint[endpointId] = sections
  }
}
