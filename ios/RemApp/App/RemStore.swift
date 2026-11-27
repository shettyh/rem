import Foundation

/// Serial background actor wrapping Rust's `RemClient` so all SQLite and sync I/O stays off `@MainActor`.
actor RemStore {
    private let client: RemClient

    init(dbPath: String, credentialStore: FfiCredentialStore = KeychainCredentialStore()) throws {
        self.client = try RemClient.open(dbPath: dbPath, credentialStore: credentialStore)
    }

    static func defaultDatabasePath() throws -> String {
        let fileManager = FileManager.default
        let appSupport = try fileManager.url(
            for: .applicationSupportDirectory,
            in: .userDomainMask,
            appropriateFor: nil,
            create: true
        )
        let dir = appSupport.appendingPathComponent("rem", isDirectory: true)
        try fileManager.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir.appendingPathComponent("collection.sqlite3").path
    }

    static func nowMs() -> Int64 {
        Int64(Date().timeIntervalSince1970 * 1000)
    }

    func defaultCloudServerUrl() -> String {
        client.defaultCloudServerUrl()
    }

    func getTodaySummary(nowMs: Int64 = RemStore.nowMs()) throws -> TodaySummaryDto {
        try client.getTodaySummary(nowMs: nowMs)
    }

    func listDecks() throws -> [DeckDto] {
        try client.listDecks()
    }

    func getDeck(id: String) throws -> DeckDto? {
        try client.getDeck(id: id)
    }

    func createDeck(name: String, color: String?, nowMs: Int64 = RemStore.nowMs()) throws -> DeckDto {
        try client.createDeck(name: name, color: color, nowMs: nowMs)
    }

    func updateDeck(
        id: String,
        name: String?,
        color: String?,
        settings: DeckSettingsPatchDto? = nil,
        nowMs: Int64 = RemStore.nowMs()
    ) throws -> DeckDto {
        try client.updateDeck(id: id, name: name, color: color, settings: settings, nowMs: nowMs)
    }

    func getDeckFsrsStatus(deckId: String) throws -> DeckFsrsStatusDto {
        try client.getDeckFsrsStatus(deckId: deckId)
    }

    func optimizeDeckFsrs(deckId: String, nowMs: Int64 = RemStore.nowMs()) throws -> DeckDto {
        try client.optimizeDeckFsrs(deckId: deckId, nowMs: nowMs)
    }

    func resetDeckFsrs(deckId: String, nowMs: Int64 = RemStore.nowMs()) throws -> DeckDto {
        try client.resetDeckFsrs(deckId: deckId, nowMs: nowMs)
    }

    func deleteDeck(id: String, nowMs: Int64 = RemStore.nowMs()) throws {
        try client.deleteDeck(id: id, nowMs: nowMs)
    }

    func listCards(deckId: String) throws -> [CardDto] {
        try client.listCards(deckId: deckId)
    }

    func searchCards(
        query: String,
        deckId: String? = nil,
        limit: UInt32 = 50
    ) throws -> [CardSearchHitDto] {
        try client.searchCards(query: query, deckId: deckId, limit: limit)
    }

    func getCard(id: String) throws -> CardDto? {
        try client.getCard(id: id)
    }

    func createCard(
        deckId: String,
        front: String,
        back: String,
        tags: [String],
        nowMs: Int64 = RemStore.nowMs()
    ) throws -> CardDto {
        try client.createCard(deckId: deckId, front: front, back: back, tags: tags, nowMs: nowMs)
    }

    func updateCard(
        id: String,
        front: String?,
        back: String?,
        tags: [String]?,
        suspended: Bool?,
        nowMs: Int64 = RemStore.nowMs()
    ) throws {
        try client.updateCard(
            id: id,
            front: front,
            back: back,
            tags: tags,
            suspended: suspended,
            nowMs: nowMs
        )
    }

    func deleteCard(id: String, nowMs: Int64 = RemStore.nowMs()) throws {
        try client.deleteCard(id: id, nowMs: nowMs)
    }

    func putAsset(mime: String, bytes: Data, nowMs: Int64 = RemStore.nowMs()) throws -> String {
        try client.putAsset(mime: mime, bytes: bytes, nowMs: nowMs)
    }

    func getAsset(hash: String) throws -> AssetDataDto? {
        try client.getAsset(hash: hash)
    }

    func startStudy(deckId: String?, nowMs: Int64 = RemStore.nowMs()) throws -> RemStudySession {
        try client.startStudy(deckId: deckId, nowMs: nowMs)
    }

    func startCustomStudy(
        deckId: String,
        mode: CustomStudyModeDto,
        amount: UInt32,
        nowMs: Int64 = RemStore.nowMs()
    ) throws -> RemStudySession {
        try client.startCustomStudy(deckId: deckId, mode: mode, amount: amount, nowMs: nowMs)
    }

    func studyCurrentView(
        session: RemStudySession,
        nowMs: Int64 = RemStore.nowMs()
    ) throws -> StudyViewDto {
        try session.currentView(nowMs: nowMs)
    }

    func studyReveal(
        session: RemStudySession,
        nowMs: Int64 = RemStore.nowMs()
    ) throws -> StudyViewDto {
        try session.reveal(nowMs: nowMs)
    }

    func studyGrade(
        session: RemStudySession,
        grade: GradeDto,
        nowMs: Int64 = RemStore.nowMs()
    ) throws -> StudyGradeOutcomeDto {
        try session.grade(grade: grade, nowMs: nowMs)
    }

    func studyAdvancePreview(
        session: RemStudySession,
        nowMs: Int64 = RemStore.nowMs()
    ) throws -> StudyViewDto {
        try session.advancePreview(nowMs: nowMs)
    }

    func studyUndoLastGrade(
        session: RemStudySession,
        nowMs: Int64 = RemStore.nowMs()
    ) throws -> StudyViewDto {
        try session.undoLastGrade(nowMs: nowMs)
    }

    func studySuspendCurrent(
        session: RemStudySession,
        nowMs: Int64 = RemStore.nowMs()
    ) throws -> StudyViewDto {
        try session.suspendCurrent(nowMs: nowMs)
    }

    func studyReloadCurrent(
        session: RemStudySession,
        nowMs: Int64 = RemStore.nowMs()
    ) throws -> StudyViewDto {
        try session.reloadCurrent(nowMs: nowMs)
    }

    func getStats(deckId: String?, days: UInt32, nowMs: Int64 = RemStore.nowMs()) throws -> StatsDto {
        try client.getStats(deckId: deckId, days: days, nowMs: nowMs)
    }

    func getStorageSummary() throws -> StorageSummaryDto {
        try client.getStorageSummary()
    }

    func exportBackupJson() throws -> String {
        try client.exportBackupJson()
    }

    func previewBackupJson(json: String) throws -> BackupPreviewDto {
        try client.previewBackupJson(json: json)
    }

    func importBackupJson(json: String, nowMs: Int64 = RemStore.nowMs()) throws -> ImportResultDto {
        try client.importBackupJson(json: json, nowMs: nowMs)
    }

    func fetchAuthProviders(serverUrl: String) throws -> AuthProvidersDto {
        try client.fetchAuthProviders(serverUrl: serverUrl)
    }

    func beginOauth(serverUrl: String, provider: OAuthProviderDto) throws -> OAuthStartDto {
        try client.beginOauth(serverUrl: serverUrl, provider: provider)
    }

    func finishOauth(
        serverUrl: String,
        code: String,
        pkceVerifier: String,
        resetExistingBinding: Bool,
        nowMs: Int64 = RemStore.nowMs()
    ) throws -> ConnectResultDto {
        try client.finishOauth(
            serverUrl: serverUrl,
            code: code,
            pkceVerifier: pkceVerifier,
            resetExistingBinding: resetExistingBinding,
            nowMs: nowMs
        )
    }

    func finishAppleNativeAuth(
        serverUrl: String,
        identityToken: String,
        authorizationCode: String?,
        nonce: String,
        email: String?,
        resetExistingBinding: Bool,
        nowMs: Int64 = RemStore.nowMs()
    ) throws -> ConnectResultDto {
        try client.finishAppleNativeAuth(
            serverUrl: serverUrl,
            identityToken: identityToken,
            authorizationCode: authorizationCode,
            nonce: nonce,
            email: email,
            resetExistingBinding: resetExistingBinding,
            nowMs: nowMs
        )
    }

    func syncStatus() throws -> SyncStatusDto {
        try client.syncStatus()
    }

    func syncNow(nowMs: Int64 = RemStore.nowMs()) throws -> SyncReportDto {
        try client.syncNow(nowMs: nowMs)
    }

    func fetchAccount(nowMs: Int64 = RemStore.nowMs()) throws -> AccountProfileDto {
        try client.fetchAccount(nowMs: nowMs)
    }

    func startTrial(nowMs: Int64 = RemStore.nowMs()) throws -> AccountProfileDto {
        try client.startTrial(nowMs: nowMs)
    }

    func verifyAppleTransaction(
        signedTransactionInfo: String,
        nowMs: Int64 = RemStore.nowMs()
    ) throws -> AccountProfileDto {
        try client.verifyAppleTransaction(
            signedTransactionInfo: signedTransactionInfo,
            nowMs: nowMs
        )
    }

    func signOut() throws {
        try client.signOut()
    }

    func deleteCloudAccount(nowMs: Int64 = RemStore.nowMs()) throws {
        try client.deleteCloudAccount(nowMs: nowMs)
    }
}

extension Error {
    /// Clean human-readable message for UI surfaces (avoids UniFFI's default `String(reflecting:)` enum dump).
    var remDisplayMessage: String {
        if let ffi = self as? RemFfiError {
            switch ffi {
            case .Storage(let message),
                 .InvalidInput(let message),
                 .NotFound(let message),
                 .Conflict(let message),
                 .NotConfigured(let message),
                 .NotAuthenticated(let message),
                 .Network(let message),
                 .CredentialStore(let message),
                 .Sync(let message):
                return message
            case .AccountMismatch(let boundServerUrl, let boundAccountId, _, _):
                return "This device is bound to another account (\(boundAccountId.prefix(8))…) on \(boundServerUrl)."
            case .SubscriptionRequired(let message, _):
                return message
            }
        }
        return localizedDescription
    }
}
