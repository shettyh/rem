import AuthenticationServices
import CryptoKit
import Foundation
import Security
import UIKit

enum OAuthFlowError: LocalizedError {
    case invalidAuthorizeURL
    case cancelled
    case missingCallbackURL
    case providerError(String)
    case stateMismatch
    case missingAuthorizationCode
    case missingIdentityToken

    var errorDescription: String? {
        switch self {
        case .invalidAuthorizeURL:
            return "Could not construct sign-in URL."
        case .cancelled:
            return "Sign-in was cancelled."
        case .missingCallbackURL:
            return "Sign-in did not return a callback URL."
        case .providerError(let code):
            return "Sign-in failed (\(code))."
        case .stateMismatch:
            return "Sign-in state verification failed. Please try again."
        case .missingAuthorizationCode:
            return "Sign-in response did not include an authorization code."
        case .missingIdentityToken:
            return "Apple did not return an identity token. Please try again."
        }
    }
}

struct NativeAppleCredential: Sendable {
    let identityToken: String
    let authorizationCode: String?
    let rawNonce: String
    let email: String?
}

struct PendingAccountSwitch: Identifiable {
    let id = UUID()
    let serverUrl: String
    let code: String
    let pkceVerifier: String
    let boundServerUrl: String
    let boundAccountId: String
    let incomingAccountId: String
}

/// Runs native Sign in with Apple (`ASAuthorizationController`) and Google PKCE OAuth (`ASWebAuthenticationSession`).
@MainActor
final class AuthSessionManager: NSObject, ASWebAuthenticationPresentationContextProviding,
    ASAuthorizationControllerDelegate, ASAuthorizationControllerPresentationContextProviding
{
    private var activeSession: ASWebAuthenticationSession?
    private var activeAppleController: ASAuthorizationController?
    private var appleContinuation: CheckedContinuation<NativeAppleCredential, Error>?
    private var pendingAppleNonce: String?

    nonisolated static func generateNonce(byteCount: Int = 32) -> String {
        var bytes = [UInt8](repeating: 0, count: byteCount)
        let status = SecRandomCopyBytes(kSecRandomDefault, byteCount, &bytes)
        if status != errSecSuccess {
            for idx in bytes.indices {
                bytes[idx] = UInt8.random(in: 0...255)
            }
        }
        return Data(bytes)
            .base64EncodedString()
            .replacingOccurrences(of: "+", with: "-")
            .replacingOccurrences(of: "/", with: "_")
            .replacingOccurrences(of: "=", with: "")
    }

    nonisolated static func sha256Hex(_ input: String) -> String {
        let digest = SHA256.hash(data: Data(input.utf8))
        return digest.map { String(format: "%02x", $0) }.joined()
    }

    nonisolated static func parseAppleAuthorization(
        _ authorization: ASAuthorization,
        rawNonce: String
    ) throws -> NativeAppleCredential {
        guard let credential = authorization.credential as? ASAuthorizationAppleIDCredential,
              let tokenData = credential.identityToken,
              let identityToken = String(data: tokenData, encoding: .utf8),
              !identityToken.isEmpty
        else {
            throw OAuthFlowError.missingIdentityToken
        }
        let authorizationCode = credential.authorizationCode.flatMap {
            let decoded = String(data: $0, encoding: .utf8)?.trimmingCharacters(in: .whitespacesAndNewlines)
            return (decoded?.isEmpty == false) ? decoded : nil
        }
        let email = credential.email?.trimmingCharacters(in: .whitespacesAndNewlines)
        return NativeAppleCredential(
            identityToken: identityToken,
            authorizationCode: authorizationCode,
            rawNonce: rawNonce,
            email: (email?.isEmpty == false) ? email : nil
        )
    }

    nonisolated static func parseAppleAuthorizationResult(
        _ result: Result<ASAuthorization, Error>,
        rawNonce: String
    ) throws -> NativeAppleCredential {
        switch result {
        case .success(let authorization):
            return try parseAppleAuthorization(authorization, rawNonce: rawNonce)
        case .failure(let error):
            if let authError = error as? ASAuthorizationError, authError.code == .canceled {
                throw OAuthFlowError.cancelled
            }
            throw error
        }
    }

    func authenticateWithAppleNative() async throws -> NativeAppleCredential {
        let rawNonce = Self.generateNonce()
        let request = ASAuthorizationAppleIDProvider().createRequest()
        request.requestedScopes = [.email]
        request.nonce = Self.sha256Hex(rawNonce)

        return try await withCheckedThrowingContinuation { continuation in
            let controller = ASAuthorizationController(authorizationRequests: [request])
            controller.delegate = self
            controller.presentationContextProvider = self
            self.pendingAppleNonce = rawNonce
            self.appleContinuation = continuation
            self.activeAppleController = controller
            controller.performRequests()
        }
    }

    func authorizationController(
        controller: ASAuthorizationController,
        didCompleteWithAuthorization authorization: ASAuthorization
    ) {
        let nonce = pendingAppleNonce ?? ""
        let continuation = appleContinuation
        pendingAppleNonce = nil
        appleContinuation = nil
        activeAppleController = nil
        do {
            let credential = try Self.parseAppleAuthorization(authorization, rawNonce: nonce)
            continuation?.resume(returning: credential)
        } catch {
            continuation?.resume(throwing: error)
        }
    }

    func authorizationController(
        controller: ASAuthorizationController,
        didCompleteWithError error: Error
    ) {
        let continuation = appleContinuation
        pendingAppleNonce = nil
        appleContinuation = nil
        activeAppleController = nil
        if let authError = error as? ASAuthorizationError, authError.code == .canceled {
            continuation?.resume(throwing: OAuthFlowError.cancelled)
            return
        }
        continuation?.resume(throwing: error)
    }

    func presentationAnchor(for controller: ASAuthorizationController) -> ASPresentationAnchor {
        currentPresentationAnchor()
    }

    func authenticate(authorizeUrlString: String, expectedState: String) async throws -> String {
        guard let url = URL(string: authorizeUrlString) else {
            throw OAuthFlowError.invalidAuthorizeURL
        }

        let callbackURL: URL = try await withCheckedThrowingContinuation { continuation in
            let session = ASWebAuthenticationSession(
                url: url,
                callbackURLScheme: "rem"
            ) { [weak self] callbackURL, error in
                Task { @MainActor in
                    self?.activeSession = nil
                }
                if let authError = error as? ASWebAuthenticationSessionError,
                   authError.code == .canceledLogin {
                    continuation.resume(throwing: OAuthFlowError.cancelled)
                    return
                }
                if let error {
                    continuation.resume(throwing: error)
                    return
                }
                guard let callbackURL else {
                    continuation.resume(throwing: OAuthFlowError.missingCallbackURL)
                    return
                }
                continuation.resume(returning: callbackURL)
            }
            session.presentationContextProvider = self
            session.prefersEphemeralWebBrowserSession = false
            self.activeSession = session
            if !session.start() {
                self.activeSession = nil
                continuation.resume(throwing: OAuthFlowError.invalidAuthorizeURL)
            }
        }

        return try Self.parseOAuthCallback(url: callbackURL, expectedState: expectedState)
    }

    nonisolated static func parseOAuthCallback(url: URL, expectedState: String) throws -> String {
        guard let components = URLComponents(url: url, resolvingAgainstBaseURL: false) else {
            throw OAuthFlowError.missingCallbackURL
        }
        let items = components.queryItems ?? []
        let query = Dictionary(uniqueKeysWithValues: items.compactMap { item -> (String, String)? in
            guard let value = item.value else { return nil }
            return (item.name, value)
        })

        if let errorCode = query["error"], !errorCode.isEmpty {
            throw OAuthFlowError.providerError(errorCode)
        }
        guard query["state"] == expectedState else {
            throw OAuthFlowError.stateMismatch
        }
        guard let code = query["code"], !code.isEmpty else {
            throw OAuthFlowError.missingAuthorizationCode
        }
        return code
    }

    func presentationAnchor(for session: ASWebAuthenticationSession) -> ASPresentationAnchor {
        currentPresentationAnchor()
    }

    private func currentPresentationAnchor() -> ASPresentationAnchor {
        let scenes = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
        if let keyWindow = scenes.flatMap(\.windows).first(where: \.isKeyWindow) {
            return keyWindow
        }
        if let firstWindow = scenes.flatMap(\.windows).first {
            return firstWindow
        }
        return ASPresentationAnchor()
    }
}
