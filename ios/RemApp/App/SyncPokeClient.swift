import Foundation

/// Authentication bundle required to open `GET /v1/sync/ws`.
struct SyncPokeAuth: Sendable, Equatable {
    let serverURL: String
    let deviceId: String
    let accessToken: String
}

/// Parsed inbound text frame from `UserSyncDO`.
enum SyncPokeMessage: Equatable, Sendable {
    case poke(latestSeq: UInt64)
    case pong
    case ignored

    private struct RawPokePayload: Decodable {
        let type: String
        let latestSeq: UInt64?
    }

    static func parse(_ text: String) -> SyncPokeMessage {
        if text == "pong" {
            return .pong
        }
        guard let data = text.data(using: .utf8),
              let payload = try? JSONDecoder().decode(RawPokePayload.self, from: data)
        else {
            return .ignored
        }
        if payload.type == "poke", let seq = payload.latestSeq {
            return .poke(latestSeq: seq)
        }
        return .ignored
    }
}

/// Lightweight foreground-only WebSocket listener for Cloud Sync v1.1 pokes.
///
/// Connects ONLY while the app is foregrounded (`scenePhase == .active`) and disconnects
/// immediately on `.inactive`/`.background` for zero background battery drain.
/// Auth tokens are passed exclusively via HTTP headers (`Authorization` and `X-Rem-Device-Id`),
/// never in URL query parameters.
@MainActor
final class SyncPokeClient {
    static let pingIntervalSeconds: UInt64 = 30
    static let maxBackoffSeconds: UInt64 = 30
    static let accountDeletedCloseCode: Int = 4001

    private(set) var isConnected: Bool = false
    private(set) var isRunning: Bool = false

    private var runTask: Task<Void, Never>?
    private var activeSocket: URLSessionWebSocketTask?
    private var activeSession: URLSession?

    /// Convert an HTTP(S) sync server base URL into its `ws(s)://.../v1/sync/ws` endpoint.
    static func websocketURL(from serverURL: String) -> URL? {
        let trimmed = serverURL.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty, var components = URLComponents(string: trimmed) else {
            return nil
        }
        switch components.scheme?.lowercased() {
        case "https":
            components.scheme = "wss"
        case "http":
            components.scheme = "ws"
        case "wss", "ws":
            break
        default:
            return nil
        }
        guard let host = components.host, !host.isEmpty else {
            return nil
        }
        var basePath = components.path
        while basePath.hasSuffix("/") {
            basePath.removeLast()
        }
        components.path = basePath + "/v1/sync/ws"
        components.query = nil
        components.fragment = nil
        return components.url
    }

    /// Build the WebSocket upgrade request with header-only authentication.
    static func makeRequest(auth: SyncPokeAuth) -> URLRequest? {
        guard let url = websocketURL(from: auth.serverURL) else { return nil }
        var request = URLRequest(url: url)
        request.setValue("Bearer \(auth.accessToken)", forHTTPHeaderField: "Authorization")
        request.setValue(auth.deviceId, forHTTPHeaderField: "X-Rem-Device-Id")
        return request
    }

    /// Returns `true` when an incoming poke announces a newer sequence number than already synced locally.
    static func shouldTriggerSync(pokeLatestSeq: UInt64, localLastSeq: UInt64) -> Bool {
        pokeLatestSeq > localLastSeq
    }

    /// Exponential reconnect delay in nanoseconds (1s, 2s, 4s, 8s, 16s, capped at 30s).
    static func backoffDelayNanoseconds(attempt: Int) -> UInt64 {
        let clamped = max(0, min(attempt, 5))
        let seconds = min(UInt64(1) << clamped, maxBackoffSeconds)
        return seconds * 1_000_000_000
    }

    /// Start (or restart) the foreground WebSocket poke loop.
    func start(
        authProvider: @escaping @Sendable (_ forceRefresh: Bool) async throws -> SyncPokeAuth,
        onPoke: @escaping @MainActor (UInt64) -> Void,
        onConnectionChange: (@MainActor (Bool) -> Void)? = nil,
        onSubscriptionRequired: (@MainActor () -> Void)? = nil,
        onAccountClosed: (@MainActor () -> Void)? = nil
    ) {
        stop()
        isRunning = true

        runTask = Task { [weak self] in
            var attempt = 0
            var forceRefresh = false
            var refreshedAfterUnauthorized = false

            while let self, self.isRunning, !Task.isCancelled {
                let auth: SyncPokeAuth
                do {
                    auth = try await authProvider(forceRefresh)
                    forceRefresh = false
                } catch {
                    if let ffi = error as? RemFfiError {
                        switch ffi {
                        case .NotConfigured, .NotAuthenticated:
                            self.setConnected(false, onConnectionChange: onConnectionChange)
                            self.isRunning = false
                            return
                        case .SubscriptionRequired:
                            self.setConnected(false, onConnectionChange: onConnectionChange)
                            self.isRunning = false
                            onSubscriptionRequired?()
                            return
                        default:
                            break
                        }
                    }
                    let delay = Self.backoffDelayNanoseconds(attempt: attempt)
                    attempt += 1
                    try? await Task.sleep(nanoseconds: delay)
                    continue
                }

                guard self.isRunning, !Task.isCancelled else { break }
                guard let request = Self.makeRequest(auth: auth) else {
                    self.isRunning = false
                    break
                }

                let outcome = await self.connectAndListen(
                    request: request,
                    onOpen: { [weak self] in
                        attempt = 0
                        refreshedAfterUnauthorized = false
                        self?.setConnected(true, onConnectionChange: onConnectionChange)
                    },
                    onPoke: onPoke
                )

                self.setConnected(false, onConnectionChange: onConnectionChange)
                guard self.isRunning, !Task.isCancelled else { break }

                switch outcome {
                case .subscriptionRequired:
                    self.isRunning = false
                    onSubscriptionRequired?()
                    return
                case .accountClosed:
                    self.isRunning = false
                    onAccountClosed?()
                    return
                case .unauthorized:
                    if refreshedAfterUnauthorized {
                        // Already refreshed once and still 401; back off normally.
                        let delay = Self.backoffDelayNanoseconds(attempt: attempt)
                        attempt += 1
                        try? await Task.sleep(nanoseconds: delay)
                    } else {
                        refreshedAfterUnauthorized = true
                        forceRefresh = true
                        try? await Task.sleep(nanoseconds: 500_000_000)
                    }
                case .disconnected:
                    let delay = Self.backoffDelayNanoseconds(attempt: attempt)
                    attempt += 1
                    try? await Task.sleep(nanoseconds: delay)
                }
            }
        }
    }

    /// Cleanly close any active WebSocket and cancel reconnection loops.
    func stop() {
        isRunning = false
        runTask?.cancel()
        runTask = nil
        activeSocket?.cancel(with: .goingAway, reason: nil)
        activeSocket = nil
        activeSession?.invalidateAndCancel()
        activeSession = nil
        isConnected = false
    }

    private enum DisconnectOutcome {
        case disconnected
        case unauthorized
        case subscriptionRequired
        case accountClosed
    }

    private func setConnected(_ connected: Bool, onConnectionChange: (@MainActor (Bool) -> Void)?) {
        guard isConnected != connected else { return }
        isConnected = connected
        onConnectionChange?(connected)
    }

    private func connectAndListen(
        request: URLRequest,
        onOpen: @escaping @MainActor () -> Void,
        onPoke: @escaping @MainActor (UInt64) -> Void
    ) async -> DisconnectOutcome {
        let delegate = WebSocketDelegateBox(onOpen: {
            Task { @MainActor in onOpen() }
        })
        let config = URLSessionConfiguration.ephemeral
        config.waitsForConnectivity = false
        let session = URLSession(configuration: config, delegate: delegate, delegateQueue: nil)
        let socket = session.webSocketTask(with: request)
        self.activeSession = session
        self.activeSocket = socket

        defer {
            socket.cancel(with: .goingAway, reason: nil)
            session.invalidateAndCancel()
            if self.activeSocket === socket {
                self.activeSocket = nil
            }
            if self.activeSession === session {
                self.activeSession = nil
            }
        }

        socket.resume()

        // Periodic text "ping" keepalive handled by Cloudflare DO auto-response without waking the DO.
        let pingTask = Task {
            while !Task.isCancelled {
                try? await Task.sleep(nanoseconds: Self.pingIntervalSeconds * 1_000_000_000)
                guard !Task.isCancelled else { break }
                do {
                    try await socket.send(.string("ping"))
                } catch {
                    break
                }
            }
        }
        defer { pingTask.cancel() }

        while isRunning && !Task.isCancelled {
            do {
                let message = try await socket.receive()
                switch message {
                case .string(let text):
                    if case .poke(let latestSeq) = SyncPokeMessage.parse(text) {
                        onPoke(latestSeq)
                    }
                case .data(let data):
                    if let text = String(data: data, encoding: .utf8),
                       case .poke(let latestSeq) = SyncPokeMessage.parse(text) {
                        onPoke(latestSeq)
                    }
                @unknown default:
                    break
                }
            } catch {
                if let http = socket.response as? HTTPURLResponse {
                    if http.statusCode == 402 {
                        return .subscriptionRequired
                    }
                    if http.statusCode == 401 {
                        return .unauthorized
                    }
                }
                let closeRaw = delegate.closeCodeRaw ?? socket.closeCode.rawValue
                if closeRaw == Self.accountDeletedCloseCode {
                    return .accountClosed
                }
                return .disconnected
            }
        }

        return .disconnected
    }
}

private final class WebSocketDelegateBox: NSObject, URLSessionWebSocketDelegate, @unchecked Sendable {
    private let onOpenHandler: @Sendable () -> Void
    private let lock = NSLock()
    private var _closeCodeRaw: Int?

    var closeCodeRaw: Int? {
        lock.lock()
        defer { lock.unlock() }
        return _closeCodeRaw
    }

    init(onOpen: @escaping @Sendable () -> Void) {
        self.onOpenHandler = onOpen
    }

    func urlSession(
        _ session: URLSession,
        webSocketTask: URLSessionWebSocketTask,
        didOpenWithProtocol protocol: String?
    ) {
        onOpenHandler()
    }

    func urlSession(
        _ session: URLSession,
        webSocketTask: URLSessionWebSocketTask,
        didCloseWith closeCode: URLSessionWebSocketTask.CloseCode,
        reason: Data?
    ) {
        lock.lock()
        _closeCodeRaw = closeCode.rawValue
        lock.unlock()
    }
}
