import SwiftUI

struct SettingsView: View {
    @Bindable var appState: AppState

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                Text("Settings")
                    .font(.system(size: 28, weight: .bold))
                    .tracking(-0.6)
                    .foregroundStyle(RemColor.text)

                section("CLOUD SYNC") {
                    VStack(spacing: 0) {
                        HStack {
                            VStack(alignment: .leading, spacing: 3) {
                                Text(appState.syncStatus?.signedIn == true ? "rem Cloud" : "Local-only mode")
                                    .font(.system(size: 14.5, weight: .semibold))
                                    .foregroundStyle(RemColor.text)
                                Text(syncSubtitle)
                                    .font(.system(size: 12))
                                    .foregroundStyle(RemColor.muted)
                            }
                            Spacer()
                            RemPillBadge(
                                text: appState.syncStatus?.signedIn == true ? "Active" : "Local",
                                tone: appState.syncStatus?.signedIn == true ? .good : .neutral
                            )
                        }
                        .padding(14)

                        if appState.syncStatus?.signedIn == true {
                            Divider().overlay(RemColor.border)
                            Button {
                                Task { await appState.syncNow(silent: false) }
                            } label: {
                                HStack {
                                    Text("Sync now")
                                        .font(.system(size: 14))
                                        .foregroundStyle(RemColor.text)
                                    Spacer()
                                    Text(appState.isSyncing ? "Syncing…" : "Run")
                                        .font(.system(size: 13, weight: .semibold))
                                        .foregroundStyle(RemColor.accent)
                                }
                                .padding(14)
                            }
                        }
                    }
                }

                section("READING & APPEARANCE") {
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
                    }
                }

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
                    }
                }
            }
            .padding(.horizontal, 18)
            .padding(.top, 10)
            .padding(.bottom, 104)
        }
        .background(RemColor.bg)
    }

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

    private var syncSubtitle: String {
        guard let status = appState.syncStatus else { return "Checking local database…" }
        if !status.signedIn {
            return "Decks stay strictly on this iPhone. Cloud sign-in arrives in Slice 4D."
        }
        if status.pendingOutboxCount > 0 {
            return "\(status.pendingOutboxCount) pending changes queued"
        }
        return "Connected to \(status.serverUrl ?? status.defaultServerUrl)"
    }

    private var storageSubtitle: String {
        guard let summary = appState.storageSummary else { return "Calculating…" }
        let mb = Double(summary.assetBytes) / 1_048_576.0
        return "\(summary.totalCards) cards · \(summary.assetCount) cached assets · \(String(format: "%.2f", mb)) MB"
    }
}
