import Foundation
import XCTest

@testable import CodexMenuBar

final class TurnStoreHistoryTests: XCTestCase {
  func testArchivesTurnOnCompletion() {
    let store = TurnStore()
    let start = Date(timeIntervalSince1970: 1_700_000_000)
    let end = start.addingTimeInterval(5)

    store.UpsertTurnStarted(endpointId: "ep-1", threadId: "thread-1", turnId: "turn-1", at: start)
    store.RecordProgress(
      endpointId: "ep-1",
      threadId: "thread-1",
      turnId: "turn-1",
      category: .gen,
      state: .started,
      label: "Thinking",
      at: start.addingTimeInterval(1)
    )
    store.RecordProgress(
      endpointId: "ep-1",
      threadId: "thread-1",
      turnId: "turn-1",
      category: .gen,
      state: .completed,
      label: "Done",
      at: start.addingTimeInterval(2)
    )
    store.RecordFileChange(
      endpointId: "ep-1",
      turnId: "turn-1",
      change: FileChangeSummary(path: "src/main.swift", kind: .update)
    )
    store.RecordCommand(
      endpointId: "ep-1",
      turnId: "turn-1",
      command: CommandSummary(
        command: "swift test",
        status: .completed,
        exitCode: 0,
        durationMs: 1200
      )
    )
    store.MarkTurnCompleted(
      endpointId: "ep-1",
      threadId: "thread-1",
      turnId: "turn-1",
      status: .completed,
      at: end
    )

    let rows = store.EndpointRows(activeEndpointIds: ["ep-1"])
    XCTAssertEqual(rows.count, 1)
    XCTAssertEqual(rows[0].recentRuns.count, 1)
    XCTAssertEqual(rows[0].recentRuns[0].turnId, "turn-1")
    XCTAssertEqual(rows[0].recentRuns[0].status, .completed)
    XCTAssertEqual(rows[0].recentRuns[0].fileChanges.map(\.path), ["src/main.swift"])
    XCTAssertEqual(rows[0].recentRuns[0].commands.map(\.command), ["swift test"])
    XCTAssertFalse(rows[0].recentRuns[0].TimelineSegments().isEmpty)
  }

  func testArchivesTurnWhenSnapshotReconciliationCompletesIt() {
    let store = TurnStore()
    let start = Date(timeIntervalSince1970: 1_700_000_000)
    let reconcile = start.addingTimeInterval(3)

    store.UpsertTurnStarted(endpointId: "ep-1", threadId: "thread-1", turnId: "turn-1", at: start)
    store.ReconcileSnapshotActiveTurns(endpointId: "ep-1", activeTurnKeys: [], at: reconcile)

    let rows = store.EndpointRows(activeEndpointIds: ["ep-1"])
    XCTAssertEqual(rows.count, 1)
    XCTAssertEqual(rows[0].recentRuns.count, 1)
    XCTAssertEqual(rows[0].recentRuns[0].turnId, "turn-1")
    XCTAssertEqual(rows[0].recentRuns[0].status, .completed)
  }

  func testSnapshotReconciliationHonorsEndpointTurnKeys() {
    let store = TurnStore()
    let start = Date(timeIntervalSince1970: 1_700_000_000)

    store.UpsertTurnStarted(endpointId: "ep-1", threadId: "thread-1", turnId: "turn-1", at: start)
    store.ReconcileSnapshotActiveTurns(
      endpointId: "ep-1",
      activeTurnKeys: ["ep-1:turn-1"],
      at: start.addingTimeInterval(1)
    )

    let rows = store.EndpointRows(activeEndpointIds: ["ep-1"])
    XCTAssertEqual(rows.count, 1)
    XCTAssertTrue(rows[0].recentRuns.isEmpty)
    XCTAssertEqual(rows[0].activeTurn?.turnId, "turn-1")
    XCTAssertEqual(rows[0].activeTurn?.status, .inProgress)
  }

  func testTokenUsageUpdateBackfillsArchivedRun() {
    let store = TurnStore()
    let start = Date(timeIntervalSince1970: 1_700_000_000)

    store.UpsertTurnStarted(endpointId: "ep-1", threadId: "thread-1", turnId: "turn-1", at: start)
    store.MarkTurnCompleted(
      endpointId: "ep-1",
      threadId: "thread-1",
      turnId: "turn-1",
      status: .completed,
      at: start.addingTimeInterval(1)
    )

    var usage = TokenUsageInfo()
    usage.totalTokens = 123
    usage.inputTokens = 77
    usage.outputTokens = 46

    store.UpdateTokenUsage(
      endpointId: "ep-1",
      threadId: "thread-1",
      turnId: "turn-1",
      tokenUsage: usage
    )

    let rows = store.EndpointRows(activeEndpointIds: ["ep-1"])
    XCTAssertEqual(rows.count, 1)
    XCTAssertEqual(rows[0].recentRuns.count, 1)
    XCTAssertEqual(rows[0].recentRuns[0].tokenUsage, usage)
  }

  func testApplyItemMetadataExtractsPromptPreviewFromStringAndArrayContent() {
    let store = TurnStore()
    let now = Date(timeIntervalSince1970: 1_700_000_000)

    store.ApplyItemMetadata(
      endpointId: "ep-1",
      threadId: "thread-1",
      turnId: "turn-1",
      item: [
        "type": "userMessage",
        "content": [
          "Summarize",
          ["type": "text", "text": "this diff"],
          ["type": "input_text", "text": "quickly"],
        ],
      ],
      at: now
    )

    var rows = store.EndpointRows(activeEndpointIds: ["ep-1"])
    XCTAssertEqual(rows.first?.promptPreview, "Summarize this diff quickly")

    store.ApplyItemMetadata(
      endpointId: "ep-1",
      threadId: "thread-1",
      turnId: "turn-1",
      item: [
        "type": "userMessage",
        "content": "Single string prompt",
      ],
      at: now.addingTimeInterval(1)
    )

    rows = store.EndpointRows(activeEndpointIds: ["ep-1"])
    XCTAssertEqual(rows.first?.promptPreview, "Single string prompt")
  }

  func testApplyItemMetadataExtractsPromptPreviewFromPascalCaseUserMessageType() {
    let store = TurnStore()
    let now = Date(timeIntervalSince1970: 1_700_000_000)

    store.ApplyItemMetadata(
      endpointId: "ep-1",
      threadId: "thread-1",
      turnId: "turn-1",
      item: [
        "type": "UserMessage",
        "content": [
          ["type": "text", "text": "Plan"],
          ["type": "text", "text": "next steps"],
        ],
      ],
      at: now
    )

    let rows = store.EndpointRows(activeEndpointIds: ["ep-1"])
    XCTAssertEqual(rows.first?.promptPreview, "Plan next steps")
  }

  func testTurnMetadataCapturesThinkingLevelAndPersistsToHistory() {
    let store = TurnStore()
    let start = Date(timeIntervalSince1970: 1_700_000_000)
    let turn: [String: Any] = [
      "id": "turn-1",
      "model": "gpt-5",
      "modelProvider": "openai",
      "reasoningEffort": "high",
    ]

    store.UpsertTurnStarted(endpointId: "ep-1", threadId: "thread-1", turnId: "turn-1", at: start)
    store.UpdateTurnMetadata(
      endpointId: "ep-1",
      threadId: "thread-1",
      turnId: "turn-1",
      turn: turn,
      at: start.addingTimeInterval(1)
    )
    store.MarkTurnCompleted(
      endpointId: "ep-1",
      threadId: "thread-1",
      turnId: "turn-1",
      status: .completed,
      at: start.addingTimeInterval(2)
    )

    let rows = store.EndpointRows(activeEndpointIds: ["ep-1"])
    XCTAssertEqual(rows.first?.thinkingLevel, "high")
    XCTAssertEqual(rows.first?.recentRuns.first?.thinkingLevel, "high")
  }

  func testCompletedRunHistoryIsCappedAtFifty() {
    let store = TurnStore()
    let base = Date(timeIntervalSince1970: 1_700_000_000)

    for index in 0..<55 {
      let start = base.addingTimeInterval(Double(index) * 10)
      let end = start.addingTimeInterval(2)
      let turnId = "turn-\(index)"
      store.UpsertTurnStarted(endpointId: "ep-1", threadId: "thread-1", turnId: turnId, at: start)
      store.MarkTurnCompleted(
        endpointId: "ep-1",
        threadId: "thread-1",
        turnId: turnId,
        status: .completed,
        at: end
      )
    }

    let rows = store.EndpointRows(activeEndpointIds: ["ep-1"])
    XCTAssertEqual(rows.count, 1)
    XCTAssertEqual(rows[0].recentRuns.count, 50)
    XCTAssertEqual(rows[0].recentRuns.first?.turnId, "turn-54")
    XCTAssertEqual(rows[0].recentRuns.last?.turnId, "turn-5")
  }

  func testCompletedRunHistoryIsIsolatedPerEndpoint() {
    let store = TurnStore()
    let base = Date(timeIntervalSince1970: 1_700_000_000)

    store.UpsertTurnStarted(endpointId: "ep-1", threadId: "thread-a", turnId: "turn-a", at: base)
    store.MarkTurnCompleted(
      endpointId: "ep-1",
      threadId: "thread-a",
      turnId: "turn-a",
      status: .completed,
      at: base.addingTimeInterval(1)
    )

    store.UpsertTurnStarted(
      endpointId: "ep-2",
      threadId: "thread-b",
      turnId: "turn-b",
      at: base.addingTimeInterval(2)
    )
    store.MarkTurnCompleted(
      endpointId: "ep-2",
      threadId: "thread-b",
      turnId: "turn-b",
      status: .interrupted,
      at: base.addingTimeInterval(3)
    )

    let rows = store.EndpointRows(activeEndpointIds: ["ep-1", "ep-2"])
      .sorted { $0.endpointId < $1.endpointId }
    XCTAssertEqual(rows.count, 2)
    XCTAssertEqual(rows[0].endpointId, "ep-1")
    XCTAssertEqual(rows[0].recentRuns.map(\.turnId), ["turn-a"])
    XCTAssertEqual(rows[1].endpointId, "ep-2")
    XCTAssertEqual(rows[1].recentRuns.map(\.turnId), ["turn-b"])
    XCTAssertEqual(rows[1].recentRuns.first?.status, .interrupted)
  }
}
