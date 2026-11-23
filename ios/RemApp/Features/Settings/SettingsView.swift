import AuthenticationServices
import SwiftUI
import UIKit

struct SettingsView: View {
    @Bindable var appState: AppState
    @Environment(\.colorScheme) private var colorScheme

    @State private var useCustomServer: Bool = false
    @State private var customServerUrl: String = ""
    @State private var pendingAppleNonce: String = ""
    @State private var showDeleteAccountConfirm: Bool = false
    @State private var showPremiumPaywall: Bool = false
    @State private var shareBackupURL: URL?

    var body: some View {
        NavigationStack {
            Form {
                cloudSyncSection
                readingAndHabitSection
                yourDataSection
                legalFooterSection
            }
            .scrollContentBackground(.hidden)
            .background(RemColor.groupedBg)
            .navigationTitle("Settings")
            .onAppear {
            if appState.expandCustomServerInSettings {
                useCustomServer = true
                appState.expandCustomServerInSettings = false
            }
            if let bound = appState.syncStatus?.serverUrl,
               let def = appState.syncStatus?.defaultServerUrl,
               bound.trimmingCharacters(in: CharacterSet(charactersIn: "/")) != def.trimmingCharacters(in: CharacterSet(charactersIn: "/")) {
                useCustomServer = true
                if customServerUrl.isEmpty {
                    customServerUrl = bound
                }
            }
            if appState.syncStatus?.signedIn == true {
                Task {
                    await appState.refreshAccount(triggerSyncIfUnlocked: appState.isSyncPaused)
                    if appState.canShowAppleSubscribe {
                        await appState.storeKit.loadProducts()
                    }
                }
            }
        }
        .alert("Delete cloud account?", isPresented: $showDeleteAccountConfirm) {
            Button("Delete cloud data", role: .destructive) {
                Task { await appState.deleteCloudAccount() }
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            if appState.canShowAppleManageSubscription {
                Text("Permanently removes your synced data and images from the sync server. Your local cards on this iPhone remain intact. Deleting your cloud account does not cancel your App Store subscription — cancel it in iOS Settings → Apple Account → Subscriptions.")
            } else {
                Text("Permanently removes your synced data and images from the sync server. Your local cards on this iPhone remain intact.")
            }
        }
        .alert(
            "Switch cloud account?",
            isPresented: Binding(
                get: { appState.pendingAccountSwitch != nil },
                set: { if !$0 { appState.pendingAccountSwitch = nil } }
            )
        ) {
            if let mismatch = appState.pendingAccountSwitch {
                Button("Reset & sign in with Apple") {
                    Task { await appState.switchAccountWithReauth(provider: .apple, serverUrl: mismatch.serverUrl) }
                }
                Button("Reset & sign in with Google") {
                    Task { await appState.switchAccountWithReauth(provider: .google, serverUrl: mismatch.serverUrl) }
                }
            }
            Button("Cancel", role: .cancel) {
                appState.pendingAccountSwitch = nil
            }
        } message: {
            Text("This iPhone is currently linked to another cloud account. Switching keeps your local cards and connects the new account.")
        }
        .sheet(isPresented: $showPremiumPaywall) {
            PremiumPaywallSheet(appState: appState)
        }
        .sheet(isPresented: Binding(
            get: { shareBackupURL != nil },
            set: { if !$0 { shareBackupURL = nil } }
        )) {
            if let url = shareBackupURL {
                ActivityShareSheet(items: [url])
            }
        }
        }
    }

    // MARK: - Sections

    private var cloudSyncSection: some View {
        let isSignedIn = appState.syncStatus?.signedIn == true
        return Section("Cloud Sync") {
            if isSignedIn {
                signedInRows
            } else {
                signedOutCard
            }
        }
        .listRowBackground(RemColor.surface)
        .listRowSeparatorTint(RemColor.border)
    }

    @ViewBuilder
    private var signedInRows: some View {
        let ent = appState.entitlement
        let entState = appState.effectiveEntitlementState
        let isPaused = appState.isSyncPaused
        let trialDays = Int(ent?.trialDays ?? 30)
        let daysLeft = daysRemaining(ent?.expiresAt)

        HStack(alignment: .top) {
            VStack(alignment: .leading, spacing: 3) {
                Text(entState == .selfHosted ? "Self-hosted sync" : "rem Premium")
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(RemColor.text)
                Text(signedInSubtitle)
                    .font(.caption)
                    .foregroundStyle(RemColor.muted)
                if let planSubtitle = planDetailLine(ent: ent, state: entState, daysLeft: daysLeft) {
                    Text(planSubtitle)
                        .font(.caption2)
                        .foregroundStyle(RemColor.faint)
                }
                if let err = appState.lastSyncError, !isPaused {
                    Text(err)
                        .font(.caption)
                        .foregroundStyle(RemColor.again)
                        .padding(.top, 2)
                }
            }
            Spacer()
            entitlementBadge(ent: ent, state: entState, daysLeft: daysLeft)
        }
        .padding(.vertical, 2)

        if entState == .none || entState == .expired || entState == .pastDue {
            VStack(alignment: .leading, spacing: 10) {
                Text(pausedCardTitle(state: entState))
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(RemColor.text)

                Text(pausedCardBody(state: entState, provider: ent?.provider, trialDays: trialDays))
                    .font(.footnote)
                    .foregroundStyle(RemColor.muted)
                    .lineSpacing(2)

                let showTrialButton = (entState == .none || entState == .expired)
                    && (ent?.canStartTrial ?? (entState == .none))

                if showTrialButton {
                    Button {
                        Task { await appState.startTrial() }
                    } label: {
                        HStack(spacing: 8) {
                            if appState.isStartingTrial {
                                ProgressView()
                                    .controlSize(.small)
                                    .tint(RemColor.onAccent)
                            }
                            Text(appState.isStartingTrial ? "Starting trial…" : "Start \(trialDays)-day free trial")
                        }
                    }
                    .buttonStyle(RemPrimaryButtonStyle())
                    .disabled(appState.isStartingTrial || appState.isCheckingEntitlement)
                    .padding(.top, 2)
                }

                if appState.canShowAppleSubscribe {
                    let upgradeTitle = appState.storeKit.localizedPrice.map {
                        "Upgrade to Premium — \($0)/year"
                    } ?? "Upgrade to Premium"
                    if showTrialButton {
                        Button {
                            showPremiumPaywall = true
                        } label: {
                            Text(upgradeTitle)
                        }
                        .buttonStyle(RemSecondaryButtonStyle())
                        .disabled(appState.isStartingTrial || appState.storeKit.isPurchasing)
                    } else {
                        Button {
                            showPremiumPaywall = true
                        } label: {
                            Text(upgradeTitle)
                        }
                        .buttonStyle(RemPrimaryButtonStyle())
                        .disabled(appState.storeKit.isPurchasing)
                        .padding(.top, 2)
                    }
                }

                if entState == .pastDue && appState.canShowAppleManageSubscription {
                    Button {
                        Task { await appState.manageAppleSubscription() }
                    } label: {
                        Text("Manage subscription")
                    }
                    .buttonStyle(RemPrimaryButtonStyle())
                    .padding(.top, 2)
                }

                Button {
                    Task { _ = await appState.restoreApplePurchases() }
                } label: {
                    HStack(spacing: 6) {
                        if appState.storeKit.isRestoring {
                            ProgressView().controlSize(.mini)
                        }
                        Text(appState.storeKit.isRestoring ? "Restoring purchases…" : "Restore purchases")
                            .font(.footnote.weight(.semibold))
                            .foregroundStyle(RemColor.accent)
                    }
                    .frame(maxWidth: .infinity)
                    .padding(.top, 2)
                }
                .buttonStyle(.plain)
                .disabled(appState.storeKit.isRestoring || appState.storeKit.isPurchasing)
            }
            .padding(12)
            .background(entState == .pastDue ? RemColor.hardBg : RemColor.bgInset)
            .clipShape(RoundedRectangle(cornerRadius: 11, style: .continuous))
            .overlay(
                RoundedRectangle(cornerRadius: 11, style: .continuous)
                    .strokeBorder(
                        entState == .pastDue ? RemColor.hard.opacity(0.28) : RemColor.border,
                        lineWidth: 1
                    )
            )
            .padding(.vertical, 4)
        }

        if isPaused || entState == .pastDue {
            Button {
                Task { await appState.checkAccessAndRetrySync() }
            } label: {
                HStack {
                    Text("Check access / Retry sync")
                        .foregroundStyle(RemColor.text)
                    Spacer()
                    if appState.isCheckingEntitlement || appState.isSyncing {
                        ProgressView().controlSize(.small)
                    } else {
                        Text("Check")
                            .font(.subheadline.weight(.semibold))
                            .foregroundStyle(RemColor.accent)
                    }
                }
            }
            .disabled(appState.isCheckingEntitlement || appState.isSyncing)
        } else {
            Button {
                Task { await appState.syncNow(silent: false) }
            } label: {
                HStack {
                    Text("Sync now")
                        .foregroundStyle(RemColor.text)
                    Spacer()
                    if appState.isSyncing {
                        ProgressView().controlSize(.small)
                    } else {
                        Text(appState.lastSyncError != nil ? "Retry" : "Run")
                            .font(.subheadline.weight(.semibold))
                            .foregroundStyle(RemColor.accent)
                    }
                }
            }
            .disabled(appState.isSyncing)

            if appState.canShowAppleSubscribe {
                Button {
                    showPremiumPaywall = true
                } label: {
                    HStack {
                        VStack(alignment: .leading, spacing: 2) {
                            Text("Upgrade to rem Premium")
                                .foregroundStyle(RemColor.text)
                            Text(upgradeRowSubtitle(daysLeft: daysLeft))
                                .font(.caption)
                                .foregroundStyle(RemColor.muted)
                        }
                        Spacer()
                        Text("Upgrade")
                            .font(.subheadline.weight(.semibold))
                            .foregroundStyle(RemColor.accent)
                    }
                }
            } else if appState.canShowAppleManageSubscription {
                Button {
                    Task { await appState.manageAppleSubscription() }
                } label: {
                    HStack {
                        VStack(alignment: .leading, spacing: 2) {
                            Text("Manage subscription")
                                .foregroundStyle(RemColor.text)
                            if let dateStr = formattedShortDate(ent?.expiresAt) {
                                let verb = (ent?.cancelAtPeriodEnd ?? false) ? "Ends" : "Renews"
                                Text("\(verb) \(dateStr) · App Store")
                                    .font(.caption)
                                    .foregroundStyle(RemColor.muted)
                            } else {
                                Text("App Store")
                                    .font(.caption)
                                    .foregroundStyle(RemColor.muted)
                            }
                        }
                        Spacer()
                        Image(systemName: "chevron.right")
                            .font(.caption.weight(.semibold))
                            .foregroundStyle(RemColor.faint)
                    }
                }
            }
        }

        HStack {
            Text("Server")
                .foregroundStyle(RemColor.text)
            Spacer()
            Text(displayServerName)
                .font(displayServerName == "rem Cloud" ? .subheadline : .caption.monospaced())
                .foregroundStyle(RemColor.muted)
                .lineLimit(1)
        }

        Button {
            Task { await appState.signOut() }
        } label: {
            Text("Sign out (keep local cards)")
                .foregroundStyle(RemColor.text)
        }

        Button(role: .destructive) {
            showDeleteAccountConfirm = true
        } label: {
            Text("Delete cloud account")
        }
    }

    private var signedOutCard: some View {
        VStack(alignment: .leading, spacing: 11) {
            VStack(alignment: .leading, spacing: 3) {
                Text("Local-only mode")
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(RemColor.text)
                Text("Your decks stay on this iPhone. Sign in to sync decks, study progress, and images across your devices.")
                    .font(.footnote)
                    .foregroundStyle(RemColor.muted)
                    .lineSpacing(2)
            }

            SignInWithAppleButton(.continue) { request in
                let rawNonce = AuthSessionManager.generateNonce()
                pendingAppleNonce = rawNonce
                request.requestedScopes = [.email]
                request.nonce = AuthSessionManager.sha256Hex(rawNonce)
            } onCompletion: { result in
                let rawNonce = pendingAppleNonce
                pendingAppleNonce = ""
                do {
                    let credential = try AuthSessionManager.parseAppleAuthorizationResult(
                        result,
                        rawNonce: rawNonce
                    )
                    Task {
                        await appState.signInWithAppleNative(
                            credential: credential,
                            customServerUrl: useCustomServer ? customServerUrl : nil
                        )
                    }
                } catch OAuthFlowError.cancelled {
                    return
                } catch {
                    appState.showToast(error.remDisplayMessage)
                }
            }
            .signInWithAppleButtonStyle(colorScheme == .dark ? .white : .black)
            .frame(height: 44)
            .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
            .id(colorScheme)
            .disabled(appState.isAuthenticating)
            .opacity(appState.isAuthenticating ? 0.6 : 1.0)

            Button {
                Task {
                    await appState.signIn(
                        provider: .google,
                        customServerUrl: useCustomServer ? customServerUrl : nil
                    )
                }
            } label: {
                HStack(spacing: 8) {
                    GoogleLogoMark(size: 16)
                    Text(appState.isAuthenticating ? "Signing in…" : "Continue with Google")
                }
            }
            .buttonStyle(RemSecondaryButtonStyle())
            .disabled(appState.isAuthenticating)

            DisclosureGroup("Custom server", isExpanded: $useCustomServer) {
                VStack(alignment: .leading, spacing: 7) {
                    TextField(
                        "https://sync.example.com",
                        text: $customServerUrl
                    )
                    .font(.footnote.monospaced())
                    .keyboardType(.URL)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    .padding(10)
                    .background(RemColor.bgInset)
                    .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
                    .overlay(
                        RoundedRectangle(cornerRadius: 8, style: .continuous)
                            .strokeBorder(RemColor.borderStrong, lineWidth: 1)
                    )

                    HStack(spacing: 4) {
                        Text("Connect to your own server.")
                            .foregroundStyle(RemColor.faint)
                        Link("Setup guide ↗", destination: RemDocs.selfHostingGuideURL)
                            .foregroundStyle(RemColor.faint)
                            .underline()
                    }
                    .font(.caption)
                }
                .padding(.top, 6)
            }
            .font(.footnote.weight(.medium))
            .foregroundStyle(RemColor.muted)
        }
        .padding(.vertical, 4)
    }

    private var readingAndHabitSection: some View {
        Section("Reading & Habit") {
            HStack {
                Text("Card text size")
                    .foregroundStyle(RemColor.text)
                Spacer()
                Picker("Text size", selection: $appState.cardTextSize) {
                    ForEach(CardTextSize.allCases) { size in
                        Text(size.shortLabel).tag(size)
                    }
                }
                .pickerStyle(.segmented)
                .frame(width: 124)
            }

            HStack {
                Text("Theme")
                    .foregroundStyle(RemColor.text)
                Spacer()
                Picker("Theme", selection: $appState.appearance) {
                    ForEach(AppAppearance.allCases) { mode in
                        Text(mode.label).tag(mode)
                    }
                }
                .pickerStyle(.segmented)
                .frame(width: 186)
            }

            Toggle("Haptic grade feedback", isOn: $appState.hapticsEnabled)
                .tint(RemColor.accent)

            Toggle(isOn: Binding(
                get: { appState.reminderEnabled },
                set: { next in Task { await appState.setReminderEnabled(next) } }
            )) {
                VStack(alignment: .leading, spacing: 2) {
                    Text("Daily reminder")
                        .foregroundStyle(RemColor.text)
                    Text("Only notifies if cards are still due")
                        .font(.caption)
                        .foregroundStyle(RemColor.muted)
                }
            }
            .tint(RemColor.accent)

            if appState.reminderEnabled {
                DatePicker(
                    "Reminder time",
                    selection: $appState.reminderDate,
                    displayedComponents: .hourAndMinute
                )
                .tint(RemColor.accent)
            }
        }
        .listRowBackground(RemColor.surface)
        .listRowSeparatorTint(RemColor.border)
    }

    private var yourDataSection: some View {
        Section("Your Data") {
            VStack(alignment: .leading, spacing: 2) {
                Text("On this iPhone")
                    .foregroundStyle(RemColor.text)
                Text(storageSubtitle)
                    .font(.caption)
                    .foregroundStyle(RemColor.muted)
            }

            Button {
                Task {
                    shareBackupURL = await appState.exportBackupFile()
                }
            } label: {
                HStack {
                    Text("Export backup")
                        .foregroundStyle(RemColor.text)
                    Spacer()
                    Image(systemName: "square.and.arrow.up")
                        .foregroundStyle(RemColor.muted)
                }
            }
        }
        .listRowBackground(RemColor.surface)
        .listRowSeparatorTint(RemColor.border)
    }

    private var legalFooterSection: some View {
        Section {
            EmptyView()
        } footer: {
            VStack(spacing: 8) {
                HStack(spacing: 12) {
                    Spacer()
                    Link("Privacy Policy", destination: RemDocs.privacyPolicyURL)
                    Text("·").foregroundStyle(RemColor.borderStrong)
                    Link("Terms of Use", destination: RemDocs.termsOfUseURL)
                    Spacer()
                }
                .font(.footnote.weight(.medium))
                .foregroundStyle(RemColor.muted)

                if let versionLabel = appVersionLabel {
                    Text(versionLabel)
                        .font(.caption)
                        .foregroundStyle(RemColor.faint)
                }
            }
            .frame(maxWidth: .infinity)
            .padding(.top, 4)
        }
    }

    // MARK: - Helpers

    private var appVersionLabel: String? {
        let info = Bundle.main.infoDictionary
        let short = info?["CFBundleShortVersionString"] as? String
        let build = info?["CFBundleVersion"] as? String
        if let short, let build, !short.isEmpty, !build.isEmpty {
            return "rem \(short) (\(build))"
        }
        if let short, !short.isEmpty {
            return "rem \(short)"
        }
        return nil
    }

    private func upgradeRowSubtitle(daysLeft: Int?) -> String {
        let pricePart = appState.storeKit.localizedPrice.map { "\($0)/year" }
        switch (daysLeft, pricePart) {
        case let (days?, price?):
            return "\(days)d left · \(price)"
        case let (days?, nil):
            return "\(days)d left · Annual plan via App Store"
        case let (nil, price?):
            return "\(price) via App Store"
        case (nil, nil):
            return "Annual plan via App Store"
        }
    }

    private func daysRemaining(_ expiresAt: Int64?) -> Int? {
        guard let expiresAt else { return nil }
        let nowMs = RemStore.nowMs()
        let remainingMs = max(0, expiresAt - nowMs)
        return max(1, Int((remainingMs + 86_400_000 - 1) / 86_400_000))
    }

    private func formattedShortDate(_ epochMs: Int64?) -> String? {
        guard let epochMs else { return nil }
        let date = Date(timeIntervalSince1970: TimeInterval(epochMs) / 1000.0)
        return date.formatted(date: .abbreviated, time: .omitted)
    }

    private func entitlementBadge(
        ent: EntitlementInfoDto?,
        state: EntitlementStateDto,
        daysLeft: Int?
    ) -> some View {
        let pending = appState.syncStatus?.pendingOutboxCount ?? 0
        switch state {
        case .trialing:
            return RemPillBadge(text: daysLeft.map { "Trial · \($0)d left" } ?? "Trial", tone: .good)
        case .invite:
            return RemPillBadge(text: daysLeft.map { "Invite · \($0)d left" } ?? "Free access", tone: .good)
        case .pastDue:
            return RemPillBadge(text: "Payment issue", tone: .hard)
        case .expired:
            return RemPillBadge(text: "Sync paused", tone: .hard)
        case .none:
            return RemPillBadge(text: "Not active", tone: .hard)
        case .active, .selfHosted:
            if appState.lastSyncError != nil {
                return RemPillBadge(text: pending > 0 ? "Pending" : "Error", tone: pending > 0 ? .hard : .again)
            }
            if pending > 0 {
                return RemPillBadge(text: "\(pending) pending", tone: .hard)
            }
            return RemPillBadge(text: "Synced", tone: .good)
        }
    }

    private func planDetailLine(
        ent: EntitlementInfoDto?,
        state: EntitlementStateDto,
        daysLeft: Int?
    ) -> String? {
        switch state {
        case .trialing:
            return daysLeft.map { "30-day free trial · \($0)d remaining" } ?? "Free trial active"
        case .invite:
            return daysLeft.map { "Free access · \($0)d remaining" } ?? "Free access across all devices"
        case .active:
            let via = ent?.provider == "apple" ? "rem Premium · App Store" : "rem Premium · Web"
            if let dateStr = formattedShortDate(ent?.expiresAt) {
                let verb = (ent?.cancelAtPeriodEnd ?? false) ? "Ends" : "Renews"
                return "\(via) · \(verb) \(dateStr)"
            }
            return via
        case .pastDue:
            return "Renewal issue · Local cards stay safe"
        case .expired, .none, .selfHosted:
            return nil
        }
    }

    private func pausedCardTitle(state: EntitlementStateDto) -> String {
        switch state {
        case .none: return "Activate rem Premium"
        case .pastDue: return "Payment issue"
        case .expired: return "Sync paused"
        default: return "Cloud sync paused"
        }
    }

    private func pausedCardBody(state: EntitlementStateDto, provider: String?, trialDays: Int) -> String {
        switch state {
        case .none:
            return "All local features stay free forever. Start a \(trialDays)-day free trial with no card required to sync across your devices."
        case .pastDue:
            if provider == "apple" {
                return "We couldn't renew your App Store subscription. Your cards stay safely on this iPhone."
            }
            return "There is a billing issue with your rem Premium plan. Your cards stay safely on this iPhone."
        case .expired:
            return "Your trial or rem Premium plan has ended. Your cards stay safely on this iPhone, and you can keep studying offline anytime."
        default:
            return "Your cards stay safely on this iPhone."
        }
    }

    private var signedInSubtitle: String {
        guard let status = appState.syncStatus else { return "Connected" }
        let providerLabel: String = {
            switch status.provider {
            case "apple": return "Apple"
            case "google": return "Google"
            default: return "Cloud"
            }
        }()
        let identity = status.email ?? "Signed in with \(providerLabel)"
        if status.pendingOutboxCount > 0 {
            let changeWord = status.pendingOutboxCount == 1 ? "change" : "changes"
            return "\(identity) · \(status.pendingOutboxCount) \(changeWord) waiting to sync"
        }
        if let last = appState.lastSyncedAt {
            let formatter = RelativeDateTimeFormatter()
            formatter.unitsStyle = .abbreviated
            return "\(identity) · \(formatter.localizedString(for: last, relativeTo: Date()))"
        }
        return identity
    }

    private var displayServerName: String {
        guard let status = appState.syncStatus else { return "rem Cloud" }
        guard let url = status.serverUrl else { return "rem Cloud" }
        if url.trimmingCharacters(in: CharacterSet(charactersIn: "/"))
            == status.defaultServerUrl.trimmingCharacters(in: CharacterSet(charactersIn: "/")) {
            return "rem Cloud"
        }
        return url.replacingOccurrences(of: "https://", with: "")
    }

    private var storageSubtitle: String {
        guard let summary = appState.storageSummary else { return "Calculating…" }
        let cardWord = summary.totalCards == 1 ? "card" : "cards"
        let imageWord = summary.assetCount == 1 ? "image" : "images"
        let mb = Double(summary.assetBytes) / 1_048_576.0
        let sizeText = mb < 0.1 && summary.assetBytes > 0 ? "< 0.1 MB" : String(format: "%.1f MB", mb)
        if summary.assetCount == 0 {
            return "\(summary.totalCards) \(cardWord)"
        }
        return "\(summary.totalCards) \(cardWord) · \(summary.assetCount) \(imageWord) · \(sizeText)"
    }
}

struct ActivityShareSheet: UIViewControllerRepresentable {
    let items: [Any]

    func makeUIViewController(context: Context) -> UIActivityViewController {
        UIActivityViewController(activityItems: items, applicationActivities: nil)
    }

    func updateUIViewController(_ uiViewController: UIActivityViewController, context: Context) {}
}
