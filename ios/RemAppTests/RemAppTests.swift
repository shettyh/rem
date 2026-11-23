import AuthenticationServices
import StoreKit
import StoreKitTest
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

    func testMarkdownParserParsesTablesAndPlainTextPreview() {
        let tableMd = """
        | Engine | Write | Read |
        | --- | :---: | ---: |
        | B-Tree | Random | Fast |
        | LSM | Sequential | Compaction |
        """
        let blocks = MarkdownBlockParser.parse(tableMd)
        XCTAssertEqual(blocks.count, 1)
        XCTAssertEqual(
            blocks[0],
            .table(
                headers: ["Engine", "Write", "Read"],
                rows: [
                    ["B-Tree", "Random", "Fast"],
                    ["LSM", "Sequential", "Compaction"]
                ]
            )
        )

        let rawCard = """
        ## What is `WAL`?
        **Write-Ahead Logging** in [SQLite](https://sqlite.org)
        ![Diagram](asset:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa)
        """
        XCTAssertEqual(
            MarkdownBlockParser.plainTextPreview(rawCard),
            "What is WAL? Write-Ahead Logging in SQLite Diagram"
        )

        XCTAssertEqual(ReviewScreen.formatDuration(seconds: 0), "1s")
        XCTAssertEqual(ReviewScreen.formatDuration(seconds: 42), "42s")
        XCTAssertEqual(ReviewScreen.formatDuration(seconds: 120), "2m")
        XCTAssertEqual(ReviewScreen.formatDuration(seconds: 125), "2m 5s")
    }

    func testOAuthCallbackParsingValidatesStateAndExtractsCode() throws {
        let valid = URL(string: "rem://auth/callback?code=abc123&state=xyz789")!
        XCTAssertEqual(
            try AuthSessionManager.parseOAuthCallback(url: valid, expectedState: "xyz789"),
            "abc123"
        )

        let badState = URL(string: "rem://auth/callback?code=abc123&state=wrong")!
        XCTAssertThrowsError(try AuthSessionManager.parseOAuthCallback(url: badState, expectedState: "xyz789"))

        let providerErr = URL(string: "rem://auth/callback?error=access_denied&state=xyz789")!
        XCTAssertThrowsError(try AuthSessionManager.parseOAuthCallback(url: providerErr, expectedState: "xyz789"))
    }

    func testNativeAppleNonceGenerationAndSha256Hex() {
        let n1 = AuthSessionManager.generateNonce()
        let n2 = AuthSessionManager.generateNonce()
        XCTAssertGreaterThanOrEqual(n1.count, 32)
        XCTAssertNotEqual(n1, n2)
        XCTAssertEqual(
            AuthSessionManager.sha256Hex("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        )
    }

    func testMapAppleAuthorizationErrorFormatsCleanMessages() {
        let canceled = AuthSessionManager.mapAppleAuthorizationError(ASAuthorizationError(.canceled))
        XCTAssertEqual(canceled as? OAuthFlowError, .cancelled)

        let unknown = AuthSessionManager.mapAppleAuthorizationError(ASAuthorizationError(.unknown))
        let unknownMessage = unknown.remDisplayMessage
        XCTAssertTrue(unknownMessage.contains("Apple error 1000"))
        XCTAssertFalse(unknownMessage.contains("com.apple.AuthenticationServices.AuthorizationError"))

        let failed = AuthSessionManager.mapAppleAuthorizationError(ASAuthorizationError(.failed))
        let failedMessage = failed.remDisplayMessage
        XCTAssertTrue(failedMessage.contains("Apple error 1004"))
        XCTAssertFalse(failedMessage.contains("Operation"))
    }

    func testBackupExportAndPerDeckStats() async throws {
        let (dir, store) = try makeTempStore()
        defer { try? FileManager.default.removeItem(at: dir) }

        let now: Int64 = 1_763_720_000_000
        let deck = try await store.createDeck(name: "TypeScript", color: "#d97914", nowMs: now)
        _ = try await store.createCard(
            deckId: deck.id,
            front: "What does satisfies do?",
            back: "Validates without widening.",
            tags: ["ts"],
            nowMs: now
        )
        let session = try await store.startStudy(deckId: deck.id, nowMs: now + 10)
        _ = try session.reveal(nowMs: now + 20)
        _ = try session.grade(grade: .easy, nowMs: now + 30)

        let stats = try await store.getStats(deckId: nil, days: 30, nowMs: now + 50)
        XCTAssertEqual(stats.reviews, 1)
        XCTAssertEqual(stats.byDeck.count, 1)
        XCTAssertEqual(stats.byDeck.first?.deckName, "TypeScript")

        let backupJson = try await store.exportBackupJson()
        XCTAssertTrue(backupJson.contains("TypeScript"))
        XCTAssertTrue(backupJson.contains("What does satisfies do?"))
    }

    func testMarkdownFormatterWrapsSelectionAndRemovesAssets() {
        let wrappedBold = MarkdownFormatter.apply(
            .bold,
            to: "Hello hash ring world",
            selectedRange: NSRange(location: 6, length: 9)
        )
        XCTAssertEqual(wrappedBold.text, "Hello **hash ring** world")
        XCTAssertEqual(wrappedBold.selectedRange, NSRange(location: 19, length: 0))

        let emptyItalic = MarkdownFormatter.apply(
            .italic,
            to: "Note: ",
            selectedRange: NSRange(location: 6, length: 0)
        )
        XCTAssertEqual(emptyItalic.text, "Note: *text*")
        XCTAssertEqual(emptyItalic.selectedRange, NSRange(location: 7, length: 4))

        let code = MarkdownFormatter.apply(
            .inlineCode,
            to: "Use map here",
            selectedRange: NSRange(location: 4, length: 3)
        )
        XCTAssertEqual(code.text, "Use `map` here")

        let bullet = MarkdownFormatter.apply(
            .bulletList,
            to: "First\nSecond",
            selectedRange: NSRange(location: 0, length: 12)
        )
        XCTAssertEqual(bullet.text, "- First\n- Second")

        let hash = String(repeating: "f", count: 64)
        let stripped = MarkdownFormatter.removeAssetReference(
            hash: hash,
            from: "Answer text\n![](asset:\(hash))\n"
        )
        XCTAssertEqual(stripped, "Answer text")
    }

    @MainActor
    func testAppStateEntitlementCachingAndPausedStateTransitions() async throws {
        let (dir, store) = try makeTempStore()
        defer {
            AppState.clearCachedEntitlement()
            try? FileManager.default.removeItem(at: dir)
        }

        let accountId = "usr_test_entitlement"
        let trialInfo = EntitlementInfoDto(
            state: .trialing,
            syncEnabled: true,
            provider: "trial",
            plan: "trial_30d",
            expiresAt: 1_900_000_000_000,
            cancelAtPeriodEnd: false,
            canStartTrial: false,
            trialDays: 30,
            priceDisplay: "$5/year",
            canSubscribePolar: false,
            canManageBilling: false
        )

        AppState.saveCachedEntitlement(trialInfo, for: accountId)
        let loaded = AppState.loadCachedEntitlement(for: accountId)
        XCTAssertEqual(loaded?.state, .trialing)
        XCTAssertEqual(loaded?.syncEnabled, true)
        XCTAssertEqual(loaded?.expiresAt, 1_900_000_000_000)
        XCTAssertNil(AppState.loadCachedEntitlement(for: "usr_other"))

        let appState = AppState(store: store)
        let pausedProfile = AccountProfileDto(
            id: accountId,
            email: "alice@example.com",
            providers: ["apple"],
            authMethod: "jwt",
            syncEnabled: false,
            entitlement: EntitlementInfoDto(
                state: .none,
                syncEnabled: false,
                provider: nil,
                plan: nil,
                expiresAt: nil,
                cancelAtPeriodEnd: false,
                canStartTrial: true,
                trialDays: 30,
                priceDisplay: "$5/year",
                canSubscribePolar: false,
                canManageBilling: false
            )
        )
        appState.applyAccountProfile(pausedProfile)
        XCTAssertEqual(appState.effectiveEntitlementState, .none)
        XCTAssertTrue(appState.syncAccessRestricted)

        let activeProfile = AccountProfileDto(
            id: accountId,
            email: "alice@example.com",
            providers: ["apple"],
            authMethod: "jwt",
            syncEnabled: true,
            entitlement: trialInfo
        )
        appState.applyAccountProfile(activeProfile)
        XCTAssertEqual(appState.effectiveEntitlementState, .trialing)
        XCTAssertFalse(appState.syncAccessRestricted)

        AppState.clearCachedEntitlement()
        XCTAssertNil(AppState.loadCachedEntitlement(for: accountId))
    }

    func testAppleAppAccountTokenMatchesServerTestVectors() {
        // 1. Standard 16-byte production userId (`usr_` + 22 base64url chars) matches server bijectively
        let prodToken = StoreKitManager.appAccountToken(for: "usr_ASNFZ4mrze_-3LqYdlQyEA")
        XCTAssertEqual(prodToken.uuidString.lowercased(), "01234567-89ab-cdef-fedc-ba9876543210")

        // 2. Non-standard userId produces identical RFC 9562 UUIDv8 as `sync-server/src/appleBilling.ts`
        let customToken = StoreKitManager.appAccountToken(for: "usr_test_entitlement")
        XCTAssertEqual(customToken.uuidString.lowercased(), "ffeedbb4-45cf-8678-85e8-487c7c392293")

        // 3. Canonical UUID passes through unchanged
        let direct = StoreKitManager.appAccountToken(for: "11111111-2222-3333-4444-555555555555")
        XCTAssertEqual(direct.uuidString.lowercased(), "11111111-2222-3333-4444-555555555555")
    }

    @MainActor
    func testAppleSubscribeAndManageVisibilityPreventsDoubleBilling() throws {
        let (dir, store) = try makeTempStore()
        defer {
            AppState.clearCachedEntitlement()
            try? FileManager.default.removeItem(at: dir)
        }

        let appState = AppState(store: store)
        appState.syncStatus = SyncStatusDto(
            mode: .cloud,
            signedIn: true,
            serverUrl: "https://rem-sync.shettyh.fyi",
            defaultServerUrl: "https://rem-sync.shettyh.fyi",
            accountId: "usr_vis_test",
            email: "alice@example.com",
            provider: "apple",
            pendingOutboxCount: 0,
            lastSeq: 0
        )

        func makeEntitlement(
            state: EntitlementStateDto,
            syncEnabled: Bool,
            provider: String?,
            expiresAt: Int64? = nil
        ) -> EntitlementInfoDto {
            EntitlementInfoDto(
                state: state,
                syncEnabled: syncEnabled,
                provider: provider,
                plan: nil,
                expiresAt: expiresAt,
                cancelAtPeriodEnd: false,
                canStartTrial: state == .none,
                trialDays: 30,
                priceDisplay: "$5/year",
                canSubscribePolar: false,
                canManageBilling: false
            )
        }

        // none -> show Subscribe, hide Manage
        appState.entitlement = makeEntitlement(state: .none, syncEnabled: false, provider: nil)
        XCTAssertTrue(appState.canShowAppleSubscribe)
        XCTAssertFalse(appState.canShowAppleManageSubscription)

        // trialing -> show Subscribe, hide Manage
        appState.entitlement = makeEntitlement(state: .trialing, syncEnabled: true, provider: "trial", expiresAt: 1_900_000_000_000)
        XCTAssertTrue(appState.canShowAppleSubscribe)
        XCTAssertFalse(appState.canShowAppleManageSubscription)

        // active Polar -> hide Subscribe (prevent double-billing!) & hide Apple Manage
        appState.entitlement = makeEntitlement(state: .active, syncEnabled: true, provider: "polar", expiresAt: 1_900_000_000_000)
        XCTAssertFalse(appState.canShowAppleSubscribe)
        XCTAssertFalse(appState.canShowAppleManageSubscription)

        // active Apple -> hide Subscribe & show Apple Manage
        appState.entitlement = makeEntitlement(state: .active, syncEnabled: true, provider: "apple", expiresAt: 1_900_000_000_000)
        XCTAssertFalse(appState.canShowAppleSubscribe)
        XCTAssertTrue(appState.canShowAppleManageSubscription)

        // indefinite invite -> hide Subscribe & hide Manage
        appState.entitlement = makeEntitlement(state: .invite, syncEnabled: true, provider: "invite", expiresAt: nil)
        XCTAssertFalse(appState.canShowAppleSubscribe)
        XCTAssertFalse(appState.canShowAppleManageSubscription)

        // time-limited invite -> allow Subscribe, hide Manage
        appState.entitlement = makeEntitlement(state: .invite, syncEnabled: true, provider: "invite", expiresAt: 1_900_000_000_000)
        XCTAssertTrue(appState.canShowAppleSubscribe)
        XCTAssertFalse(appState.canShowAppleManageSubscription)

        // past_due Apple -> hide Subscribe, show Manage
        appState.entitlement = makeEntitlement(state: .pastDue, syncEnabled: true, provider: "apple", expiresAt: 1_900_000_000_000)
        XCTAssertFalse(appState.canShowAppleSubscribe)
        XCTAssertTrue(appState.canShowAppleManageSubscription)

        // past_due Polar -> hide Subscribe, hide Apple Manage
        appState.entitlement = makeEntitlement(state: .pastDue, syncEnabled: true, provider: "polar", expiresAt: 1_900_000_000_000)
        XCTAssertFalse(appState.canShowAppleSubscribe)
        XCTAssertFalse(appState.canShowAppleManageSubscription)

        // expired -> show Subscribe, hide Manage
        appState.entitlement = makeEntitlement(state: .expired, syncEnabled: false, provider: "trial")
        XCTAssertTrue(appState.canShowAppleSubscribe)
        XCTAssertFalse(appState.canShowAppleManageSubscription)

        // self-hosted -> hide both
        appState.entitlement = makeEntitlement(state: .selfHosted, syncEnabled: true, provider: "self_hosted")
        XCTAssertFalse(appState.canShowAppleSubscribe)
        XCTAssertFalse(appState.canShowAppleManageSubscription)
    }

    @MainActor
    func testStoreKitPurchaseAndRestoreFlowWithLocalConfiguration() async throws {
        let session = try SKTestSession(configurationFileNamed: "RemPremium")
        session.disableDialogs = true
        session.clearTransactions()
        defer { session.clearTransactions() }

        let manager = StoreKitManager()
        XCTAssertNil(manager.localizedPrice)
        await manager.loadProducts()
        XCTAssertEqual(manager.product?.id, StoreKitManager.premiumYearlyProductID)
        XCTAssertEqual(manager.localizedPrice, "$4.99")
        XCTAssertEqual(manager.displayPrice, "$4.99")

        var verifiedJwsList: [String] = []
        let purchaseOutcome = await manager.purchase(accountId: "usr_ASNFZ4mrze_-3LqYdlQyEA") { jws in
            verifiedJwsList.append(jws)
        }
        XCTAssertEqual(purchaseOutcome, .purchased)
        XCTAssertEqual(verifiedJwsList.count, 1)
        let purchasedJws = try XCTUnwrap(verifiedJwsList.first)
        XCTAssertEqual(purchasedJws.split(separator: ".").count, 3)

        var restoredJwsList: [String] = []
        let restoreOutcome = await manager.restorePurchases { jws in
            restoredJwsList.append(jws)
        }
        XCTAssertEqual(restoreOutcome, .restored)
        XCTAssertEqual(restoredJwsList.count, 1)
        let restoredJws = try XCTUnwrap(restoredJwsList.first)
        XCTAssertEqual(restoredJws.split(separator: ".").count, 3)
    }

    func testDeckStepParserHandlesValidAndInvalidTokens() {
        XCTAssertEqual(DeckStepParser.parseTokens("  1m   10m\t1d "), ["1m", "10m", "1d"])
        XCTAssertTrue(DeckStepParser.isValidToken("30s"))
        XCTAssertTrue(DeckStepParser.isValidToken("10m"))
        XCTAssertTrue(DeckStepParser.isValidToken("2h"))
        XCTAssertTrue(DeckStepParser.isValidToken("1d"))
        XCTAssertTrue(DeckStepParser.isValidToken("15"))
        XCTAssertFalse(DeckStepParser.isValidToken(""))
        XCTAssertFalse(DeckStepParser.isValidToken("m"))
        XCTAssertFalse(DeckStepParser.isValidToken("10w"))
        XCTAssertFalse(DeckStepParser.isValidToken("abc"))
    }

    @MainActor
    func testUpdateDeckPersistsNameColorAndFullFsrsSettings() async throws {
        let (dir, store) = try makeTempStore()
        defer { try? FileManager.default.removeItem(at: dir) }

        let appState = AppState(store: store)
        let createdDeck = await appState.createDeck(name: "Systems", color: "#5a47e8")
        let created = try XCTUnwrap(createdDeck)
        XCTAssertEqual(created.settings.desiredRetention, 0.9, accuracy: 0.0001)
        XCTAssertEqual(created.settings.newPerDay, 20)
        XCTAssertEqual(created.settings.maxReviews, 200)
        XCTAssertEqual(created.settings.insertionOrder, .sequential)
        XCTAssertEqual(created.settings.leechAction, .suspend)

        let patch = DeckSettingsPatchDto(
            newPerDay: 30,
            maxReviews: 180,
            learnSteps: "1m 10m 1d",
            insertionOrder: .random,
            relearnSteps: "15m",
            minimumInterval: 2,
            leechThreshold: 5,
            leechAction: .tag,
            buryRelated: true,
            showTimer: false,
            desiredRetention: 0.93,
            maximumInterval: 36_500
        )

        let updatedDeck = await appState.updateDeck(
            id: created.id,
            name: "Distributed Systems",
            color: "#1e9b5e",
            settings: patch
        )
        let updated = try XCTUnwrap(updatedDeck)
        XCTAssertEqual(updated.name, "Distributed Systems")
        XCTAssertEqual(updated.color, "#1e9b5e")
        XCTAssertEqual(updated.desiredRetention, 0.93, accuracy: 0.0001)
        XCTAssertEqual(updated.newPerDay, 30)
        XCTAssertEqual(updated.maxReviews, 180)
        XCTAssertEqual(updated.settings.learnSteps, "1m 10m 1d")
        XCTAssertEqual(updated.settings.insertionOrder, .random)
        XCTAssertEqual(updated.settings.relearnSteps, "15m")
        XCTAssertEqual(updated.settings.minimumInterval, 2)
        XCTAssertEqual(updated.settings.leechThreshold, 5)
        XCTAssertEqual(updated.settings.leechAction, .tag)

        let reloadedDeck = try await store.getDeck(id: created.id)
        let reloaded = try XCTUnwrap(reloadedDeck)
        XCTAssertEqual(reloaded.name, "Distributed Systems")
        XCTAssertEqual(reloaded.color, "#1e9b5e")
        XCTAssertEqual(reloaded.settings, updated.settings)
        XCTAssertEqual(appState.decks.first(where: { $0.id == created.id })?.name, "Distributed Systems")

        let fsrsStatus = try await store.getDeckFsrsStatus(deckId: created.id)
        XCTAssertEqual(fsrsStatus.reviewCount, 0)
        XCTAssertFalse(fsrsStatus.canOptimize)
        XCTAssertFalse(fsrsStatus.hasPersonalizedWeights)
    }
}
