import SwiftUI

struct TodayView: View {
    @Bindable var appState: AppState
    @State private var deckPendingDelete: DeckSummaryDto?

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    VStack(alignment: .leading, spacing: 4) {
                        HStack(alignment: .center, spacing: 12) {
                            Text(formattedDate)
                                .font(.footnote.weight(.medium))
                                .foregroundStyle(RemColor.muted)
                            Spacer()
                            SyncStatusChip(appState: appState)
                        }
                        Text(greetingText)
                            .font(.largeTitle.bold())
                            .tracking(-0.6)
                            .foregroundStyle(RemColor.text)
                        Text(subtitleText)
                            .font(.subheadline)
                            .foregroundStyle(RemColor.muted)
                    }

                    heroSection

                    decksSection
                }
                .padding(.horizontal, 18)
                .padding(.top, 8)
                .padding(.bottom, 24)
            }
            .background(RemColor.groupedBg)
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
                    Button {
                        appState.showNewDeckSheet = true
                    } label: {
                        Text("Create first deck")
                            .fontWeight(.semibold)
                            .frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.borderedProminent)
                    .controlSize(.large)
                    .tint(RemColor.accent)
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
                    HStack(alignment: .center) {
                        HStack(alignment: .center, spacing: 12) {
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
                                .fontWeight(.semibold)
                            Image(systemName: "arrow.right")
                                .font(.subheadline.weight(.semibold))
                                .opacity(0.85)
                        }
                        .frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.borderedProminent)
                    .controlSize(.large)
                    .tint(RemColor.accent)
                }
            }
        }
    }

    @ViewBuilder
    private var decksSection: some View {
        let deckSummaries = appState.todaySummary?.decks ?? []
        let treeNodes = DeckTreeBuilder.flatten(
            DeckTreeBuilder.build(from: deckSummaries),
            collapsed: appState.collapsedDeckPaths
        )
        if !treeNodes.isEmpty {
            VStack(alignment: .leading, spacing: 8) {
                HStack {
                    Text("YOUR DECKS")
                        .font(.caption2.weight(.bold))
                        .tracking(0.8)
                        .foregroundStyle(RemColor.muted)
                    Spacer()
                    Text("\(deckSummaries.count) \(deckSummaries.count == 1 ? "deck" : "decks")")
                        .font(.caption)
                        .foregroundStyle(RemColor.muted)
                }
                .padding(.horizontal, 2)
                .padding(.top, 4)

                VStack(spacing: 0) {
                    ForEach(Array(treeNodes.enumerated()), id: \.element.id) { idx, node in
                        if let deck = node.deck {
                            let isCollapsed = appState.collapsedDeckPaths.contains(node.path)
                            HStack(spacing: 0) {
                                if !node.children.isEmpty {
                                    Button {
                                        toggleCollapse(node.path)
                                    } label: {
                                        Image(systemName: isCollapsed ? "chevron.right" : "chevron.down")
                                            .font(.system(size: 11, weight: .bold))
                                            .foregroundStyle(RemColor.muted)
                                            .frame(width: 34, height: 40)
                                            .contentShape(Rectangle())
                                    }
                                    .buttonStyle(.plain)
                                    .padding(.leading, CGFloat(min(node.depth, 4)) * 14 + 4)
                                    .accessibilityLabel(isCollapsed ? "Expand \(node.label)" : "Collapse \(node.label)")
                                }

                                NavigationLink(value: deck.id) {
                                    DeckSummaryRow(
                                        deck: deck,
                                        displayName: node.label,
                                        depth: node.children.isEmpty ? node.depth : 0,
                                        dueOverride: node.dueCount,
                                        newDueOverride: node.newDueCount,
                                        totalOverride: node.totalCards,
                                        leadingInset: node.children.isEmpty ? 14 : 2,
                                        onStudy: node.dueCount > 0 ? {
                                            appState.startStudy(deckId: deck.id)
                                        } : nil
                                    )
                                }
                                .buttonStyle(.plain)
                            }
                            .contextMenu {
                                if node.dueCount > 0 {
                                    Button {
                                        appState.startStudy(deckId: deck.id)
                                    } label: {
                                        Label("Study due cards (\(node.dueCount))", systemImage: "play.fill")
                                    }
                                }
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
                        } else {
                            VirtualDeckFolderRow(
                                node: node,
                                isCollapsed: appState.collapsedDeckPaths.contains(node.path)
                            ) {
                                toggleCollapse(node.path)
                            }
                        }

                        if idx < treeNodes.count - 1 {
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

    private func toggleCollapse(_ path: String) {
        withAnimation(.easeOut(duration: 0.16)) {
            appState.toggleDeckCollapse(path)
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
        Date.now.formatted(.dateTime.weekday(.abbreviated).month(.abbreviated).day())
    }
}

struct DeckSummaryRow: View {
    let deck: DeckSummaryDto
    var showChevron: Bool = true
    var displayName: String? = nil
    var depth: Int = 0
    var dueOverride: UInt32? = nil
    var newDueOverride: UInt32? = nil
    var totalOverride: UInt32? = nil
    var leadingInset: CGFloat? = nil
    var onStudy: (() -> Void)? = nil

    private var effectiveName: String { displayName ?? deck.name }
    private var effectiveDue: UInt32 { dueOverride ?? deck.dueCount }
    private var effectiveNewDue: UInt32 { newDueOverride ?? deck.newDueCount }
    private var effectiveTotal: UInt32 { totalOverride ?? deck.totalCards }

    var body: some View {
        HStack(spacing: 8) {
            if depth > 0 {
                Color.clear
                    .frame(width: CGFloat(min(depth, 4)) * 14, height: 1)
            }

            VStack(alignment: .leading, spacing: 3) {
                HStack(spacing: 8) {
                    Circle()
                        .fill(Color(hexString: deck.color))
                        .frame(width: 8, height: 8)
                    Text(effectiveName)
                        .font(.subheadline.weight(.semibold))
                        .foregroundStyle(RemColor.text)
                        .lineLimit(1)
                }

                Text(subtitleText)
                    .font(.caption)
                    .foregroundStyle(RemColor.muted)
            }

            Spacer()

            HStack(spacing: 8) {
                if let onStudy, effectiveDue > 0 {
                    Button(action: onStudy) {
                        statusPill
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel("Study \(effectiveDue) due in \(effectiveName)")
                } else {
                    statusPill
                }
                if showChevron {
                    Image(systemName: "chevron.right")
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(RemColor.faint)
                }
            }
        }
        .padding(.leading, leadingInset ?? (showChevron ? 14 : 0))
        .padding(.trailing, showChevron ? 14 : 0)
        .padding(.vertical, showChevron ? 13 : 4)
        .contentShape(Rectangle())
    }

    private var subtitleText: String {
        let countLabel = "\(effectiveTotal) \(effectiveTotal == 1 ? "card" : "cards")"
        if effectiveNewDue > 0 {
            return "\(countLabel) · \(effectiveNewDue) new"
        }
        return countLabel
    }

    private var statusPill: some View {
        if effectiveDue > 0 {
            return RemPillBadge(text: "\(effectiveDue) due", tone: .accent)
        }
        if effectiveTotal > 0 {
            return RemPillBadge(text: "✓ Done", tone: .good)
        }
        return RemPillBadge(text: "Empty", tone: .neutral)
    }
}

struct VirtualDeckFolderRow: View {
    let node: DeckTreeNode
    let isCollapsed: Bool
    var insetPadding: Bool = true
    let onToggle: () -> Void

    var body: some View {
        Button(action: onToggle) {
            HStack(spacing: 8) {
                if node.depth > 0 {
                    Color.clear
                        .frame(width: CGFloat(min(node.depth, 4)) * 14, height: 1)
                }

                Image(systemName: isCollapsed ? "chevron.right" : "chevron.down")
                    .font(.system(size: 10.5, weight: .bold))
                    .foregroundStyle(RemColor.muted)
                    .frame(width: 20, height: 24)

                VStack(alignment: .leading, spacing: 3) {
                    HStack(spacing: 7) {
                        Image(systemName: "folder")
                            .font(.caption)
                            .foregroundStyle(RemColor.muted)
                        Text(node.label)
                            .font(.subheadline.weight(.semibold))
                            .foregroundStyle(RemColor.text)
                            .lineLimit(1)
                    }
                    let countLabel = "\(node.totalCards) \(node.totalCards == 1 ? "card" : "cards")"
                    let newPart = node.newDueCount > 0 ? " · \(node.newDueCount) new" : ""
                    Text("\(countLabel)\(newPart)")
                        .font(.caption)
                        .foregroundStyle(RemColor.muted)
                }

                Spacer()

                if node.dueCount > 0 {
                    RemPillBadge(text: "\(node.dueCount) due", tone: .accent)
                } else if node.totalCards > 0 {
                    RemPillBadge(text: "✓ Done", tone: .good)
                }
            }
            .padding(.horizontal, insetPadding ? 14 : 0)
            .padding(.vertical, insetPadding ? 13 : 4)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
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
            if needsAttention {
                statusLabel(font: .caption.weight(.semibold))
                    .padding(.horizontal, 9)
                    .padding(.vertical, 3.5)
                    .background(bgColor)
                    .clipShape(Capsule())
                    .overlay(Capsule().strokeBorder(borderColor, lineWidth: 1))
            } else {
                statusLabel(font: .footnote.weight(.medium))
                    .padding(.vertical, 2)
                    .contentShape(Rectangle())
            }
        }
        .buttonStyle(.plain)
        .accessibilityHint(isCloud && !isPaused ? "Sync now" : "Open sync settings")
    }

    private func statusLabel(font: Font) -> some View {
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
                .font(font)
                .foregroundStyle(textColor)
        }
    }

    private var needsAttention: Bool {
        !appState.isSyncing && isCloud && (isPaused || appState.lastSyncError != nil)
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
        if appState.isSyncing || !isCloud { return RemColor.muted }
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
