import SwiftUI
import UIKit

struct SettingsView: View {
    @Bindable var appState: AppState

    @State private var useCustomServer: Bool = false
    @State private var customServerUrl: String = ""
    @State private var showDeleteAccountConfirm: Bool = false
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
                localStorageSection
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
        }
        .alert("Delete cloud account?", isPresented: $showDeleteAccountConfirm) {
            Button("Delete cloud data", role: .destructive) {
                Task { await appState.deleteCloudAccount() }
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("Permanently removes your synced data and images from the sync server. Your local cards on this iPhone remain intact.")
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
            if let mismatch = appState.pendingAccountSwitch {
                Text("This device is currently bound to another account (\(mismatch.boundAccountId.prefix(8))…). Resetting sync binding keeps your local cards and connects the new account.")
            }
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
                VStack(spacing: 0) {
                    HStack(alignment: .top) {
                        VStack(alignment: .leading, spacing: 3) {
                            Text("rem Cloud")
                                .font(.system(size: 14.5, weight: .semibold))
                                .foregroundStyle(RemColor.text)
                            Text(signedInSubtitle)
                                .font(.system(size: 12))
                                .foregroundStyle(RemColor.muted)
                            if let err = appState.lastSyncError {
                                Text(err)
                                    .font(.system(size: 11.5))
                                    .foregroundStyle(RemColor.again)
                                    .padding(.top, 2)
                            }
                        }
                        Spacer()
                        statusBadge
                    }
                    .padding(14)

                    Divider().overlay(RemColor.border)

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

                    Divider().overlay(RemColor.border)

                    HStack {
                        Text("Server")
                            .font(.system(size: 14))
                            .foregroundStyle(RemColor.text)
                        Spacer()
                        Text(displayServerName)
                            .font(.system(size: 12.5, design: .monospaced))
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
            } else {
                VStack(alignment: .leading, spacing: 12) {
                    VStack(alignment: .leading, spacing: 3) {
                        Text("Local-only mode")
                            .font(.system(size: 14.5, weight: .semibold))
                            .foregroundStyle(RemColor.text)
                        Text("Your decks stay strictly on this iPhone. Sign in to sync decks, FSRS schedules, and images across Mac, CLI, and iPhone.")
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
                            Text(appState.isAuthenticating ? "Signing in…" : "Continue with Apple")
                        }
                        .font(.system(size: 14.5, weight: .semibold))
                        .foregroundStyle(RemColor.bg)
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 12.5)
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
                        Text(appState.isAuthenticating ? "Signing in…" : "Continue with Google")
                    }
                    .buttonStyle(RemSecondaryButtonStyle())
                    .disabled(appState.isAuthenticating)

                    Divider().overlay(RemColor.border)

                    Button {
                        withAnimation(.easeOut(duration: 0.16)) {
                            useCustomServer.toggle()
                        }
                    } label: {
                        HStack {
                            Text("Self-hosted server URL")
                                .font(.system(size: 13, weight: .medium))
                                .foregroundStyle(RemColor.muted)
                            Spacer()
                            Image(systemName: useCustomServer ? "chevron.up" : "chevron.down")
                                .font(.system(size: 11, weight: .semibold))
                                .foregroundStyle(RemColor.faint)
                        }
                    }

                    if useCustomServer {
                        VStack(alignment: .leading, spacing: 6) {
                            Text("Point the standard app at your own Cloudflare Worker:")
                                .font(.system(size: 11.5))
                                .foregroundStyle(RemColor.muted)
                            TextField(
                                appState.syncStatus?.defaultServerUrl ?? "https://rem-sync.shettyh.fyi",
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
                        }
                    }
                }
                .padding(14)
            }
        }
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

    private var localStorageSection: some View {
        section("LOCAL STORAGE") {
            VStack(spacing: 0) {
                HStack {
                    VStack(alignment: .leading, spacing: 2) {
                        Text("SQLite & offline images")
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
                        Text("Export JSON backup")
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

    private var statusBadge: some View {
        let pending = appState.syncStatus?.pendingOutboxCount ?? 0
        if appState.lastSyncError != nil {
            return RemPillBadge(text: pending > 0 ? "Queued" : "Error", tone: pending > 0 ? .hard : .again)
        }
        if pending > 0 {
            return RemPillBadge(text: "\(pending) queued", tone: .hard)
        }
        return RemPillBadge(text: "Synced", tone: .good)
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
            return "\(identity) · \(status.pendingOutboxCount) ops queued"
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
        let mb = Double(summary.assetBytes) / 1_048_576.0
        return "\(summary.totalCards) cards · \(summary.assetCount) cached assets · \(String(format: "%.2f", mb)) MB"
    }
}

struct ActivityShareSheet: UIViewControllerRepresentable {
    let items: [Any]

    func makeUIViewController(context: Context) -> UIActivityViewController {
        UIActivityViewController(activityItems: items, applicationActivities: nil)
    }

    func updateUIViewController(_ uiViewController: UIActivityViewController, context: Context) {}
}
