import AuthenticationServices
import SwiftUI
import UIKit
import UniformTypeIdentifiers

struct SettingsView: View {
    @Bindable var appState: AppState
    @Environment(\.colorScheme) private var colorScheme

    @State private var useCustomServer: Bool = false
    @State private var customServerUrl: String = ""
    @State private var pendingAppleNonce: String = ""
    @State private var showPremiumPaywall: Bool = false
    @State private var shareBackupURL: URL?
    @State private var showBackupImporter: Bool = false
    @State private var showApkgImporter: Bool = false
    @State private var pendingBackupImport: PendingBackupImport?
    @State private var pendingApkgImport: PendingApkgImport?

    private var apkgImportContentTypes: [UTType] {
        var types: [UTType] = [.zip, .data]
        if let colpkg = UTType(filenameExtension: "colpkg") {
            types.insert(colpkg, at: 0)
        }
        if let apkg = UTType(filenameExtension: "apkg") {
            types.insert(apkg, at: 0)
        }
        return types
    }

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
                if CloudSyncFormatter.isCustomServer(
                    serverUrl: appState.syncStatus?.serverUrl,
                    defaultServerUrl: appState.syncStatus?.defaultServerUrl
                ), let bound = appState.syncStatus?.serverUrl {
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
            .fileImporter(
                isPresented: $showBackupImporter,
                allowedContentTypes: [.json],
                allowsMultipleSelection: false
            ) { result in
                handleBackupFileImportResult(result)
            }
            .fileImporter(
                isPresented: $showApkgImporter,
                allowedContentTypes: apkgImportContentTypes,
                allowsMultipleSelection: false
            ) { result in
                handleApkgFileImportResult(result)
            }
            .alert(
                "Import Anki deck?",
                isPresented: Binding(
                    get: { pendingApkgImport != nil },
                    set: {
                        if !$0 {
                            appState.discardPendingApkgImport(pendingApkgImport)
                            pendingApkgImport = nil
                        }
                    }
                )
            ) {
                if let pending = pendingApkgImport {
                    Button(
                        pending.preview.replaced.isEmpty ? "Import" : "Import & replace",
                        role: pending.preview.replaced.isEmpty ? nil : .destructive
                    ) {
                        let url = pending.tempFileURL
                        pendingApkgImport = nil
                        Task { await appState.commitApkgImport(tempFileURL: url) }
                    }
                }
                Button("Cancel", role: .cancel) {
                    appState.discardPendingApkgImport(pendingApkgImport)
                    pendingApkgImport = nil
                }
            } message: {
                if let preview = pendingApkgImport?.preview {
                    Text(apkgImportAlertMessage(preview))
                }
            }
            .alert(
                "Replace existing decks?",
                isPresented: Binding(
                    get: { pendingBackupImport != nil },
                    set: { if !$0 { pendingBackupImport = nil } }
                )
            ) {
                if let pending = pendingBackupImport {
                    Button("Import & replace", role: .destructive) {
                        pendingBackupImport = nil
                        Task { await appState.commitBackupImport(json: pending.json) }
                    }
                }
                Button("Cancel", role: .cancel) {
                    pendingBackupImport = nil
                }
            } message: {
                if let preview = pendingBackupImport?.preview {
                    let names = preview.replaced.joined(separator: ", ")
                    if preview.added.isEmpty {
                        Text("Importing will replace existing deck(s): \(names).")
                    } else {
                        Text("Importing will replace existing deck(s): \(names). New decks (\(preview.added.count)) will be added alongside them.")
                    }
                }
            }
        }
    }

    // MARK: - Sections

    private var cloudSyncSection: some View {
        let isSignedIn = appState.syncStatus?.signedIn == true
        return Section {
            if isSignedIn {
                signedInRows
            } else {
                signedOutCard
            }
        } header: {
            Text("Cloud Sync")
        } footer: {
            if isSignedIn {
                cloudSyncFooter
            }
        }
        .listRowBackground(RemColor.surface)
        .listRowSeparatorTint(RemColor.border)
    }

    @ViewBuilder
    private var cloudSyncFooter: some View {
        let isPaused = appState.isSyncPaused
        let pending = appState.syncStatus?.pendingOutboxCount ?? 0
        if let err = appState.lastSyncError, !isPaused {
            Text(err)
                .font(.caption)
                .foregroundStyle(RemColor.again)
        } else if !isPaused && pending > 0 {
            let changeWord = pending == 1 ? "change" : "changes"
            Text("\(pending) \(changeWord) waiting to sync")
                .font(.caption)
                .foregroundStyle(RemColor.muted)
        } else if !isPaused, let last = appState.lastSyncedAt {
            Text("Last synced \(CloudSyncFormatter.relativeLastSynced(last))")
                .font(.caption)
                .foregroundStyle(RemColor.muted)
        }
    }

    @ViewBuilder
    private var signedInRows: some View {
        let ent = appState.entitlement
        let entState = appState.effectiveEntitlementState
        let isPaused = appState.isSyncPaused
        let trialDays = Int(ent?.trialDays ?? 30)
        let daysLeft = CloudSyncFormatter.daysRemaining(ent?.expiresAt)

        NavigationLink {
            CloudAccountView(appState: appState)
        } label: {
            HStack(spacing: 12) {
                RemLogoBadge(size: 36)
                VStack(alignment: .leading, spacing: 2) {
                    Text(accountRowTitle(state: entState))
                        .font(.body.weight(.semibold))
                        .foregroundStyle(RemColor.text)
                    if let subtitle = accountRowSubtitle(ent: ent, state: entState, daysLeft: daysLeft) {
                        Text(subtitle)
                            .font(.caption)
                            .foregroundStyle(RemColor.muted)
                            .lineLimit(1)
                    }
                }
                Spacer(minLength: 8)
                syncStatusBadge(state: entState)
            }
            .padding(.vertical, 2)
        }

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
                            }
                            Text(appState.isStartingTrial ? "Starting trial…" : "Start \(trialDays)-day free trial")
                                .fontWeight(.semibold)
                        }
                        .frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.borderedProminent)
                    .controlSize(.large)
                    .tint(RemColor.accent)
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
                                .fontWeight(.semibold)
                                .frame(maxWidth: .infinity)
                        }
                        .buttonStyle(.bordered)
                        .controlSize(.large)
                        .tint(RemColor.accent)
                        .disabled(appState.isStartingTrial || appState.storeKit.isPurchasing)
                    } else {
                        Button {
                            showPremiumPaywall = true
                        } label: {
                            Text(upgradeTitle)
                                .fontWeight(.semibold)
                                .frame(maxWidth: .infinity)
                        }
                        .buttonStyle(.borderedProminent)
                        .controlSize(.large)
                        .tint(RemColor.accent)
                        .disabled(appState.storeKit.isPurchasing)
                        .padding(.top, 2)
                    }
                }

                if entState == .pastDue && appState.canShowAppleManageSubscription {
                    Button {
                        Task { await appState.manageAppleSubscription() }
                    } label: {
                        Text("Manage subscription")
                            .fontWeight(.semibold)
                            .frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.borderedProminent)
                    .controlSize(.large)
                    .tint(RemColor.accent)
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
                    Text(appState.isCheckingEntitlement || appState.isSyncing ? "Checking access…" : "Check access & retry sync")
                        .foregroundStyle(
                            (appState.isCheckingEntitlement || appState.isSyncing) ? RemColor.muted : RemColor.accent
                        )
                    Spacer()
                    if appState.isCheckingEntitlement || appState.isSyncing {
                        ProgressView().controlSize(.small)
                    }
                }
            }
            .disabled(appState.isCheckingEntitlement || appState.isSyncing)
        } else {
            if appState.canShowAppleSubscribe {
                Button {
                    showPremiumPaywall = true
                } label: {
                    HStack {
                        VStack(alignment: .leading, spacing: 2) {
                            Text("Upgrade to rem Premium")
                                .foregroundStyle(RemColor.accent)
                            if let price = appState.storeKit.localizedPrice {
                                Text("\(price)/year via App Store")
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

            Button {
                Task { await appState.syncNow(silent: false) }
            } label: {
                HStack {
                    Text(syncActionLabel)
                        .foregroundStyle(appState.isSyncing ? RemColor.muted : RemColor.accent)
                    Spacer()
                    if appState.isSyncing {
                        ProgressView().controlSize(.small)
                    }
                }
            }
            .disabled(appState.isSyncing)
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
                        .fontWeight(.semibold)
                }
                .frame(maxWidth: .infinity)
            }
            .buttonStyle(.bordered)
            .controlSize(.large)
            .tint(RemColor.text)
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

            Button {
                showBackupImporter = true
            } label: {
                HStack {
                    Text(appState.isImportingBackup ? "Importing backup…" : "Import backup (JSON)")
                        .foregroundStyle(appState.isImportingBackup ? RemColor.muted : RemColor.text)
                    Spacer()
                    if appState.isImportingBackup {
                        ProgressView().controlSize(.small)
                    } else {
                        Image(systemName: "square.and.arrow.down")
                            .foregroundStyle(RemColor.muted)
                    }
                }
            }
            .disabled(appState.isImportingBackup || appState.isImportingApkg)

            Button {
                showApkgImporter = true
            } label: {
                HStack {
                    Text(appState.isImportingApkg ? "Importing Anki deck…" : "Import Anki deck (.apkg)")
                        .foregroundStyle(appState.isImportingApkg ? RemColor.muted : RemColor.text)
                    Spacer()
                    if appState.isImportingApkg {
                        ProgressView().controlSize(.small)
                    } else {
                        Image(systemName: "tray.and.arrow.down")
                            .foregroundStyle(RemColor.muted)
                    }
                }
            }
            .disabled(appState.isImportingBackup || appState.isImportingApkg)
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
                    Text("·").foregroundStyle(RemColor.borderStrong)
                    Link("Support", destination: RemDocs.supportURL)
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

    private func handleBackupFileImportResult(_ result: Result<[URL], Error>) {
        switch result {
        case .success(let urls):
            guard let url = urls.first else { return }
            Task {
                if let pending = await appState.prepareBackupImport(from: url) {
                    if pending.preview.replaced.isEmpty {
                        await appState.commitBackupImport(json: pending.json)
                    } else {
                        pendingBackupImport = pending
                    }
                }
            }
        case .failure(let error):
            appState.showToast("Could not open file: \(error.remDisplayMessage)")
        }
    }

    private func handleApkgFileImportResult(_ result: Result<[URL], Error>) {
        switch result {
        case .success(let urls):
            guard let url = urls.first else { return }
            Task {
                if let pending = await appState.prepareApkgImport(from: url) {
                    pendingApkgImport = pending
                }
            }
        case .failure(let error):
            appState.showToast("Could not open file: \(error.remDisplayMessage)")
        }
    }

    private func apkgImportAlertMessage(_ preview: ApkgImportPreviewDto) -> String {
        let deckCount = preview.added.count + preview.replaced.count
        let deckWord = deckCount == 1 ? "deck" : "decks"
        let cardWord = preview.totalCards == 1 ? "card" : "cards"
        let mediaPart = preview.mediaCount > 0 ? ", \(preview.mediaCount) media" : ""
        let revPart = preview.reviewLogs > 0 ? ", \(preview.reviewLogs) review logs" : ""
        var msg: String
        if preview.replaced.isEmpty {
            msg = "Found \(deckCount) \(deckWord), \(preview.totalCards) \(cardWord)\(mediaPart)\(revPart)."
        } else {
            let names = preview.replaced.joined(separator: ", ")
            msg = "Found \(deckCount) \(deckWord), \(preview.totalCards) \(cardWord)\(mediaPart)\(revPart). Existing deck(s) will be replaced: \(names)."
        }
        if !preview.warnings.isEmpty {
            let shown = preview.warnings.prefix(2).joined(separator: " ")
            msg += "\n\nNote: \(shown)"
        }
        return msg
    }

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

    private var syncActionLabel: String {
        if appState.isSyncing {
            return "Syncing…"
        }
        return appState.lastSyncError != nil ? "Retry sync" : "Sync now"
    }

    private func accountRowTitle(state: EntitlementStateDto) -> String {
        switch state {
        case .selfHosted: return "Self-hosted sync"
        case .none: return "rem Cloud"
        case .active, .trialing, .pastDue, .expired: return "rem Premium"
        }
    }

    private func accountRowSubtitle(
        ent: EntitlementInfoDto?,
        state: EntitlementStateDto,
        daysLeft: Int?
    ) -> String? {
        switch state {
        case .active:
            if let dateStr = CloudSyncFormatter.formattedShortDate(ent?.expiresAt) {
                let verb = (ent?.cancelAtPeriodEnd ?? false) ? "Ends" : "Renews"
                return "\(verb) \(dateStr)"
            }
            return "Active"
        case .trialing:
            return daysLeft.map { "Free trial · \($0)d left" } ?? "Free trial"
        case .pastDue:
            return "Payment issue"
        case .expired:
            return "Subscription ended"
        case .none:
            return "Not active"
        case .selfHosted:
            return CloudSyncFormatter.displayServerHost(serverUrl: appState.syncStatus?.serverUrl) ?? "Custom server"
        }
    }

    private func syncStatusBadge(state: EntitlementStateDto) -> some View {
        let pending = appState.syncStatus?.pendingOutboxCount ?? 0
        switch state {
        case .pastDue:
            return RemPillBadge(text: "Past due", tone: .hard)
        case .expired, .none:
            return RemPillBadge(text: "Paused", tone: .hard)
        case .active, .trialing, .selfHosted:
            if appState.lastSyncError != nil {
                return RemPillBadge(text: pending > 0 ? "Pending" : "Error", tone: pending > 0 ? .hard : .again)
            }
            if pending > 0 {
                return RemPillBadge(text: "\(pending) pending", tone: .hard)
            }
            return RemPillBadge(text: "Synced", tone: .good)
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

// MARK: - Account Detail Screen

struct CloudAccountView: View {
    @Bindable var appState: AppState
    @Environment(\.dismiss) private var dismiss

    @State private var showSignOutConfirm: Bool = false
    @State private var showDeleteAccountConfirm: Bool = false
    @State private var showPremiumPaywall: Bool = false

    var body: some View {
        let ent = appState.entitlement
        let entState = appState.effectiveEntitlementState

        Form {
            if entState != .selfHosted {
                subscriptionSection(ent: ent, state: entState)
            }
            accountIdentitySection
            signOutSection
            deleteAccountSection
        }
        .scrollContentBackground(.hidden)
        .background(RemColor.groupedBg)
        .navigationTitle("Account")
        .navigationBarTitleDisplayMode(.inline)
        .onAppear {
            if appState.canShowAppleSubscribe && appState.storeKit.product == nil {
                Task { await appState.storeKit.loadProducts() }
            }
        }
        .onChange(of: appState.syncStatus?.signedIn) { _, isSignedIn in
            if isSignedIn != true {
                dismiss()
            }
        }
        .alert("Sign out?", isPresented: $showSignOutConfirm) {
            Button("Sign Out", role: .destructive) {
                Task {
                    await appState.signOut()
                    if appState.syncStatus?.signedIn != true {
                        dismiss()
                    }
                }
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("Your decks and cards will stay on this iPhone. Cloud sync will pause until you sign in again.")
        }
        .alert("Delete cloud account?", isPresented: $showDeleteAccountConfirm) {
            Button("Delete cloud data", role: .destructive) {
                Task {
                    await appState.deleteCloudAccount()
                    if appState.syncStatus?.signedIn != true {
                        dismiss()
                    }
                }
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            if appState.canShowAppleManageSubscription {
                Text("Permanently removes your synced data and images from the sync server. Your local cards on this iPhone remain intact. Deleting your cloud account does not cancel your App Store subscription — cancel it in iOS Settings → Apple Account → Subscriptions.")
            } else {
                Text("Permanently removes your synced data and images from the sync server. Your local cards on this iPhone remain intact.")
            }
        }
        .sheet(isPresented: $showPremiumPaywall) {
            PremiumPaywallSheet(appState: appState)
        }
    }

    private func subscriptionSection(ent: EntitlementInfoDto?, state: EntitlementStateDto) -> some View {
        Section("Subscription") {
            HStack {
                Text("Plan")
                    .foregroundStyle(RemColor.text)
                Spacer()
                Text(planDisplayName(state: state))
                    .foregroundStyle(state == .pastDue ? RemColor.hard : RemColor.muted)
            }

            if let billedVia = CloudSyncFormatter.billingProviderDisplayName(ent?.provider) {
                HStack {
                    Text("Billed via")
                        .foregroundStyle(RemColor.text)
                    Spacer()
                    Text(billedVia)
                        .foregroundStyle(RemColor.muted)
                }
            }

            if let dateLabel = renewalDateLabel(ent: ent, state: state),
               let dateStr = CloudSyncFormatter.formattedShortDate(ent?.expiresAt) {
                HStack {
                    Text(dateLabel)
                        .foregroundStyle(RemColor.text)
                    Spacer()
                    Text(dateStr)
                        .foregroundStyle(RemColor.muted)
                }
            }

            if appState.canShowAppleManageSubscription {
                Button {
                    Task { await appState.manageAppleSubscription() }
                } label: {
                    HStack {
                        Text("Manage subscription")
                            .foregroundStyle(RemColor.text)
                        Spacer()
                        Image(systemName: "chevron.right")
                            .font(.caption.weight(.semibold))
                            .foregroundStyle(RemColor.faint)
                    }
                }
            } else if appState.canShowAppleSubscribe {
                Button {
                    showPremiumPaywall = true
                } label: {
                    HStack {
                        Text("Upgrade to rem Premium")
                            .foregroundStyle(RemColor.accent)
                        Spacer()
                        if let price = appState.storeKit.localizedPrice {
                            Text("\(price)/yr")
                                .font(.subheadline)
                                .foregroundStyle(RemColor.muted)
                        }
                        Image(systemName: "chevron.right")
                            .font(.caption.weight(.semibold))
                            .foregroundStyle(RemColor.faint)
                    }
                }
            }

            if appState.canShowAppleSubscribe || state == .expired || state == .none {
                Button {
                    Task { _ = await appState.restoreApplePurchases() }
                } label: {
                    HStack {
                        Text(appState.storeKit.isRestoring ? "Restoring purchases…" : "Restore purchases")
                            .foregroundStyle(appState.storeKit.isRestoring ? RemColor.muted : RemColor.accent)
                        Spacer()
                        if appState.storeKit.isRestoring {
                            ProgressView().controlSize(.small)
                        }
                    }
                }
                .disabled(appState.storeKit.isRestoring || appState.storeKit.isPurchasing)
            }
        }
        .listRowBackground(RemColor.surface)
        .listRowSeparatorTint(RemColor.border)
    }

    private var accountIdentitySection: some View {
        let status = appState.syncStatus
        let visibleEmail = CloudSyncFormatter.visibleEmail(status?.email)
        let isCustomServer = CloudSyncFormatter.isCustomServer(
            serverUrl: status?.serverUrl,
            defaultServerUrl: status?.defaultServerUrl
        )

        return Section("Cloud Account") {
            HStack {
                Text("Signed in with")
                    .foregroundStyle(RemColor.text)
                Spacer()
                Text(CloudSyncFormatter.providerDisplayName(status?.provider))
                    .foregroundStyle(RemColor.muted)
            }

            if let email = visibleEmail {
                HStack {
                    Text("Email")
                        .foregroundStyle(RemColor.text)
                    Spacer(minLength: 12)
                    Text(email)
                        .foregroundStyle(RemColor.muted)
                        .lineLimit(1)
                        .truncationMode(.middle)
                }
            }

            if isCustomServer, let host = CloudSyncFormatter.displayServerHost(serverUrl: status?.serverUrl) {
                HStack {
                    Text("Server")
                        .foregroundStyle(RemColor.text)
                    Spacer(minLength: 12)
                    Text(host)
                        .font(.caption.monospaced())
                        .foregroundStyle(RemColor.muted)
                        .lineLimit(1)
                        .truncationMode(.middle)
                }
            }
        }
        .listRowBackground(RemColor.surface)
        .listRowSeparatorTint(RemColor.border)
    }

    private var signOutSection: some View {
        Section {
            Button {
                showSignOutConfirm = true
            } label: {
                Text("Sign out")
                    .foregroundStyle(RemColor.text)
            }
        }
        .listRowBackground(RemColor.surface)
        .listRowSeparatorTint(RemColor.border)
    }

    private var deleteAccountSection: some View {
        Section {
            Button(role: .destructive) {
                showDeleteAccountConfirm = true
            } label: {
                Text("Delete cloud account")
            }
        } footer: {
            Text("Signing out or deleting your cloud account never deletes cards stored on this iPhone.")
                .font(.caption)
                .foregroundStyle(RemColor.muted)
        }
        .listRowBackground(RemColor.surface)
        .listRowSeparatorTint(RemColor.border)
    }

    private func planDisplayName(state: EntitlementStateDto) -> String {
        switch state {
        case .active: return "rem Premium"
        case .trialing: return "Free trial"
        case .pastDue: return "Payment issue"
        case .expired: return "Expired"
        case .none: return "Not active"
        case .selfHosted: return "Self-hosted"
        }
    }

    private func renewalDateLabel(ent: EntitlementInfoDto?, state: EntitlementStateDto) -> String? {
        switch state {
        case .trialing: return "Trial ends"
        case .expired: return "Ended"
        case .active, .pastDue: return (ent?.cancelAtPeriodEnd ?? false) ? "Ends" : "Renews"
        case .none, .selfHosted: return nil
        }
    }
}

// MARK: - Formatting Helpers

enum CloudSyncFormatter {
    private static let relativeSyncFormatter: RelativeDateTimeFormatter = {
        let formatter = RelativeDateTimeFormatter()
        formatter.unitsStyle = .abbreviated
        return formatter
    }()

    static func relativeLastSynced(_ date: Date, now: Date = Date()) -> String {
        if now.timeIntervalSince(date) < 60 {
            return "just now"
        }
        return relativeSyncFormatter.localizedString(for: date, relativeTo: now)
    }

    /// Returns a clean user-facing email, hiding empty values and Apple Hide My Email relay addresses.
    static func visibleEmail(_ rawEmail: String?) -> String? {
        guard let trimmed = rawEmail?.trimmingCharacters(in: .whitespacesAndNewlines),
              !trimmed.isEmpty
        else {
            return nil
        }
        if trimmed.lowercased().hasSuffix("@privaterelay.appleid.com") {
            return nil
        }
        return trimmed
    }

    static func daysRemaining(_ expiresAt: Int64?, nowMs: Int64 = RemStore.nowMs()) -> Int? {
        guard let expiresAt else { return nil }
        let remainingMs = max(0, expiresAt - nowMs)
        return max(1, Int((remainingMs + 86_400_000 - 1) / 86_400_000))
    }

    static func formattedShortDate(_ epochMs: Int64?) -> String? {
        guard let epochMs else { return nil }
        let date = Date(timeIntervalSince1970: TimeInterval(epochMs) / 1000.0)
        return date.formatted(date: .abbreviated, time: .omitted)
    }

    static func isCustomServer(serverUrl: String?, defaultServerUrl: String?) -> Bool {
        guard let serverUrl, let defaultServerUrl else { return false }
        let normalizedServer = serverUrl.trimmingCharacters(in: CharacterSet(charactersIn: "/")).lowercased()
        let normalizedDefault = defaultServerUrl.trimmingCharacters(in: CharacterSet(charactersIn: "/")).lowercased()
        return !normalizedServer.isEmpty && normalizedServer != normalizedDefault
    }

    static func displayServerHost(serverUrl: String?) -> String? {
        guard let serverUrl else { return nil }
        let trimmed = serverUrl
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .trimmingCharacters(in: CharacterSet(charactersIn: "/"))
        guard !trimmed.isEmpty else { return nil }
        if trimmed.lowercased().hasPrefix("https://") {
            return String(trimmed.dropFirst("https://".count))
        }
        if trimmed.lowercased().hasPrefix("http://") {
            return String(trimmed.dropFirst("http://".count))
        }
        return trimmed
    }

    static func providerDisplayName(_ provider: String?) -> String {
        switch provider?.lowercased() {
        case "apple": return "Apple"
        case "google": return "Google"
        default: return "Cloud"
        }
    }

    static func billingProviderDisplayName(_ provider: String?) -> String? {
        switch provider?.lowercased() {
        case "apple": return "App Store"
        case "polar", "web", "stripe": return "Web"
        default: return nil
        }
    }
}

struct ActivityShareSheet: UIViewControllerRepresentable {
    let items: [Any]

    func makeUIViewController(context: Context) -> UIActivityViewController {
        UIActivityViewController(activityItems: items, applicationActivities: nil)
    }

    func updateUIViewController(_ uiViewController: UIActivityViewController, context: Context) {}
}
