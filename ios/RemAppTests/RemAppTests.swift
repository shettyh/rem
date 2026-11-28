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

        // allowlisted active -> hide Subscribe & hide Manage
        appState.entitlement = makeEntitlement(state: .active, syncEnabled: true, provider: "allowlist", expiresAt: nil)
        XCTAssertFalse(appState.canShowAppleSubscribe)
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
        XCTAssertEqual(created.settings.maxReviews, 9999)
        XCTAssertEqual(created.settings.insertionOrder, .sequential)
        XCTAssertEqual(created.settings.leechAction, .tag)

        let patch = DeckSettingsPatchDto(
            newPerDay: 30,
            maxReviews: 180,
            learnSteps: "1m 10m 1d",
            insertionOrder: .random,
            relearnSteps: "15m",
            minimumInterval: 2,
            leechThreshold: 5,
            leechAction: .suspend,
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
        XCTAssertEqual(updated.settings.leechAction, .suspend)

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

    func testCloudSyncFormatterHidesPrivateRelayAndFormatsRelativeTime() {
        XCTAssertNil(CloudSyncFormatter.visibleEmail("7w2pfz25mf@privaterelay.appleid.com"))
        XCTAssertNil(CloudSyncFormatter.visibleEmail("  USER@PrivateRelay.AppleID.com  "))
        XCTAssertNil(CloudSyncFormatter.visibleEmail("   "))
        XCTAssertNil(CloudSyncFormatter.visibleEmail(nil))
        XCTAssertEqual(CloudSyncFormatter.visibleEmail(" alice@example.com "), "alice@example.com")

        let now = Date(timeIntervalSince1970: 1_764_000_000)
        XCTAssertEqual(CloudSyncFormatter.relativeLastSynced(now, now: now), "just now")
        XCTAssertEqual(CloudSyncFormatter.relativeLastSynced(now.addingTimeInterval(2), now: now), "just now")
        XCTAssertEqual(CloudSyncFormatter.relativeLastSynced(now.addingTimeInterval(-59), now: now), "just now")
        XCTAssertNotEqual(CloudSyncFormatter.relativeLastSynced(now.addingTimeInterval(-120), now: now), "just now")

        XCTAssertFalse(CloudSyncFormatter.isCustomServer(
            serverUrl: "https://rem-sync.shettyh.fyi/",
            defaultServerUrl: "https://rem-sync.shettyh.fyi"
        ))
        XCTAssertTrue(CloudSyncFormatter.isCustomServer(
            serverUrl: "https://sync.example.com/",
            defaultServerUrl: "https://rem-sync.shettyh.fyi"
        ))
        XCTAssertEqual(CloudSyncFormatter.displayServerHost(serverUrl: "https://sync.example.com/"), "sync.example.com")
        XCTAssertEqual(CloudSyncFormatter.billingProviderDisplayName("apple"), "App Store")
        XCTAssertEqual(CloudSyncFormatter.billingProviderDisplayName("polar"), "Web")
        XCTAssertNil(CloudSyncFormatter.billingProviderDisplayName("trial"))
    }

    func testUndoLastGradeRemovesReviewLogFromStatsAndFsrsStatus() async throws {
        let (dir, store) = try makeTempStore()
        defer { try? FileManager.default.removeItem(at: dir) }

        let now: Int64 = 1_764_000_000_000
        let deck = try await store.createDeck(name: "Undo Stats", color: "#5a47e8", nowMs: now)
        _ = try await store.updateDeck(
            id: deck.id,
            name: nil,
            color: nil,
            settings: DeckSettingsPatchDto(
                newPerDay: 20,
                maxReviews: 200,
                learnSteps: "",
                insertionOrder: .sequential,
                relearnSteps: "",
                minimumInterval: 1,
                leechThreshold: 8,
                leechAction: .suspend,
                buryRelated: true,
                showTimer: false,
                desiredRetention: 0.9,
                maximumInterval: 36_500
            ),
            nowMs: now + 5
        )
        _ = try await store.createCard(
            deckId: deck.id,
            front: "Q1",
            back: "A1",
            tags: [],
            nowMs: now + 10
        )

        let session = try await store.startStudy(deckId: deck.id, nowMs: now + 100)
        _ = try session.reveal(nowMs: now + 150)
        _ = try session.grade(grade: .good, nowMs: now + 200)

        let statsAfterGrade = try await store.getStats(deckId: deck.id, days: 30, nowMs: now + 250)
        XCTAssertEqual(statsAfterGrade.reviews, 1)
        let fsrsAfterGrade = try await store.getDeckFsrsStatus(deckId: deck.id)
        XCTAssertEqual(fsrsAfterGrade.reviewCount, 1)

        _ = try session.undoLastGrade(nowMs: now + 300)

        let statsAfterUndo = try await store.getStats(deckId: deck.id, days: 30, nowMs: now + 350)
        XCTAssertEqual(statsAfterUndo.reviews, 0)
        let fsrsAfterUndo = try await store.getDeckFsrsStatus(deckId: deck.id)
        XCTAssertEqual(fsrsAfterUndo.reviewCount, 0)
    }

    func testCrossDeckSearchCustomStudyPreviewAndBackupImportRoundTrip() async throws {
        let (dir, store) = try makeTempStore()
        defer { try? FileManager.default.removeItem(at: dir) }

        let now: Int64 = 1_764_100_000_000
        let rust = try await store.createDeck(name: "Rust", color: "#5a47e8", nowMs: now)
        let sql = try await store.createDeck(name: "SQL", color: "#1e9b5e", nowMs: now + 1)
        _ = try await store.createCard(
            deckId: rust.id,
            front: "What does Pin<P> guarantee?",
            back: "Pointee cannot move in memory.",
            tags: ["async"],
            nowMs: now + 2
        )
        _ = try await store.createCard(
            deckId: sql.id,
            front: "What is WAL mode?",
            back: "Write-ahead logging for concurrent readers.",
            tags: ["sqlite"],
            nowMs: now + 3
        )

        // Cross-deck search finds the right card and deck metadata.
        let hits = try await store.searchCards(query: "Pin<P>", limit: 20)
        XCTAssertEqual(hits.count, 1)
        XCTAssertEqual(hits.first?.deckName, "Rust")
        XCTAssertEqual(hits.first?.card.tags, ["async"])

        // Custom study preview advances without changing schedules or review logs, and rejects suspend.
        let previewSession = try await store.startCustomStudy(
            deckId: rust.id,
            mode: .previewNew,
            amount: 5,
            nowMs: now + 10
        )
        let initial = try await store.studyCurrentView(session: previewSession, nowMs: now + 10)
        XCTAssertTrue(initial.preview)
        XCTAssertEqual(initial.remaining, 1)
        do {
            _ = try await store.studySuspendCurrent(session: previewSession, nowMs: now + 10)
            XCTFail("Expected preview session to reject suspend")
        } catch {
            XCTAssertTrue(error.remDisplayMessage.lowercased().contains("preview"))
        }
        let revealed = try await store.studyReveal(session: previewSession, nowMs: now + 11)
        XCTAssertTrue(revealed.revealed)
        XCTAssertNil(revealed.intervals)
        let finished = try await store.studyAdvancePreview(session: previewSession, nowMs: now + 12)
        XCTAssertNil(finished.current)
        XCTAssertEqual(finished.reviewed, 1)
        let statsAfterPreview = try await store.getStats(deckId: rust.id, days: 30, nowMs: now + 20)
        XCTAssertEqual(statsAfterPreview.reviews, 0)

        // Backup export -> preview -> import replaces matching deck names cleanly.
        let exported = try await store.exportBackupJson()
        let preview = try await store.previewBackupJson(json: exported)
        XCTAssertTrue(preview.added.isEmpty)
        XCTAssertEqual(Set(preview.replaced), Set(["Rust", "SQL"]))
        XCTAssertEqual(preview.totalCards, 2)

        let imported = try await store.importBackupJson(json: exported, nowMs: now + 30)
        XCTAssertEqual(Set(imported.replaced), Set(["Rust", "SQL"]))
        let hitsAfterImport = try await store.searchCards(query: "concurrent", limit: 10)
        XCTAssertEqual(hitsAfterImport.count, 1)
        XCTAssertEqual(hitsAfterImport.first?.deckName, "SQL")

        // Lenient parser accepts older backups with partial DeckSettings & legacy SM-2 scheduling.
        let legacyBackup = """
        {
          "format": "rem-backup",
          "version": 1,
          "decks": [{
            "name": "Legacy",
            "createdAt": 1000,
            "settings": { "newPerDay": 15 },
            "cards": [{
              "front": "Legacy Q",
              "back": "Legacy A",
              "createdAt": 1000,
              "updatedAt": 1000,
              "scheduling": { "repetitions": 2, "intervalDays": 6, "easeFactor": 2.5, "due": 2000 }
            }]
          }]
        }
        """
        let legacyPreview = try await store.previewBackupJson(json: legacyBackup)
        XCTAssertEqual(legacyPreview.added, ["Legacy"])
        XCTAssertEqual(legacyPreview.totalCards, 1)
        let legacyImported = try await store.importBackupJson(json: legacyBackup, nowMs: now + 40)
        XCTAssertEqual(legacyImported.added, ["Legacy"])
    }

    func testWave1DeckTreeClozeMathAndMarkdownAudio() {
        // DeckTreeBuilder: nested hierarchy, virtual folders, rolled-up counts, prefix safety
        XCTAssertTrue(DeckTreeBuilder.isDescendantOrSelf("CS::OS", of: "CS"))
        XCTAssertFalse(DeckTreeBuilder.isDescendantOrSelf("CSS", of: "CS"))

        let summaries: [DeckSummaryDto] = [
            DeckSummaryDto(id: "d1", name: "Languages::Spanish::Verbs", color: "#5a47e8", totalCards: 10, dueCount: 3, newDueCount: 1, masteredRatio: 0.5, desiredRetention: 0.9),
            DeckSummaryDto(id: "d2", name: "Languages::Spanish::Vocab", color: "#1e9b5e", totalCards: 5, dueCount: 2, newDueCount: 2, masteredRatio: 0.2, desiredRetention: 0.9),
            DeckSummaryDto(id: "d3", name: "Languages", color: "#d97914", totalCards: 4, dueCount: 1, newDueCount: 0, masteredRatio: 0.75, desiredRetention: 0.9),
        ]
        let roots = DeckTreeBuilder.build(from: summaries)
        XCTAssertEqual(roots.count, 1)
        XCTAssertEqual(roots[0].path, "Languages")
        XCTAssertEqual(roots[0].dueCount, 6)
        XCTAssertEqual(roots[0].newDueCount, 3)
        XCTAssertEqual(roots[0].totalCards, 19)
        XCTAssertEqual(roots[0].children.count, 1)
        XCTAssertNil(roots[0].children[0].deck) // Virtual folder "Languages::Spanish"
        XCTAssertEqual(roots[0].children[0].dueCount, 5)

        let collapsed = DeckTreeBuilder.flatten(roots, collapsed: ["Languages::Spanish"])
        XCTAssertEqual(collapsed.map(\.path), ["Languages", "Languages::Spanish"])

        // ClozeRenderer: nested LaTeX braces, hints, rendering & formatter
        let clozeText = "Euler: {{c1::$e^{i\\pi} + 1 = 0$::identity}} and {{c2::42}}"
        XCTAssertEqual(ClozeRenderer.extractIndices(clozeText), [1, 2])
        XCTAssertEqual(ClozeRenderer.nextIndex(in: clozeText), 3)
        XCTAssertEqual(ClozeRenderer.stripMarkup(clozeText), "Euler: $e^{i\\pi} + 1 = 0$ and 42")

        let q1 = ClozeRenderer.render(clozeText, activeIndex: 1, revealed: false)
        XCTAssertTrue(q1.contains("[identity]"))
        XCTAssertTrue(q1.contains("42"))
        XCTAssertFalse(q1.contains("{{c1"))

        // ClozeRenderer skips inline backtick code spans
        let codeCloze = "Literal `{{c1::ignored}}` and {{c1::active}}"
        XCTAssertEqual(ClozeRenderer.extractIndices(codeCloze), [1])
        let codeClozeQ = ClozeRenderer.render(codeCloze, activeIndex: 1, revealed: false)
        XCTAssertTrue(codeClozeQ.contains("`{{c1::ignored}}`"))
        XCTAssertFalse(codeClozeQ.contains("active"))

        // RubyFormatter strips `{base|reading}` outside code spans and GFM table cells preserve escaped `\|`
        XCTAssertEqual(
            RubyFormatter.stripMarkup("{答|こた}えに **{丸|まる}** and `{残|のこ}`"),
            "答えに **丸** and `{残|のこ}`"
        )
        XCTAssertEqual(
            MarkdownBlockParser.plainTextPreview("{答|こた}えに **{丸|まる}** をつける"),
            "答えに 丸 をつける"
        )
        let rubyTable = MarkdownBlockParser.parse("| Kanji | Meaning |\n| --- | --- |\n| {日\\|にち} | day |")
        XCTAssertEqual(rubyTable, [.table(headers: ["Kanji", "Meaning"], rows: [["{日|にち}", "day"]])])

        let formattedCloze = MarkdownFormatter.apply(
            .cloze(index: 2),
            to: "Capital of France is Paris.",
            selectedRange: NSRange(location: 21, length: 5)
        )
        XCTAssertEqual(formattedCloze.text, "Capital of France is {{c2::Paris}}.")

        // LatexMathFormatter + MarkdownBlockParser
        let normalized = LatexMathFormatter.normalizeDelimiters("Inline \\(x^2\\) and `\\(code\\)`\n\\[\\frac{a}{b}\\]")
        XCTAssertEqual(normalized, "Inline $x^2$ and `\\(code\\)`\n$$\\frac{a}{b}$$")
        XCTAssertEqual(LatexMathFormatter.formatUnicode("\\alpha + x^2 \\leq \\infty"), "α + x² ≤ ∞")
        // Regression checks for command prefix collisions & nested commands
        XCTAssertEqual(LatexMathFormatter.formatUnicode("\\left(x + 1\\right)"), "(x + 1)")
        XCTAssertEqual(LatexMathFormatter.formatUnicode("\\neg P + a_1 + \\cdots + a_n"), "¬ P + a₁ + ⋯ + aₙ")
        XCTAssertEqual(LatexMathFormatter.formatUnicode("\\textbf{hi} \\text{world}"), "\\textbf{hi} world")
        XCTAssertEqual(LatexMathFormatter.formatUnicode("\\frac{1}{\\frac{2}{3}}"), "1/(2/3)")
        let frac = LatexMathFormatter.parseTopLevelFraction("\\frac{1}{2}")
        XCTAssertEqual(frac?.0, "1")
        XCTAssertEqual(frac?.1, "2")

        // Regression check: inline $$...$$ with trailing text or $$$$ must not swallow subsequent paragraphs
        let trailingMathBlocks = MarkdownBlockParser.parse("$$E = mc^2$$ is Einstein's formula.\n\nNext paragraph.")
        XCTAssertEqual(trailingMathBlocks.count, 2)
        let emptyDollarBlocks = MarkdownBlockParser.parse("$$$$\n\nKeep me.")
        XCTAssertEqual(emptyDollarBlocks.count, 2)

        let audioHash = String(repeating: "c", count: 64)
        let blocks = MarkdownBlockParser.parse("$$\nE = mc^2\n$$\n\n[Audio: pron.mp3](asset:\(audioHash))")
        XCTAssertEqual(blocks, [
            .mathBlock(latex: "E = mc^2"),
            .audio(label: "pron.mp3", hash: audioHash),
        ])
        // Audio asset removal removes `[Audio: ...](asset:...)` as well as `![...](asset:...)`
        let strippedAudio = MarkdownFormatter.removeAssetReference(
            hash: audioHash,
            from: "Hello\n[Audio: pron.mp3](asset:\(audioHash))"
        )
        XCTAssertEqual(strippedAudio, "Hello")
    }

    func testWave1StoreClozeSiblingsTimerAndNestedDecks() async throws {
        let (dir, store) = try makeTempStore()
        defer { try? FileManager.default.removeItem(at: dir) }

        let now: Int64 = 1_764_200_000_000
        let parent = try await store.createDeck(name: "Bio", color: "#5a47e8", nowMs: now)
        let child = try await store.createDeck(name: "Bio::Cells", color: "#1e9b5e", nowMs: now + 1)

        // Enable showTimer and buryRelated on child deck
        _ = try await store.updateDeck(
            id: child.id,
            name: nil,
            color: nil,
            settings: DeckSettingsPatchDto(
                newPerDay: 20,
                maxReviews: 200,
                learnSteps: "",
                insertionOrder: .sequential,
                relearnSteps: "",
                minimumInterval: 1,
                leechThreshold: 8,
                leechAction: .tag,
                buryRelated: true,
                showTimer: true,
                desiredRetention: 0.9,
                maximumInterval: 36_500
            ),
            nowMs: now + 2
        )

        let clozeCards = try await store.createCardOrCloze(
            deckId: child.id,
            front: "{{c1::Mitochondria}} make {{c2::ATP}}.",
            back: "Cell bio",
            tags: ["bio"],
            nowMs: now + 5
        )
        XCTAssertEqual(clozeCards.count, 2)
        XCTAssertEqual(clozeCards[0].siblingGroupId, clozeCards[1].siblingGroupId)
        XCTAssertEqual(clozeCards.compactMap(\.clozeIndex), [1, 2])

        // Updating cloze note propagates to siblings and creates c3
        let updated = try await store.updateClozeNote(
            cardId: clozeCards[0].id,
            front: "{{c1::Mitochondria}} make {{c2::ATP}} via {{c3::respiration}}.",
            back: "Updated extra",
            tags: ["bio", "cells"],
            nowMs: now + 10
        )
        XCTAssertEqual(updated.count, 3)

        // Studying parent includes child cards, surfaces showTimer, and buries siblings after grading c1
        let session = try await store.startStudy(deckId: parent.id, nowMs: now + 20)
        let initial = try await store.studyCurrentView(session: session, nowMs: now + 20)
        XCTAssertTrue(initial.showTimer)
        XCTAssertEqual(initial.current?.clozeIndex, 1)

        _ = try await store.studyReveal(session: session, nowMs: now + 25)
        let graded = try await store.studyGrade(session: session, grade: .easy, nowMs: now + 30)
        // Siblings c2 & c3 buried for today!
        XCTAssertNil(graded.view.current)
    }

    func testImageOcclusionParsingPreviewCompositeAndSiblingLifecycle() async throws {
        let (dir, store) = try makeTempStore()
        defer { try? FileManager.default.removeItem(at: dir) }

        let now: Int64 = 1_764_300_000_000
        let deck = try await store.createDeck(name: "Systems", color: "#5a47e8", nowMs: now)
        let renderer = UIGraphicsImageRenderer(size: CGSize(width: 64, height: 64))
        let img = renderer.image { ctx in
            UIColor.systemBlue.setFill()
            ctx.fill(CGRect(x: 0, y: 0, width: 64, height: 64))
        }
        let pngBytes = try XCTUnwrap(img.pngData())
        let hash = try await store.putAsset(mime: "image/png", bytes: pngBytes, nowMs: now + 1)

        let block = OcclusionBlock(
            v: 1,
            image: "asset:\(hash)",
            mode: .hideAll,
            alt: "TCP Header",
            masks: [
                OcclusionMask(id: 1, type: .rect, x: 0.1, y: 0.1, w: 0.3, h: 0.2),
                OcclusionMask(id: 1, type: .ellipse, x: 0.5, y: 0.1, w: 0.3, h: 0.2),
                OcclusionMask(id: 2, type: .rect, x: 0.1, y: 0.5, w: 0.8, h: 0.2),
            ]
        )
        let front = ImageOcclusionParser.composeDocument(header: "TCP Header", block: block)

        XCTAssertTrue(ImageOcclusionParser.hasOcclusion(front))
        XCTAssertEqual(ImageOcclusionParser.extractIndices(front), [1, 2])
        let parsedBlocks = MarkdownBlockParser.parse(front)
        XCTAssertEqual(parsedBlocks.count, 2)
        if case .imageOcclusion(let parsed) = parsedBlocks[1] {
            XCTAssertEqual(parsed.masks.count, 3)
            XCTAssertEqual(parsed.mode, .hideAll)
        } else {
            XCTFail("Expected .imageOcclusion block")
        }

        let preview = MarkdownBlockParser.plainTextPreview(front)
        XCTAssertTrue(preview.contains("TCP Header"))
        XCTAssertFalse(preview.contains("\"masks\""))

        let composite = ImageOcclusionParser.renderCompositeImage(
            base: img,
            block: block,
            activeIndex: 1,
            revealed: false,
            peekAll: false
        )
        XCTAssertEqual(composite.size.width * composite.scale, img.size.width * img.scale)
        XCTAssertEqual(composite.size.height * composite.scale, img.size.height * img.scale)
        XCTAssertLessThanOrEqual(max(composite.size.width, composite.size.height), 2048)

        let created = try await store.createCardOrCloze(
            deckId: deck.id,
            front: front,
            back: "RFC 9293",
            tags: ["networking"],
            nowMs: now + 5
        )
        XCTAssertEqual(created.count, 2)
        XCTAssertEqual(created[0].siblingGroupId, created[1].siblingGroupId)
        XCTAssertEqual(created.compactMap(\.clozeIndex), [1, 2])

        // Lossy JSON parsing preserves valid masks when invalid/future elements exist,
        // and tilde fences (`~~~rem-occlusion`) parse into `.imageOcclusion`.
        let mixedPayload = """
        ~~~rem-occlusion extra-info
        {"v":1,"image":"asset:\(hash)","mode":"future","masks":[{"id":-1,"x":0.1,"y":0.1,"w":0.2,"h":0.2},{"id":1,"type":"polygon","x":0.1,"y":0.1,"w":0.2,"h":0.2},{"id":2,"x":null,"y":0.1,"w":0.2,"h":0.2}]}
        ~~~
        """
        let tildeBlocks = MarkdownBlockParser.parse(mixedPayload)
        XCTAssertEqual(tildeBlocks.count, 1)
        if case .imageOcclusion(let parsedMixed) = tildeBlocks[0] {
            XCTAssertEqual(parsedMixed.masks.count, 1)
            XCTAssertEqual(parsedMixed.masks[0].id, 1)
            XCTAssertEqual(parsedMixed.masks[0].type, .rect)
        } else {
            XCTFail("Expected .imageOcclusion from tilde fence")
        }

        // Clearing header with syncAltFromHeader removes stale alt
        let cleared = ImageOcclusionParser.composeDocument(
            header: "",
            block: block,
            syncAltFromHeader: true
        )
        XCTAssertNil(ImageOcclusionParser.extractDocument(cleared)?.block.alt)
    }

    func testDeckTreeBuilderDuplicatesSubtreeSummaryAndDeleteTreeLifecycle() async throws {
        // 1. Strict descendant matching excludes self and non-matching prefixes
        XCTAssertFalse(DeckTreeBuilder.isStrictDescendant("CS", of: "CS"))
        XCTAssertTrue(DeckTreeBuilder.isStrictDescendant("CS::Rust", of: "CS"))
        XCTAssertFalse(DeckTreeBuilder.isStrictDescendant("CSS", of: "CS"))

        // 2. Duplicate-named decks are preserved as siblings instead of overwriting
        let summaries: [DeckSummaryDto] = [
            DeckSummaryDto(id: "d1", name: "Anatomy::Unit 01::Cells", color: "#5a47e8", totalCards: 10, dueCount: 3, newDueCount: 1, masteredRatio: 0.5, desiredRetention: 0.9),
            DeckSummaryDto(id: "d2", name: "Anatomy::Unit 02::Bones", color: "#1e9b5e", totalCards: 15, dueCount: 2, newDueCount: 2, masteredRatio: 0.2, desiredRetention: 0.9),
            DeckSummaryDto(id: "d3", name: "CS", color: "#d97914", totalCards: 4, dueCount: 1, newDueCount: 0, masteredRatio: 0.75, desiredRetention: 0.9),
            DeckSummaryDto(id: "d4", name: "CS::Rust", color: "#3ba0e8", totalCards: 6, dueCount: 2, newDueCount: 1, masteredRatio: 0.5, desiredRetention: 0.9),
            DeckSummaryDto(id: "d5", name: "CS", color: "#e8638c", totalCards: 2, dueCount: 0, newDueCount: 0, masteredRatio: 1.0, desiredRetention: 0.9),
        ]
        let roots = DeckTreeBuilder.build(from: summaries)
        XCTAssertEqual(roots.count, 3)

        // Virtual parent folder "Anatomy"
        let anatomy = roots[0]
        XCTAssertNil(anatomy.deck)
        XCTAssertEqual(anatomy.subtreeDeckIds, ["d1", "d2"])
        XCTAssertEqual(anatomy.subtreeDeckCount, 2)
        XCTAssertEqual(anatomy.subdeckCount, 2)
        XCTAssertEqual(anatomy.subtreeCardCount, 25)
        let anatomyDelete = DeckTreeBuilder.describeDelete(node: anatomy)
        XCTAssertEqual(anatomyDelete.title, "Delete \"Anatomy\"?")
        XCTAssertTrue(anatomyDelete.message.contains("2 subdecks and 25 cards"))
        XCTAssertEqual(anatomyDelete.confirmLabel, "Delete all")

        // Real parent deck "CS" and duplicate sibling "CS"
        let csPrimary = roots[1]
        let csDup = roots[2]
        XCTAssertEqual(csPrimary.deck?.id, "d3")
        XCTAssertEqual(csPrimary.subdeckCount, 1)
        XCTAssertEqual(csPrimary.subtreeCardCount, 10)
        XCTAssertEqual(csDup.deck?.id, "d5")
        XCTAssertEqual(csDup.subdeckCount, 0)
        XCTAssertEqual(csDup.subtreeCardCount, 2)

        // Collapsed path pruning
        let pruned = DeckTreeBuilder.pruneCollapsedPaths(
            ["Anatomy", "Anatomy::Unit 01", "Anatomy2", "CS"],
            removing: ["Anatomy"]
        )
        XCTAssertEqual(pruned, ["Anatomy2", "CS"])

        // 3. End-to-end store: cascading deleteDeck, virtual-folder deleteDeckTree, and sweepOrphanAssets
        let (dir, store) = try makeTempStore()
        defer { try? FileManager.default.removeItem(at: dir) }

        let now: Int64 = 1_764_400_000_000
        let parent = try await store.createDeck(name: "Bio", color: "#5a47e8", nowMs: now)
        let child = try await store.createDeck(name: "Bio::Cells", color: "#1e9b5e", nowMs: now + 1)
        let dupSibling = try await store.createDeck(name: "Bio", color: "#d97914", nowMs: now + 2)
        let prefixCollision = try await store.createDeck(name: "Bio2::Lab", color: "#3ba0e8", nowMs: now + 3)

        let v1 = try await store.createDeck(name: "JLPT::N5::Vocab", color: "#5a47e8", nowMs: now + 4)
        let v2 = try await store.createDeck(name: "JLPT::N4::Kanji", color: "#1e9b5e", nowMs: now + 5)

        let doomedHash = try await store.putAsset(mime: "image/png", bytes: Data([1, 2, 3]), nowMs: now + 6)
        let keptHash = try await store.putAsset(mime: "image/png", bytes: Data([4, 5, 6]), nowMs: now + 7)
        _ = try await store.createCard(deckId: v1.id, front: "![](asset:\(doomedHash))", back: "a", tags: [], nowMs: now + 8)
        _ = try await store.createCard(deckId: dupSibling.id, front: "![](asset:\(keptHash))", back: "b", tags: [], nowMs: now + 9)
        _ = try await store.createCard(deckId: child.id, front: "Cell", back: "Unit", tags: [], nowMs: now + 10)

        // Cascading deleteDeck removes parent + child, keeps duplicate sibling + prefix collision
        try await store.deleteDeck(id: parent.id, nowMs: now + 20)
        let afterParentDelete = Set(try await store.listDecks().map(\.id))
        XCTAssertFalse(afterParentDelete.contains(parent.id))
        XCTAssertFalse(afterParentDelete.contains(child.id))
        XCTAssertTrue(afterParentDelete.contains(dupSibling.id))
        XCTAssertTrue(afterParentDelete.contains(prefixCollision.id))

        // deleteDeckTree removes virtual parent folder "JLPT" and all its subdecks
        let deletedTreeIds = try await store.deleteDeckTree(path: "JLPT", nowMs: now + 30)
        XCTAssertEqual(Set(deletedTreeIds), Set([v1.id, v2.id]))
        let remainingIds = Set(try await store.listDecks().map(\.id))
        XCTAssertEqual(remainingIds, Set([dupSibling.id, prefixCollision.id]))

        // Orphan asset sweep deletes asset from deleted JLPT card and preserves surviving asset
        let swept = try await store.sweepOrphanAssets()
        XCTAssertEqual(swept, 1)
        let fetchedDoomed = try await store.getAsset(hash: doomedHash)
        let fetchedKept = try await store.getAsset(hash: keptHash)
        XCTAssertNil(fetchedDoomed)
        XCTAssertNotNil(fetchedKept)
    }

    @MainActor
    func testDeckImportRequestCoordinatesSheetDismissalAndAlertFormatting() throws {
        let (dir, store) = try makeTempStore()
        defer { try? FileManager.default.removeItem(at: dir) }

        let appState = AppState(store: store)

        // Direct request when no sheet is open activates the file importer immediately.
        appState.requestDeckImport(.apkg)
        XCTAssertEqual(appState.activeImportPicker, .apkg)
        XCTAssertNil(appState.deferredImportPicker)

        // Re-entrant request while picker is already active is ignored.
        appState.requestDeckImport(.backup)
        XCTAssertEqual(appState.activeImportPicker, .apkg)

        appState.activeImportPicker = nil

        // Request from inside NewDeckSheet dismisses the sheet and defers picker until onDismiss.
        appState.showNewDeckSheet = true
        appState.requestDeckImport(.backup)
        XCTAssertFalse(appState.showNewDeckSheet)
        XCTAssertNil(appState.activeImportPicker)
        XCTAssertEqual(appState.deferredImportPicker, .backup)
        XCTAssertFalse(appState.canRequestDeckImport)

        // Rapid second tap during dismissal animation must not wedge activeImportPicker.
        appState.requestDeckImport(.apkg)
        XCTAssertNil(appState.activeImportPicker)
        XCTAssertEqual(appState.deferredImportPicker, .backup)

        appState.presentDeferredImportIfNeeded(delayMs: 0)
        XCTAssertNil(appState.deferredImportPicker)
        XCTAssertEqual(appState.activeImportPicker, .backup)

        // Busy import or competing modal guards block new picker requests.
        appState.activeImportPicker = nil
        appState.isImportingApkg = true
        appState.requestDeckImport(.backup)
        XCTAssertNil(appState.activeImportPicker)
        appState.isImportingApkg = false

        appState.editingDeckRequest = DeckEditRequest(deckId: "deck-1", onDeleted: nil)
        appState.requestDeckImport(.apkg)
        XCTAssertNil(appState.activeImportPicker)
        appState.editingDeckRequest = nil

        // Cancel pending .apkg removes staged temp file cleanly.
        let stagedURL = dir.appendingPathComponent("staged-test.apkg")
        try Data([0x50, 0x4B]).write(to: stagedURL)
        let pending = PendingApkgImport(
            tempFileURL: stagedURL,
            preview: ApkgImportPreviewDto(
                added: ["Test"],
                replaced: [],
                totalCards: 1,
                mediaCount: 0,
                reviewLogs: 0,
                warnings: []
            )
        )
        appState.pendingApkgImport = pending
        appState.cancelPendingApkgImport(pending)
        XCTAssertNil(appState.pendingApkgImport)
        XCTAssertFalse(FileManager.default.fileExists(atPath: stagedURL.path))

        // Alert message formatting handles added, replaced, media, review logs, and warnings.
        let cleanPreview = ApkgImportPreviewDto(
            added: ["Japanese"],
            replaced: [],
            totalCards: 12,
            mediaCount: 2,
            reviewLogs: 5,
            warnings: []
        )
        XCTAssertEqual(
            AppState.apkgImportAlertMessage(cleanPreview),
            "Found 1 deck, 12 cards, 2 media, 5 review logs."
        )

        let replacePreview = ApkgImportPreviewDto(
            added: ["Spanish::Verbs"],
            replaced: ["Spanish"],
            totalCards: 1,
            mediaCount: 0,
            reviewLogs: 0,
            warnings: ["Skipped 1 empty note."]
        )
        XCTAssertEqual(
            AppState.apkgImportAlertMessage(replacePreview),
            "Found 2 decks, 1 card. Existing deck(s) will be replaced: Spanish.\n\nNote: Skipped 1 empty note."
        )
    }

    @MainActor
    func testSyncPokeClientURLRequestParsingAndSeqGating() throws {
        XCTAssertEqual(
            SyncPokeClient.websocketURL(from: "https://rem-sync.shettyh.fyi/")?.absoluteString,
            "wss://rem-sync.shettyh.fyi/v1/sync/ws"
        )
        XCTAssertEqual(
            SyncPokeClient.websocketURL(from: "http://127.0.0.1:8787")?.absoluteString,
            "ws://127.0.0.1:8787/v1/sync/ws"
        )
        XCTAssertEqual(
            SyncPokeClient.websocketURL(from: "https://sync.example.com/rem/")?.absoluteString,
            "wss://sync.example.com/rem/v1/sync/ws"
        )
        XCTAssertNil(SyncPokeClient.websocketURL(from: "ftp://example.com"))
        XCTAssertNil(SyncPokeClient.websocketURL(from: "   "))

        let auth = SyncPokeAuth(
            serverURL: "https://rem-sync.shettyh.fyi",
            deviceId: "dev-iphone-1",
            accessToken: "jwt-secret-token"
        )
        let request = try XCTUnwrap(SyncPokeClient.makeRequest(auth: auth))
        XCTAssertEqual(request.url?.absoluteString, "wss://rem-sync.shettyh.fyi/v1/sync/ws")
        XCTAssertNil(request.url?.query)
        XCTAssertFalse(request.url?.absoluteString.contains("jwt-secret-token") ?? true)
        XCTAssertEqual(request.value(forHTTPHeaderField: "Authorization"), "Bearer jwt-secret-token")
        XCTAssertEqual(request.value(forHTTPHeaderField: "X-Rem-Device-Id"), "dev-iphone-1")

        XCTAssertEqual(
            SyncPokeMessage.parse(#"{"type":"poke","latestSeq":42}"#),
            .poke(latestSeq: 42)
        )
        XCTAssertEqual(SyncPokeMessage.parse("pong"), .pong)
        XCTAssertEqual(SyncPokeMessage.parse(#"{"type":"other","latestSeq":42}"#), .ignored)
        XCTAssertEqual(SyncPokeMessage.parse(#"{"type":"poke","latestSeq":-1}"#), .ignored)
        XCTAssertEqual(SyncPokeMessage.parse("not json"), .ignored)

        XCTAssertTrue(SyncPokeClient.shouldTriggerSync(pokeLatestSeq: 42, localLastSeq: 41))
        XCTAssertFalse(SyncPokeClient.shouldTriggerSync(pokeLatestSeq: 42, localLastSeq: 42))
        XCTAssertFalse(SyncPokeClient.shouldTriggerSync(pokeLatestSeq: 41, localLastSeq: 42))

        XCTAssertEqual(SyncPokeClient.backoffDelayNanoseconds(attempt: 0), 1_000_000_000)
        XCTAssertEqual(SyncPokeClient.backoffDelayNanoseconds(attempt: 1), 2_000_000_000)
        XCTAssertEqual(SyncPokeClient.backoffDelayNanoseconds(attempt: 4), 16_000_000_000)
        XCTAssertEqual(SyncPokeClient.backoffDelayNanoseconds(attempt: 5), 30_000_000_000)
        XCTAssertEqual(SyncPokeClient.backoffDelayNanoseconds(attempt: 20), 30_000_000_000)
    }
}
