import CryptoKit
import Foundation
import Observation
import StoreKit
import UIKit

enum ApplePurchaseOutcome: Equatable {
    case purchased
    case cancelled
    case pending
    case failed(String)
}

enum AppleRestoreOutcome: Equatable {
    case restored
    case noActiveSubscription
    case failed(String)
}

/// Native StoreKit 2 manager for rem Premium ($4.99/year auto-renewable subscription).
@Observable
@MainActor
final class StoreKitManager {
    static let premiumYearlyProductID = "com.shettyh.rem.premium.yearly"

    var product: Product?
    var isLoadingProduct: Bool = false
    var isPurchasing: Bool = false
    var isRestoring: Bool = false
    var lastError: String?

    @ObservationIgnored
    nonisolated(unsafe) private var updatesTask: Task<Void, Never>?

    /// Localized App Store price once `loadProducts()` has resolved (nil until loaded so we never display a hardcoded USD fallback to international users).
    var localizedPrice: String? {
        product?.displayPrice
    }

    var displayPrice: String {
        product?.displayPrice ?? ""
    }

    deinit {
        updatesTask?.cancel()
    }

    /// Deterministic UUID derived from `accountId` (mirrored identically in `sync-server/src/appleBilling.ts`):
    /// - Standard 16-byte `usr_<22-char-base64url>` IDs map bijectively to those exact 16 bytes so ASN v2
    ///   webhooks can resolve `user_id` even before `/v1/billing/apple/verify` runs.
    /// - Canonical UUIDs are returned directly.
    /// - Any other account ID maps to an RFC 9562 UUIDv8 from SHA-256("rem:apple-account-token:v1:" + accountId).
    nonisolated static func appAccountToken(for accountId: String) -> UUID {
        if accountId.hasPrefix("usr_") {
            let suffix = String(accountId.dropFirst(4))
            if suffix.count == 22, let raw = decodeBase64Url(suffix), raw.count == 16 {
                return uuidFrom16Bytes(raw)
            }
        }

        if let direct = UUID(uuidString: accountId) {
            return direct
        }

        let digest = SHA256.hash(data: Data("rem:apple-account-token:v1:\(accountId)".utf8))
        var bytes = Array(digest.prefix(16))
        bytes[6] = (bytes[6] & 0x0f) | 0x80 // UUID v8
        bytes[8] = (bytes[8] & 0x3f) | 0x80 // RFC 4122/9562 variant
        return uuidFrom16Bytes(bytes)
    }

    func loadProducts() async {
        guard !isLoadingProduct else { return }
        isLoadingProduct = true
        defer { isLoadingProduct = false }

        do {
            let fetched = try await Product.products(for: [Self.premiumYearlyProductID])
            if let match = fetched.first(where: { $0.id == Self.premiumYearlyProductID }) {
                self.product = match
                self.lastError = nil
            }
        } catch {
            self.lastError = error.localizedDescription
        }
    }

    func startTransactionListener(
        onVerifiedTransaction: @escaping @MainActor (String) async throws -> Void
    ) {
        guard updatesTask == nil else { return }
        updatesTask = Task { [weak self] in
            for await result in Transaction.updates {
                guard !Task.isCancelled else { break }
                guard case .verified(let transaction) = result,
                      transaction.productID == Self.premiumYearlyProductID else {
                    continue
                }
                do {
                    try await onVerifiedTransaction(result.jwsRepresentation)
                    await transaction.finish()
                    await MainActor.run {
                        self?.lastError = nil
                    }
                } catch {
                    // Leave unfinished so StoreKit redelivers once signed in / online.
                    await MainActor.run {
                        self?.lastError = error.remDisplayMessage
                    }
                }
            }
        }
    }

    func stopTransactionListener() {
        updatesTask?.cancel()
        updatesTask = nil
    }

    func purchase(
        accountId: String,
        verifyWithServer: @escaping @MainActor (String) async throws -> Void
    ) async -> ApplePurchaseOutcome {
        guard !isPurchasing else { return .cancelled }
        isPurchasing = true
        lastError = nil
        defer { isPurchasing = false }

        if product == nil {
            await loadProducts()
        }
        guard let product else {
            let msg = "rem Premium is currently unavailable from the App Store. Try again shortly."
            lastError = msg
            return .failed(msg)
        }

        do {
            let token = Self.appAccountToken(for: accountId)
            let result = try await product.purchase(options: [.appAccountToken(token)])
            switch result {
            case .success(let verification):
                guard case .verified(let transaction) = verification else {
                    let msg = "App Store could not verify this purchase."
                    lastError = msg
                    return .failed(msg)
                }
                try await verifyWithServer(verification.jwsRepresentation)
                await transaction.finish()
                return .purchased
            case .userCancelled:
                return .cancelled
            case .pending:
                return .pending
            @unknown default:
                return .cancelled
            }
        } catch {
            let msg = error.remDisplayMessage
            lastError = msg
            return .failed(msg)
        }
    }

    func restorePurchases(
        verifyWithServer: @escaping @MainActor (String) async throws -> Void
    ) async -> AppleRestoreOutcome {
        guard !isRestoring else { return .noActiveSubscription }
        isRestoring = true
        lastError = nil
        defer { isRestoring = false }

        do {
            try await AppStore.sync()
            var restoredAny = false
            for await result in Transaction.currentEntitlements {
                guard case .verified(let transaction) = result,
                      transaction.productID == Self.premiumYearlyProductID,
                      transaction.revocationDate == nil else {
                    continue
                }
                try await verifyWithServer(result.jwsRepresentation)
                await transaction.finish()
                restoredAny = true
            }
            return restoredAny ? .restored : .noActiveSubscription
        } catch {
            let msg = error.remDisplayMessage
            lastError = msg
            return .failed(msg)
        }
    }

    func showManageSubscriptions() async {
        guard let scene = UIApplication.shared.connectedScenes
            .compactMap({ $0 as? UIWindowScene })
            .first(where: { $0.activationState == .foregroundActive })
            ?? UIApplication.shared.connectedScenes.compactMap({ $0 as? UIWindowScene }).first else {
            return
        }
        do {
            try await AppStore.showManageSubscriptions(in: scene)
        } catch {
            lastError = error.localizedDescription
        }
    }

    // MARK: - Helpers

    private nonisolated static func decodeBase64Url(_ input: String) -> [UInt8]? {
        var base64 = input
            .replacingOccurrences(of: "-", with: "+")
            .replacingOccurrences(of: "_", with: "/")
        let remainder = base64.count % 4
        if remainder > 0 {
            base64 += String(repeating: "=", count: 4 - remainder)
        }
        guard let data = Data(base64Encoded: base64) else { return nil }
        return Array(data)
    }

    private nonisolated static func uuidFrom16Bytes(_ bytes: [UInt8]) -> UUID {
        precondition(bytes.count == 16)
        return UUID(uuid: (
            bytes[0], bytes[1], bytes[2], bytes[3],
            bytes[4], bytes[5], bytes[6], bytes[7],
            bytes[8], bytes[9], bytes[10], bytes[11],
            bytes[12], bytes[13], bytes[14], bytes[15]
        ))
    }
}
