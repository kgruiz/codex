import Foundation
import XCTest

@testable import CodexMenuBar

final class MenuBarViewModelTests: XCTestCase {
  func testCommandsSectionAutoExpandsForRunningAndFailedCommands() {
    let store = TurnStore()
    let model = MenuBarViewModel(turnStore: store)
    model.SetEndpointIds(["ep-1"])

    let start = Date(timeIntervalSince1970: 1_700_000_100)
    store.UpsertTurnStarted(endpointId: "ep-1", threadId: "thread-1", turnId: "turn-1", at: start)

    store.RecordCommand(
      endpointId: "ep-1",
      turnId: "turn-1",
      command: CommandSummary(command: "swift test", status: .inProgress, exitCode: nil, durationMs: nil)
    )
    model.SyncSectionDisclosureState()
    XCTAssertTrue(model.IsSectionExpanded(endpointId: "ep-1", section: .commands))

    store.RecordCommand(
      endpointId: "ep-1",
      turnId: "turn-1",
      command: CommandSummary(command: "swift test", status: .completed, exitCode: 0, durationMs: 1200)
    )
    model.SyncSectionDisclosureState()
    XCTAssertFalse(model.IsSectionExpanded(endpointId: "ep-1", section: .commands))

    store.RecordCommand(
      endpointId: "ep-1",
      turnId: "turn-1",
      command: CommandSummary(command: "swift test", status: .failed, exitCode: 1, durationMs: 1500)
    )
    model.SyncSectionDisclosureState()
    XCTAssertTrue(model.IsSectionExpanded(endpointId: "ep-1", section: .commands))
  }

  func testFilesSectionAutoExpandsForEditAndTurnCompletionSummary() {
    let store = TurnStore()
    let model = MenuBarViewModel(turnStore: store)
    model.SetEndpointIds(["ep-1"])

    let start = Date(timeIntervalSince1970: 1_700_000_200)
    store.UpsertTurnStarted(endpointId: "ep-1", threadId: "thread-1", turnId: "turn-1", at: start)
    store.RecordProgress(
      endpointId: "ep-1",
      threadId: "thread-1",
      turnId: "turn-1",
      category: .edit,
      state: .started,
      label: "Editing",
      at: start.addingTimeInterval(1)
    )
    store.RecordFileChange(
      endpointId: "ep-1",
      turnId: "turn-1",
      change: FileChangeSummary(path: "src/main.swift", kind: .update)
    )

    model.SyncSectionDisclosureState()
    XCTAssertTrue(model.IsSectionExpanded(endpointId: "ep-1", section: .files))

    store.MarkTurnCompleted(
      endpointId: "ep-1",
      threadId: "thread-1",
      turnId: "turn-1",
      status: .completed,
      at: start.addingTimeInterval(2)
    )

    model.SyncSectionDisclosureState()
    XCTAssertTrue(model.IsSectionExpanded(endpointId: "ep-1", section: .files))
  }

  func testPastRunsSectionOnlyAutoExpandsWhileIdle() {
    let store = TurnStore()
    let model = MenuBarViewModel(turnStore: store)
    model.SetEndpointIds(["ep-1"])

    let start = Date(timeIntervalSince1970: 1_700_000_300)
    store.UpsertTurnStarted(endpointId: "ep-1", threadId: "thread-1", turnId: "turn-1", at: start)
    store.MarkTurnCompleted(
      endpointId: "ep-1",
      threadId: "thread-1",
      turnId: "turn-1",
      status: .completed,
      at: start.addingTimeInterval(2)
    )

    model.SyncSectionDisclosureState()
    XCTAssertTrue(model.IsSectionExpanded(endpointId: "ep-1", section: .pastRuns))

    store.UpsertTurnStarted(
      endpointId: "ep-1",
      threadId: "thread-1",
      turnId: "turn-2",
      at: start.addingTimeInterval(5)
    )

    model.SyncSectionDisclosureState()
    XCTAssertFalse(model.IsSectionExpanded(endpointId: "ep-1", section: .pastRuns))
  }

  func testManualOverrideIsClearedOnNewTurn() {
    let store = TurnStore()
    let model = MenuBarViewModel(turnStore: store)
    model.SetEndpointIds(["ep-1"])

    let start = Date(timeIntervalSince1970: 1_700_000_400)
    store.UpsertTurnStarted(endpointId: "ep-1", threadId: "thread-1", turnId: "turn-1", at: start)
    store.MarkTurnCompleted(
      endpointId: "ep-1",
      threadId: "thread-1",
      turnId: "turn-1",
      status: .completed,
      at: start.addingTimeInterval(2)
    )

    model.SyncSectionDisclosureState()
    XCTAssertTrue(model.IsSectionExpanded(endpointId: "ep-1", section: .pastRuns))

    model.ToggleSection(endpointId: "ep-1", section: .pastRuns)
    XCTAssertFalse(model.IsSectionExpanded(endpointId: "ep-1", section: .pastRuns))

    model.SyncSectionDisclosureState()
    XCTAssertFalse(model.IsSectionExpanded(endpointId: "ep-1", section: .pastRuns))

    store.UpsertTurnStarted(
      endpointId: "ep-1",
      threadId: "thread-1",
      turnId: "turn-2",
      at: start.addingTimeInterval(6)
    )
    store.MarkTurnCompleted(
      endpointId: "ep-1",
      threadId: "thread-1",
      turnId: "turn-2",
      status: .completed,
      at: start.addingTimeInterval(8)
    )

    model.SyncSectionDisclosureState()
    XCTAssertTrue(model.IsSectionExpanded(endpointId: "ep-1", section: .pastRuns))
  }

  func testLowRateLimitWarningUsesTenPercentThreshold() {
    let store = TurnStore()
    let model = MenuBarViewModel(turnStore: store)
    model.SetEndpointIds(["ep-1"])

    store.UpdateRateLimits(rateLimits: RateLimitInfo(remaining: 10, limit: 100, resetsAt: nil))
    XCTAssertEqual(model.lowRateLimitWarningText, "Low rate limit (10/100)")

    store.UpdateRateLimits(rateLimits: RateLimitInfo(remaining: 11, limit: 100, resetsAt: nil))
    XCTAssertNil(model.lowRateLimitWarningText)
  }
}
