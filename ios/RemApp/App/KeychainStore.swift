import Foundation
import Security

/// iOS Keychain implementation of `FfiCredentialStore` for `rem-sync`.
/// Tokens live exclusively in Keychain and never touch SQLite, backups, or UserDefaults.
final class KeychainCredentialStore: FfiCredentialStore, @unchecked Sendable {
    private let service: String

    init(service: String = "com.shettyh.rem.sync") {
        self.service = service
    }

    func load(serverUrl: String, accountId: String) throws -> String? {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: Self.accountKey(serverUrl: serverUrl, accountId: accountId),
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne
        ]
        var result: AnyObject?
        let status = SecItemCopyMatching(query as CFDictionary, &result)
        if status == errSecItemNotFound {
            return nil
        }
        guard status == errSecSuccess, let data = result as? Data,
              let string = String(data: data, encoding: .utf8)
        else {
            throw RemFfiError.CredentialStore(message: "Keychain read failed (OSStatus \(status))")
        }
        return string
    }

    func save(serverUrl: String, accountId: String, secretJson: String) throws {
        guard let data = secretJson.data(using: .utf8) else {
            throw RemFfiError.CredentialStore(message: "Failed to encode credential JSON")
        }
        let key = Self.accountKey(serverUrl: serverUrl, accountId: accountId)
        let matchQuery: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: key
        ]
        let attributes: [String: Any] = [
            kSecValueData as String: data,
            kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        ]

        let updateStatus = SecItemUpdate(matchQuery as CFDictionary, attributes as CFDictionary)
        if updateStatus == errSecSuccess {
            return
        }
        if updateStatus != errSecItemNotFound {
            throw RemFfiError.CredentialStore(message: "Keychain update failed (OSStatus \(updateStatus))")
        }

        var addQuery = matchQuery
        addQuery[kSecValueData as String] = data
        addQuery[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        let addStatus = SecItemAdd(addQuery as CFDictionary, nil)
        guard addStatus == errSecSuccess else {
            throw RemFfiError.CredentialStore(message: "Keychain save failed (OSStatus \(addStatus))")
        }
    }

    func delete(serverUrl: String, accountId: String) throws {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: Self.accountKey(serverUrl: serverUrl, accountId: accountId)
        ]
        let status = SecItemDelete(query as CFDictionary)
        guard status == errSecSuccess || status == errSecItemNotFound else {
            throw RemFfiError.CredentialStore(message: "Keychain delete failed (OSStatus \(status))")
        }
    }

    static func accountKey(serverUrl: String, accountId: String) -> String {
        let normalizedUrl = serverUrl
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .trimmingCharacters(in: CharacterSet(charactersIn: "/"))
            .lowercased()
        let normalizedAccount = accountId.trimmingCharacters(in: .whitespacesAndNewlines)
        return "\(normalizedUrl)|\(normalizedAccount)"
    }
}

/// Thread-safe in-memory credential store for unit tests and SwiftUI previews.
final class InMemoryCredentialStore: FfiCredentialStore, @unchecked Sendable {
    private let lock = NSLock()
    private var storage: [String: String] = [:]

    func load(serverUrl: String, accountId: String) throws -> String? {
        lock.lock()
        defer { lock.unlock() }
        return storage[KeychainCredentialStore.accountKey(serverUrl: serverUrl, accountId: accountId)]
    }

    func save(serverUrl: String, accountId: String, secretJson: String) throws {
        lock.lock()
        defer { lock.unlock() }
        storage[KeychainCredentialStore.accountKey(serverUrl: serverUrl, accountId: accountId)] = secretJson
    }

    func delete(serverUrl: String, accountId: String) throws {
        lock.lock()
        defer { lock.unlock() }
        storage.removeValue(forKey: KeychainCredentialStore.accountKey(serverUrl: serverUrl, accountId: accountId))
    }
}
