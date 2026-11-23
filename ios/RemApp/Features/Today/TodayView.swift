import SwiftUI

struct TodayView: View {
    @Bindable var appState: AppState
    @State private var deckPendingDelete: DeckSummaryDto?

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    topBar

                    VStack(alignment: .leading, spacing: 4) {
                        Text(greetingText)
                            .font(.system(size: 29, weight: .bold))
                            .tracking(-0.7)
                            .foregroundStyle(RemColor.text)
                        Text(subtitleText)
                            .font(.system(size: 14))
                            .foregroundStyle(RemColor.muted)
                    }

                    heroSection

                    decksSection
                }
                .padding(.horizontal, 18)
                .padding(.top, 4)
                .padding(.bottom, 24)
            }
            .background(RemColor.bg)
            .refreshable {
                if appState.isSyncPaused {
                    await appState.refreshAccount(triggerSyncIfUnlocked: true)
                } else {
                    await appState.syncNow(silent: true)
                }
                await appState.refreshAll()
            }
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button {
                        appState.openQuickCapture()
                    } label: {
                        Image(systemName: "plus")
                    }
                    .accessibilityLabel("New card")
                }
            }
            .navigationDestination(for: String.self) { deckId in
                DeckDetailView(appState: appState, deckId: deckId)
            }
            .alert(
                "Delete deck?",
                isPresented: Binding(
                    get: { deckPendingDelete != nil },
                    set: { if !$0 { deckPendingDelete = nil } }
                )
            ) {
                if let deck = deckPendingDelete {
                    Button("Delete", role: .destructive) {
                        deckPendingDelete = nil
                        Task { await appState.deleteDeck(id: deck.id) }
                    }
                }
                Button("Cancel", role: .cancel) {
                    deckPendingDelete = nil
                }
            } message: {
                Text("This deletes the deck and all its cards locally and on synced devices.")
            }
        }
    }

    private var topBar: some View {
        HStack {
            Text(formattedDate)
                .font(.system(size: 12.5, weight: .medium))
                .foregroundStyle(RemColor.muted)
            Spacer()
            SyncStatusChip(appState: appState)
        }
    }

    @ViewBuilder
    private var heroSection: some View {
        let summary = appState.todaySummary
        let totalDue = summary?.totalDue ?? 0

        if appState.decks.isEmpty {
            RemCardSurface(padding: 20) {
                VStack(spacing: 10) {
                    Text("No decks yet")
                        .font(.system(size: 16, weight: .bold))
                        .foregroundStyle(RemColor.text)
                    Text("Create your first deck or connect rem Cloud in Settings to sync your desktop cards.")
                        .font(.system(size: 13))
                        .foregroundStyle(RemColor.muted)
                        .multilineTextAlignment(.center)
                    Button("Create first deck") {
                        appState.showNewDeckSheet = true
                    }
                    .buttonStyle(RemPrimaryButtonStyle())
                    .padding(.top, 4)
                }
            }
        } else if totalDue == 0 {
            RemCardSurface(padding: 22) {
                VStack(spacing: 10) {
                    ZStack {
                        RoundedRectangle(cornerRadius: 13, style: .continuous)
                            .fill(RemColor.goodBg)
                            .frame(width: 44, height: 44)
                        Image(systemName: "checkmark")
                            .font(.system(size: 20, weight: .bold))
                            .foregroundStyle(RemColor.good)
                    }
                    Text("All caught up for today")
                        .font(.system(size: 16, weight: .bold))
                        .foregroundStyle(RemColor.text)

                    if let summary, summary.reviewedToday > 0 {
                        let recallText = summary.recallTodayPercent.map { " · \($0)% recall" } ?? ""
                        let streakText = summary.currentStreakDays > 0 ? " · \(summary.currentStreakDays)d streak" : ""
                        Text("\(summary.reviewedToday) reviewed today\(recallText)\(streakText)")
                            .font(.system(size: 13))
                            .foregroundStyle(RemColor.muted)
                    } else {
                        Text("Tap + above whenever you want to capture a new card.")
                            .font(.system(size: 13))
                            .foregroundStyle(RemColor.muted)
                    }
                }
                .frame(maxWidth: .infinity)
            }
        } else if let summary {
            RemCardSurface(padding: 16) {
                VStack(spacing: 14) {
                    HStack(alignment: .firstTextBaseline) {
                        HStack(alignment: .firstTextBaseline, spacing: 12) {
                            Text("\(summary.totalDue)")
                                .font(.system(size: 40, weight: .bold))
                                .tracking(-1.4)
                                .foregroundStyle(RemColor.text)
                            VStack(alignment: .leading, spacing: 2) {
                                Text(summary.totalDue == 1 ? "card due" : "cards due")
                                    .font(.system(size: 14.5, weight: .semibold))
                                    .foregroundStyle(RemColor.text)
                                Text("\(summary.newDue) new · \(summary.reviewDue) review · ~\(summary.estimatedMinutes) min")
                                    .font(.system(size: 12))
                                    .foregroundStyle(RemColor.muted)
                            }
                        }
                        Spacer()
                        if summary.currentStreakDays > 0 {
                            RemPillBadge(text: "\(summary.currentStreakDays)d streak", tone: .neutral)
                        }
                    }

                    Button {
                        appState.startStudy(deckId: nil)
                    } label: {
                        HStack(spacing: 8) {
                            Text("Start review")
                            Image(systemName: "arrow.right")
                                .font(.system(size: 13, weight: .semibold))
                                .opacity(0.8)
                        }
                    }
                    .buttonStyle(RemPrimaryButtonStyle())
                }
            }
        }
    }

    @ViewBuilder
    private var decksSection: some View {
        let deckSummaries = appState.todaySummary?.decks ?? []
        if !deckSummaries.isEmpty {
            VStack(alignment: .leading, spacing: 8) {
                HStack {
                    Text("YOUR DECKS")
                        .font(.system(size: 11, weight: .bold))
                        .tracking(0.8)
                        .foregroundStyle(RemColor.muted)
                    Spacer()
                    Text("\(deckSummaries.count) \(deckSummaries.count == 1 ? "deck" : "decks")")
                        .font(.system(size: 12))
                        .foregroundStyle(RemColor.muted)
                }
                .padding(.horizontal, 2)
                .padding(.top, 4)

                VStack(spacing: 0) {
                    ForEach(Array(deckSummaries.enumerated()), id: \.element.id) { idx, deck in
                        NavigationLink(value: deck.id) {
                            DeckSummaryRow(deck: deck)
                        }
                        .buttonStyle(.plain)
                        .contextMenu {
                            Button {
                                appState.openEditDeck(deckId: deck.id)
                            } label: {
                                Label("Edit deck", systemImage: "slider.horizontal.3")
                            }
                            Button(role: .destructive) {
                                deckPendingDelete = deck
                            } label: {
                                Label("Delete deck", systemImage: "trash")
                            }
                        }

                        if idx < deckSummaries.count - 1 {
                            Divider().overlay(RemColor.border)
                        }
                    }
                }
                .background(RemColor.surface)
                .clipShape(RoundedRectangle(cornerRadius: RemRadius.lg, style: .continuous))
                .overlay(
                    RoundedRectangle(cornerRadius: RemRadius.lg, style: .continuous)
                        .strokeBorder(RemColor.border, lineWidth: 1)
                )
            }
        }
    }

    private var greetingText: String {
        let hour = Calendar.current.component(.hour, from: Date())
        switch hour {
        case 5..<12: return "Good morning."
        case 12..<17: return "Good afternoon."
        default: return "Good evening."
        }
    }

    private var subtitleText: String {
        let deckCount = appState.decks.count
        if deckCount == 0 {
            return "Ready when you are."
        }
        let due = appState.todaySummary?.totalDue ?? 0
        if due == 0 {
            return "All caught up across \(deckCount) \(deckCount == 1 ? "deck" : "decks")."
        }
        return "\(due) \(due == 1 ? "card" : "cards") due across \(deckCount) \(deckCount == 1 ? "deck" : "decks")."
    }

    private var formattedDate: String {
        let formatter = DateFormatter()
        formatter.dateFormat = "EEE, MMM d"
        return formatter.string(from: Date())
    }
}

struct DeckSummaryRow: View {
    let deck: DeckSummaryDto
    var showChevron: Bool = true

    var body: some View {
        HStack(spacing: 10) {
            VStack(alignment: .leading, spacing: 3) {
                HStack(spacing: 8) {
                    Circle()
                        .fill(Color(hexString: deck.color))
                        .frame(width: 8, height: 8)
                    Text(deck.name)
                        .font(.system(size: 15, weight: .semibold))
                        .foregroundStyle(RemColor.text)
                        .lineLimit(1)
                }

                Text(subtitleText)
                    .font(.system(size: 12))
                    .foregroundStyle(RemColor.muted)
            }

            Spacer()

            HStack(spacing: 8) {
                statusPill
                if showChevron {
                    Image(systemName: "chevron.right")
                        .font(.system(size: 12, weight: .semibold))
                        .foregroundStyle(RemColor.faint)
                }
            }
        }
        .padding(.horizontal, showChevron ? 14 : 0)
        .padding(.vertical, showChevron ? 13 : 4)
        .contentShape(Rectangle())
    }

    private var subtitleText: String {
        let countLabel = "\(deck.totalCards) \(deck.totalCards == 1 ? "card" : "cards")"
        if deck.newDueCount > 0 {
            return "\(countLabel) · \(deck.newDueCount) new"
        }
        return countLabel
    }

    private var statusPill: some View {
        if deck.dueCount > 0 {
            return RemPillBadge(text: "\(deck.dueCount) due", tone: .accent)
        }
        if deck.totalCards > 0 {
            return RemPillBadge(text: "✓ Done", tone: .good)
        }
        return RemPillBadge(text: "Empty", tone: .neutral)
    }
}

struct SyncStatusChip: View {
    @Bindable var appState: AppState

    var body: some View {
        Button {
            if isCloud && !isPaused {
                Task { await appState.syncNow(silent: false) }
            } else {
                appState.selectedTab = .settings
            }
        } label: {
            HStack(spacing: 5) {
                if appState.isSyncing {
                    ProgressView()
                        .controlSize(.mini)
                } else {
                    Circle()
                        .fill(dotColor)
                        .frame(width: 6, height: 6)
                }
                Text(labelText)
                    .font(.system(size: 11.5, weight: .semibold))
                    .foregroundStyle(textColor)
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 5)
            .background(bgColor)
            .clipShape(Capsule())
            .overlay(Capsule().strokeBorder(borderColor, lineWidth: 1))
        }
    }

    private var isCloud: Bool {
        appState.syncStatus?.mode == .cloud && appState.syncStatus?.signedIn == true
    }

    private var isPaused: Bool {
        isCloud && (appState.isSyncPaused || appState.effectiveEntitlementState == .pastDue)
    }

    private var pendingCount: UInt64 {
        appState.syncStatus?.pendingOutboxCount ?? 0
    }

    private var labelText: String {
        if appState.isSyncing { return "Syncing…" }
        if !isCloud { return "Local only" }
        if isPaused { return "Sync paused" }
        if appState.lastSyncError != nil {
            return pendingCount > 0 ? "Offline · \(pendingCount) pending" : "Sync error · Retry"
        }
        if pendingCount > 0 {
            return "\(pendingCount) pending"
        }
        return "Synced"
    }

    private var dotColor: Color {
        if !isCloud { return RemColor.faint }
        if isPaused { return RemColor.hard }
        if appState.lastSyncError != nil { return pendingCount > 0 ? RemColor.hard : RemColor.again }
        if pendingCount > 0 { return RemColor.hard }
        return RemColor.good
    }

    private var textColor: Color {
        if isPaused { return RemColor.hard }
        if appState.lastSyncError != nil {
            return pendingCount > 0 ? RemColor.hard : RemColor.again
        }
        return RemColor.muted
    }

    private var bgColor: Color {
        if isPaused { return RemColor.hardBg }
        if appState.lastSyncError != nil {
            return pendingCount > 0 ? RemColor.hardBg : RemColor.againBg
        }
        return RemColor.bgInset
    }

    private var borderColor: Color {
        if isPaused { return RemColor.hard.opacity(0.3) }
        if appState.lastSyncError != nil {
            return (pendingCount > 0 ? RemColor.hard : RemColor.again).opacity(0.3)
        }
        return RemColor.border
    }
}
