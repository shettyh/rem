import AuthenticationServices
import Foundation
import UIKit

enum OAuthFlowError: LocalizedError {
    case invalidAuthorizeURL
    case cancelled
    case missingCallbackURL
    case providerError(String)
    case stateMismatch
    case missingAuthorizationCode

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
        }
    }
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

/// Runs PKCE OAuth via native `ASWebAuthenticationSession` using the `rem://auth/callback` scheme.
@MainActor
final class AuthSessionManager: NSObject, ASWebAuthenticationPresentationContextProviding {
    private var activeSession: ASWebAuthenticationSession?

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
