import XCTest
@testable import rem

final class RemAppTests: XCTestCase {
    private func makeTempStore() throws -> (URL, RemStore) {
        let dir = FileManager.default.temporaryDirectory
            .appendingPathComponent("rem-ios-tests-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let dbPath = dir.appendingPathComponent("collection.sqlite3").path
        let store = try RemStore(dbPath: dbPath, credentialStore: InMemoryCredentialStore())
        return (dir, store)
    }

    func testDeckCardAssetAndStudyLifecycle() async throws {
        let (dir, store) = try makeTempStore()
        defer { try? FileManager.default.removeItem(at: dir) }

        let now: Int64 = 1_763_720_000_000
        let deck = try await store.createDeck(name: "Distributed systems", color: "#5a47e8", nowMs: now)
        XCTAssertEqual(deck.name, "Distributed systems")
        XCTAssertEqual(deck.color, "#5a47e8")

        let assetBytes = Data([0x89, 0x50, 0x4E, 0x47])
        let hash = try await store.putAsset(mime: "image/png", bytes: assetBytes, nowMs: now)
        let fetchedAsset = try await store.getAsset(hash: hash)
        XCTAssertEqual(fetchedAsset?.bytes, assetBytes)

        let card = try await store.createCard(
            deckId: deck.id,
            front: "Why single-threaded Durable Objects?",
            back: "Gap-free monotonic seq cursors. ![](asset:\(hash))",
            tags: ["sync", "cloudflare"],
            nowMs: now + 10
        )
        XCTAssertEqual(card.assetHashes, [hash])

        let summary = try await store.getTodaySummary(nowMs: now + 100)
        XCTAssertEqual(summary.totalDue, 1)
        XCTAssertEqual(summary.decks.count, 1)
        XCTAssertEqual(summary.estimatedMinutes, 1)

        let session = try await store.startStudy(deckId: deck.id, nowMs: now + 100)
        let revealed = try session.reveal(nowMs: now + 200)
        XCTAssertTrue(revealed.revealed)
        XCTAssertNotNil(revealed.intervals)

        let outcome = try session.grade(grade: .easy, nowMs: now + 300)
        XCTAssertNil(outcome.conflictCardId)
        XCTAssertEqual(outcome.view.reviewed, 1)
        XCTAssertTrue(outcome.view.canUndo)

        let undone = try session.undoLastGrade(nowMs: now + 350)
        XCTAssertEqual(undone.reviewed, 0)
        XCTAssertEqual(undone.current?.id, card.id)
    }

    func testKeychainAccountKeyNormalization() {
        let keyA = KeychainCredentialStore.accountKey(
            serverUrl: "https://REM-SYNC.shettyh.fyi/",
            accountId: " user-123 "
        )
        let keyB = KeychainCredentialStore.accountKey(
            serverUrl: "https://rem-sync.shettyh.fyi",
            accountId: "user-123"
        )
        XCTAssertEqual(keyA, keyB)
    }

    func testMarkdownParserExtractsCodeQuotesListsAndAssetImages() {
        let hash = String(repeating: "a", count: 64)
        let md = """
        # TypeScript

        Use `satisfies` to validate without widening.

        ```ts
        const x = 1 satisfies number
        ```

        > Keeps literal types intact.

        - First
        - Second

        ![Diagram](asset:\(hash))
        """
        let blocks = MarkdownBlockParser.parse(md)
        XCTAssertEqual(blocks.count, 6)
        XCTAssertEqual(blocks[0], .heading(level: 1, text: "TypeScript"))
        XCTAssertEqual(blocks[1], .paragraph(text: "Use `satisfies` to validate without widening."))
        XCTAssertEqual(blocks[2], .codeBlock(language: "ts", code: "const x = 1 satisfies number"))
        XCTAssertEqual(blocks[3], .blockquote(text: "Keeps literal types intact."))
        XCTAssertEqual(blocks[4], .bulletList(items: ["First", "Second"]))
        XCTAssertEqual(blocks[5], .image(alt: "Diagram", source: .asset(hash: hash)))

        let highlighted = CodeHighlighter.highlight("const x = 'hi' // note", language: "ts")
        XCTAssertEqual(String(highlighted.characters), "const x = 'hi' // note")
    }
}
