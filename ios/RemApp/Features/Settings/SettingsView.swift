import SwiftUI
import UIKit

struct SettingsView: View {
    @Bindable var appState: AppState

    @State private var useCustomServer: Bool = false
    @State private var customServerUrl: String = ""
    @State private var showDeleteAccountConfirm: Bool = false
    @State private var showPremiumPaywall: Bool = false
    @State private var shareBackupURL: URL?

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                Text("Settings")
                    .font(.system(size: 28, weight: .bold))
                    .tracking(-0.6)
                    .foregroundStyle(RemColor.text)

                cloudSyncSection
                readingAndHabitSection
                yourDataSection
            }
            .padding(.horizontal, 18)
            .padding(.top, 10)
            .padding(.bottom, 104)
        }
        .background(RemColor.bg)
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

    // MARK: - Sections

    private var cloudSyncSection: some View {
        let isSignedIn = appState.syncStatus?.signedIn == true
        return section("CLOUD SYNC") {
            if isSignedIn {
                signedInCard
            } else {
                signedOutCard
            }
        }
    }

    private var signedInCard: some View {
        let ent = appState.entitlement
        let entState = appState.effectiveEntitlementState
        let isPaused = appState.isSyncPaused
        let trialDays = Int(ent?.trialDays ?? 30)
        let daysLeft = daysRemaining(ent?.expiresAt)

        return VStack(spacing: 0) {
            HStack(alignment: .top) {
                VStack(alignment: .leading, spacing: 3) {
                    Text(entState == .selfHosted ? "Self-hosted sync" : "rem Premium")
                        .font(.system(size: 14.5, weight: .semibold))
                        .foregroundStyle(RemColor.text)
                    Text(signedInSubtitle)
                        .font(.system(size: 12))
                        .foregroundStyle(RemColor.muted)
                    if let planSubtitle = planDetailLine(ent: ent, state: entState, daysLeft: daysLeft) {
                        Text(planSubtitle)
                            .font(.system(size: 11.5))
                            .foregroundStyle(RemColor.faint)
                    }
                    if let err = appState.lastSyncError, !isPaused {
                        Text(err)
                            .font(.system(size: 11.5))
                            .foregroundStyle(RemColor.again)
                            .padding(.top, 2)
                    }
                }
                Spacer()
                entitlementBadge(ent: ent, state: entState, daysLeft: daysLeft)
            }
            .padding(14)

            if entState == .none || entState == .expired || entState == .pastDue {
                Divider().overlay(RemColor.border)

                VStack(alignment: .leading, spacing: 10) {
                    Text(pausedCardTitle(state: entState))
                        .font(.system(size: 14, weight: .semibold))
                        .foregroundStyle(RemColor.text)

                    Text(pausedCardBody(state: entState, provider: ent?.provider, trialDays: trialDays))
                        .font(.system(size: 12.5))
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
                        if showTrialButton {
                            Button {
                                showPremiumPaywall = true
                            } label: {
                                Text("Upgrade to Premium — \(appState.storeKit.displayPrice)/year")
                            }
                            .buttonStyle(RemSecondaryButtonStyle())
                            .disabled(appState.isStartingTrial || appState.storeKit.isPurchasing)
                        } else {
                            Button {
                                showPremiumPaywall = true
                            } label: {
                                Text("Upgrade to Premium — \(appState.storeKit.displayPrice)/year")
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
                                .font(.system(size: 12.5, weight: .semibold))
                                .foregroundStyle(RemColor.accent)
                        }
                        .frame(maxWidth: .infinity)
                        .padding(.top, 2)
                    }
                    .disabled(appState.storeKit.isRestoring || appState.storeKit.isPurchasing)
                }
                .padding(13)
                .background(entState == .pastDue ? RemColor.hardBg : RemColor.bgInset)
                .clipShape(RoundedRectangle(cornerRadius: 11, style: .continuous))
                .overlay(
                    RoundedRectangle(cornerRadius: 11, style: .continuous)
                        .strokeBorder(
                            entState == .pastDue ? RemColor.hard.opacity(0.28) : RemColor.border,
                            lineWidth: 1
                        )
                )
                .padding(14)
            }

            Divider().overlay(RemColor.border)

            if isPaused || entState == .pastDue {
                Button {
                    Task { await appState.checkAccessAndRetrySync() }
                } label: {
                    HStack {
                        Text("Check access / Retry sync")
                            .font(.system(size: 14))
                            .foregroundStyle(RemColor.text)
                        Spacer()
                        if appState.isCheckingEntitlement || appState.isSyncing {
                            ProgressView().controlSize(.small)
                        } else {
                            Text("Check")
                                .font(.system(size: 13, weight: .semibold))
                                .foregroundStyle(RemColor.accent)
                        }
                    }
                    .padding(14)
                    .contentShape(Rectangle())
                }
                .disabled(appState.isCheckingEntitlement || appState.isSyncing)
            } else {
                Button {
                    Task { await appState.syncNow(silent: false) }
                } label: {
                    HStack {
                        Text("Sync now")
                            .font(.system(size: 14))
                            .foregroundStyle(RemColor.text)
                        Spacer()
                        if appState.isSyncing {
                            ProgressView().controlSize(.small)
                        } else {
                            Text(appState.lastSyncError != nil ? "Retry" : "Run")
                                .font(.system(size: 13, weight: .semibold))
                                .foregroundStyle(RemColor.accent)
                        }
                    }
                    .padding(14)
                    .contentShape(Rectangle())
                }
                .disabled(appState.isSyncing)

                if appState.canShowAppleSubscribe {
                    Divider().overlay(RemColor.border)

                    Button {
                        showPremiumPaywall = true
                    } label: {
                        HStack {
                            VStack(alignment: .leading, spacing: 2) {
                                Text("Upgrade to rem Premium")
                                    .font(.system(size: 14))
                                    .foregroundStyle(RemColor.text)
                                Text(
                                    daysLeft.map { "\($0)d left · \(appState.storeKit.displayPrice)/year" }
                                        ?? "\(appState.storeKit.displayPrice)/year via App Store"
                                )
                                .font(.system(size: 11.5))
                                .foregroundStyle(RemColor.muted)
                            }
                            Spacer()
                            Text("Upgrade")
                                .font(.system(size: 13, weight: .semibold))
                                .foregroundStyle(RemColor.accent)
                        }
                        .padding(14)
                        .contentShape(Rectangle())
                    }
                } else if appState.canShowAppleManageSubscription {
                    Divider().overlay(RemColor.border)

                    Button {
                        Task { await appState.manageAppleSubscription() }
                    } label: {
                        HStack {
                            VStack(alignment: .leading, spacing: 2) {
                                Text("Manage subscription")
                                    .font(.system(size: 14))
                                    .foregroundStyle(RemColor.text)
                                if let dateStr = formattedShortDate(ent?.expiresAt) {
                                    let verb = (ent?.cancelAtPeriodEnd ?? false) ? "Ends" : "Renews"
                                    Text("\(verb) \(dateStr) · App Store")
                                        .font(.system(size: 11.5))
                                        .foregroundStyle(RemColor.muted)
                                } else {
                                    Text("App Store")
                                        .font(.system(size: 11.5))
                                        .foregroundStyle(RemColor.muted)
                                }
                            }
                            Spacer()
                            Image(systemName: "chevron.right")
                                .font(.system(size: 12, weight: .semibold))
                                .foregroundStyle(RemColor.faint)
                        }
                        .padding(14)
                        .contentShape(Rectangle())
                    }
                }
            }

            Divider().overlay(RemColor.border)

            HStack {
                Text("Server")
                    .font(.system(size: 14))
                    .foregroundStyle(RemColor.text)
                Spacer()
                Text(displayServerName)
                    .font(.system(size: 12.5, design: displayServerName == "rem Cloud" ? .default : .monospaced))
                    .foregroundStyle(RemColor.muted)
                    .lineLimit(1)
            }
            .padding(14)

            Divider().overlay(RemColor.border)

            Button {
                Task { await appState.signOut() }
            } label: {
                HStack {
                    Text("Sign out (keep local cards)")
                        .font(.system(size: 14))
                        .foregroundStyle(RemColor.text)
                    Spacer()
                }
                .padding(14)
                .contentShape(Rectangle())
            }

            Divider().overlay(RemColor.border)

            Button(role: .destructive) {
                showDeleteAccountConfirm = true
            } label: {
                HStack {
                    Text("Delete cloud account")
                        .font(.system(size: 14, weight: .medium))
                        .foregroundStyle(RemColor.danger)
                    Spacer()
                }
                .padding(14)
                .contentShape(Rectangle())
            }
        }
    }

    private var signedOutCard: some View {
        VStack(alignment: .leading, spacing: 11) {
            VStack(alignment: .leading, spacing: 3) {
                Text("Local-only mode")
                    .font(.system(size: 14.5, weight: .semibold))
                    .foregroundStyle(RemColor.text)
                Text("Your decks stay on this iPhone. Sign in to sync decks, study progress, and images across your devices.")
                    .font(.system(size: 12.5))
                    .foregroundStyle(RemColor.muted)
                    .lineSpacing(2)
            }

            Button {
                Task {
                    await appState.signIn(
                        provider: .apple,
                        customServerUrl: useCustomServer ? customServerUrl : nil
                    )
                }
            } label: {
                HStack(spacing: 8) {
                    Image(systemName: "apple.logo")
                        .font(.system(size: 15, weight: .semibold))
                    Text(appState.isAuthenticating ? "Signing in…" : "Continue with Apple")
                }
                .font(.system(size: 14.5, weight: .semibold))
                .foregroundStyle(RemColor.bg)
                .frame(maxWidth: .infinity)
                .padding(.vertical, 12)
                .background(RemColor.text)
                .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
            }
            .disabled(appState.isAuthenticating)

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

            Divider().overlay(RemColor.border)
                .padding(.top, 2)

            Button {
                withAnimation(.easeOut(duration: 0.16)) {
                    useCustomServer.toggle()
                }
            } label: {
                HStack {
                    Text("Custom server")
                        .font(.system(size: 12.5, weight: .medium))
                        .foregroundStyle(RemColor.faint)
                    Spacer()
                    Image(systemName: useCustomServer ? "chevron.up" : "chevron.down")
                        .font(.system(size: 10.5, weight: .semibold))
                        .foregroundStyle(RemColor.faint)
                }
            }

            if useCustomServer {
                VStack(alignment: .leading, spacing: 7) {
                    TextField(
                        "https://sync.example.com",
                        text: $customServerUrl
                    )
                    .font(.system(size: 12.5, design: .monospaced))
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
                    .font(.system(size: 11.5))
                }
            }
        }
        .padding(14)
    }

    private var readingAndHabitSection: some View {
        section("READING & HABIT") {
            VStack(spacing: 0) {
                HStack {
                    Text("Card text size")
                        .font(.system(size: 14))
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
                .padding(14)

                Divider().overlay(RemColor.border)

                HStack {
                    Text("Theme")
                        .font(.system(size: 14))
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
                .padding(14)

                Divider().overlay(RemColor.border)

                Toggle(isOn: $appState.hapticsEnabled) {
                    Text("Haptic grade feedback")
                        .font(.system(size: 14))
                        .foregroundStyle(RemColor.text)
                }
                .tint(RemColor.accent)
                .padding(14)

                Divider().overlay(RemColor.border)

                Toggle(isOn: Binding(
                    get: { appState.reminderEnabled },
                    set: { next in Task { await appState.setReminderEnabled(next) } }
                )) {
                    VStack(alignment: .leading, spacing: 2) {
                        Text("Daily reminder")
                            .font(.system(size: 14))
                            .foregroundStyle(RemColor.text)
                        Text("Only notifies if cards are still due")
                            .font(.system(size: 11.5))
                            .foregroundStyle(RemColor.muted)
                    }
                }
                .tint(RemColor.accent)
                .padding(14)

                if appState.reminderEnabled {
                    Divider().overlay(RemColor.border)
                    DatePicker(
                        "Reminder time",
                        selection: $appState.reminderDate,
                        displayedComponents: .hourAndMinute
                    )
                    .font(.system(size: 14))
                    .tint(RemColor.accent)
                    .padding(.horizontal, 14)
                    .padding(.vertical, 10)
                }
            }
        }
    }

    private var yourDataSection: some View {
        section("YOUR DATA") {
            VStack(spacing: 0) {
                HStack {
                    VStack(alignment: .leading, spacing: 2) {
                        Text("On this iPhone")
                            .font(.system(size: 14))
                            .foregroundStyle(RemColor.text)
                        Text(storageSubtitle)
                            .font(.system(size: 12))
                            .foregroundStyle(RemColor.muted)
                    }
                    Spacer()
                }
                .padding(14)

                Divider().overlay(RemColor.border)

                Button {
                    Task {
                        shareBackupURL = await appState.exportBackupFile()
                    }
                } label: {
                    HStack {
                        Text("Export backup")
                            .font(.system(size: 14))
                            .foregroundStyle(RemColor.text)
                        Spacer()
                        Image(systemName: "square.and.arrow.up")
                            .font(.system(size: 13.5))
                            .foregroundStyle(RemColor.muted)
                    }
                    .padding(14)
                    .contentShape(Rectangle())
                }
            }
        }
    }

    // MARK: - Helpers

    private func section<Content: View>(_ title: String, @ViewBuilder content: () -> Content) -> some View {
        VStack(alignment: .leading, spacing: 7) {
            Text(title)
                .font(.system(size: 11, weight: .bold))
                .tracking(0.8)
                .foregroundStyle(RemColor.muted)
                .padding(.horizontal, 2)
            content()
                .background(RemColor.surface)
                .clipShape(RoundedRectangle(cornerRadius: RemRadius.lg, style: .continuous))
                .overlay(
                    RoundedRectangle(cornerRadius: RemRadius.lg, style: .continuous)
                        .strokeBorder(RemColor.border, lineWidth: 1)
                )
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
