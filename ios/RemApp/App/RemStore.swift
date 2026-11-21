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

    func createDeck(name: String, color: String?, nowMs: Int64 = RemStore.nowMs()) throws -> DeckDto {
        try client.createDeck(name: name, color: color, nowMs: nowMs)
    }

    func updateDeck(id: String, name: String?, color: String?, nowMs: Int64 = RemStore.nowMs()) throws {
        try client.updateDeck(id: id, name: name, color: color, nowMs: nowMs)
    }

    func deleteDeck(id: String, nowMs: Int64 = RemStore.nowMs()) throws {
        try client.deleteDeck(id: id, nowMs: nowMs)
    }

    func listCards(deckId: String) throws -> [CardDto] {
        try client.listCards(deckId: deckId)
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

    func getStats(deckId: String?, days: UInt32, nowMs: Int64 = RemStore.nowMs()) throws -> StatsDto {
        try client.getStats(deckId: deckId, days: days, nowMs: nowMs)
    }

    func getStorageSummary() throws -> StorageSummaryDto {
        try client.getStorageSummary()
    }

    func exportBackupJson() throws -> String {
        try client.exportBackupJson()
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

    func syncStatus() throws -> SyncStatusDto {
        try client.syncStatus()
    }

    func syncNow(nowMs: Int64 = RemStore.nowMs()) throws -> SyncReportDto {
        try client.syncNow(nowMs: nowMs)
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
