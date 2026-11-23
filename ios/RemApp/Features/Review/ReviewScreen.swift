import SwiftUI
import UIKit

/// Zero-chrome focused review screen driven by shared Rust `RemStudySession` (FSRS-7).
struct ReviewScreen: View {
    @Bindable var appState: AppState
    let deckId: String?

    @Environment(\.dismiss) private var dismiss
    @State private var session: RemStudySession?
    @State private var viewState: StudyViewDto?
    @State private var loadError: String?

    @State private var sessionPassedCount: UInt32 = 0
    @State private var showMenuSheet: Bool = false
    @State private var editingCard: CardDto?
    @State private var lightboxAsset: CachedAssetImage?

    private var sessionProgress: (currentIndex: Int, total: Int, fraction: Double) {
        let total = max(1, Int((viewState?.reviewed ?? 0) + (viewState?.remaining ?? 0)))
        let currentIndex = min(total, Int(viewState?.reviewed ?? 0) + (viewState?.current == nil ? 0 : 1))
        let fraction = viewState?.current == nil ? 1.0 : Double(currentIndex) / Double(total)
        return (currentIndex, total, fraction)
    }

    var body: some View {
        NavigationStack {
            VStack(spacing: 0) {
                progressBar

                if let loadError {
                    VStack(spacing: 12) {
                        Text("Could not start session")
                            .font(.headline)
                            .foregroundStyle(RemColor.text)
                        Text(loadError)
                            .font(.footnote)
                            .foregroundStyle(RemColor.muted)
                            .multilineTextAlignment(.center)
                        Button("Back to Today") { dismiss() }
                            .buttonStyle(RemSecondaryButtonStyle())
                            .frame(maxWidth: 200)
                    }
                    .padding(24)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                } else if let viewState {
                    if let card = viewState.current {
                        reviewCanvas(card: card, viewState: viewState)
                        bottomDock(viewState: viewState)
                    } else {
                        sessionCompleteView(viewState: viewState)
                    }
                } else {
                    ProgressView()
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                }
            }
            .background(RemColor.bg)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button {
                        dismiss()
                    } label: {
                        Image(systemName: "xmark")
                    }
                    .accessibilityLabel("End session")
                }

                ToolbarItem(placement: .principal) {
                    let info = sessionProgress
                    VStack(spacing: 1) {
                        Text(viewState?.current == nil ? "Session complete" : (viewState?.deckName ?? "Review"))
                            .font(.subheadline.weight(.semibold))
                            .foregroundStyle(RemColor.text)
                            .lineLimit(1)
                        if viewState?.current != nil {
                            Text("\(info.currentIndex) / \(info.total)")
                                .font(.caption2.weight(.semibold).monospaced())
                                .foregroundStyle(RemColor.muted)
                        }
                    }
                }

                if viewState?.current != nil || (viewState?.canUndo == true) {
                    ToolbarItem(placement: .topBarTrailing) {
                        Button {
                            showMenuSheet = true
                        } label: {
                            Image(systemName: "ellipsis")
                        }
                        .accessibilityLabel("Card and session options")
                    }
                }
            }
        }
        .tint(RemColor.accent)
        .task { await startSession() }
        .sheet(isPresented: $showMenuSheet) {
            ReviewMenuSheet(
                appState: appState,
                canUndo: viewState?.canUndo ?? false,
                hasCurrentCard: viewState?.current != nil,
                onEdit: {
                    editingCard = viewState?.current
                },
                onUndo: {
                    Task { await undoLastGrade() }
                },
                onSuspend: {
                    Task { await suspendCurrentCard() }
                }
            )
        }
        .sheet(item: $editingCard) { card in
            QuickCaptureSheet(
                appState: appState,
                initialDeckId: card.deckId,
                editingCard: card,
                onSaved: {
                    Task { await reloadCurrentCard() }
                }
            )
        }
        .fullScreenCover(item: $lightboxAsset) { asset in
            ImageLightboxView(asset: asset)
        }
    }

    private var progressBar: some View {
        let progress = sessionProgress.fraction
        return GeometryReader { geo in
            ZStack(alignment: .leading) {
                Rectangle().fill(RemColor.border)
                Rectangle()
                    .fill(RemColor.accent)
                    .frame(width: geo.size.width * progress)
                    .animation(.easeOut(duration: 0.22), value: progress)
            }
        }
        .frame(height: 2)
    }

    private func reviewCanvas(card: CardDto, viewState: StudyViewDto) -> some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 14) {
                if let notice = viewState.leechNotice {
                    HStack(spacing: 8) {
                        Image(systemName: "exclamationmark.triangle.fill")
                            .font(.system(size: 12))
                            .foregroundStyle(RemColor.hard)
                        Text(notice == .suspended
                             ? "Previous card was suspended as a leech."
                             : "Previous card was tagged as a leech.")
                            .font(.system(size: 12.5, weight: .medium))
                            .foregroundStyle(RemColor.hard)
                    }
                    .padding(.horizontal, 12)
                    .padding(.vertical, 9)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(RemColor.hardBg)
                    .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
                }

                Text("QUESTION")
                    .font(.system(size: 10.5, weight: .bold))
                    .tracking(0.9)
                    .foregroundStyle(RemColor.faint)

                RemMarkdownView(
                    markdown: card.front,
                    store: appState.store,
                    baseFontSize: viewState.revealed
                        ? appState.cardTextSize.revealedQuestionSize
                        : appState.cardTextSize.questionSize,
                    codeFontSize: appState.cardTextSize.codeSize,
                    textColor: viewState.revealed ? RemColor.muted : RemColor.text,
                    baseWeight: viewState.revealed ? .regular : .medium,
                    onTapAssetImage: { asset in
                        lightboxAsset = asset
                    }
                )

                if viewState.revealed {
                    Divider()
                        .overlay(RemColor.border)
                        .padding(.vertical, 6)

                    Text("ANSWER")
                        .font(.system(size: 10.5, weight: .bold))
                        .tracking(0.9)
                        .foregroundStyle(RemColor.faint)

                    RemMarkdownView(
                        markdown: card.back.isEmpty ? "_No answer recorded_" : card.back,
                        store: appState.store,
                        baseFontSize: appState.cardTextSize.answerSize,
                        codeFontSize: appState.cardTextSize.codeSize,
                        textColor: RemColor.text,
                        baseWeight: .regular,
                        onTapAssetImage: { asset in
                            lightboxAsset = asset
                        }
                    )

                    if !card.tags.isEmpty {
                        ScrollView(.horizontal, showsIndicators: false) {
                            HStack(spacing: 6) {
                                ForEach(card.tags, id: \.self) { tag in
                                    Text("#\(tag)")
                                        .font(.system(size: 11.5, weight: .medium))
                                        .foregroundStyle(RemColor.muted)
                                        .padding(.horizontal, 9)
                                        .padding(.vertical, 4)
                                        .background(RemColor.bgInset)
                                        .clipShape(Capsule())
                                        .overlay(Capsule().strokeBorder(RemColor.border, lineWidth: 1))
                                }
                            }
                        }
                        .padding(.top, 6)
                    }
                } else {
                    Text("Tap anywhere to reveal answer")
                        .font(.system(size: 12.5))
                        .foregroundStyle(RemColor.faint)
                        .frame(maxWidth: .infinity)
                        .padding(.top, 36)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 20)
            .padding(.vertical, 22)
        }
        .contentShape(Rectangle())
        .onTapGesture {
            if !viewState.revealed {
                Task { await reveal() }
            }
        }
    }

    private func bottomDock(viewState: StudyViewDto) -> some View {
        VStack(spacing: 0) {
            Divider().overlay(RemColor.border)
            if !viewState.revealed {
                Button("Show answer") {
                    Task { await reveal() }
                }
                .buttonStyle(RemPrimaryButtonStyle())
                .padding(.horizontal, 16)
                .padding(.top, 12)
                .padding(.bottom, 16)
            } else {
                let intervals = viewState.intervals
                HStack(spacing: 7) {
                    gradeButton(.again, title: "Again", interval: intervals?.again ?? "", color: RemColor.again)
                    gradeButton(.hard, title: "Hard", interval: intervals?.hard ?? "", color: RemColor.hard)
                    gradeButton(.good, title: "Good", interval: intervals?.good ?? "", color: RemColor.good)
                    gradeButton(.easy, title: "Easy", interval: intervals?.easy ?? "", color: RemColor.easy)
                }
                .padding(.horizontal, 14)
                .padding(.top, 12)
                .padding(.bottom, 16)
            }
        }
        .background(RemColor.bg)
    }

    private func gradeButton(_ grade: GradeDto, title: String, interval: String, color: Color) -> some View {
        Button {
            Task { await submitGrade(grade) }
        } label: {
            VStack(spacing: 3) {
                Text(title)
                    .font(.system(size: 13, weight: .semibold))
                    .foregroundStyle(color)
                Text(interval)
                    .font(.system(size: 11, design: .monospaced))
                    .foregroundStyle(RemColor.muted)
            }
            .frame(maxWidth: .infinity)
            .padding(.vertical, 11)
            .background(RemColor.surface)
            .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
            .overlay(
                RoundedRectangle(cornerRadius: 10, style: .continuous)
                    .strokeBorder(RemColor.border, lineWidth: 1)
            )
        }
    }

    private func sessionCompleteView(viewState: StudyViewDto) -> some View {
        let recallPct = viewState.reviewed > 0
            ? Int((Double(sessionPassedCount) / Double(viewState.reviewed) * 100.0).rounded())
            : 100
        let syncNote: String = {
            if appState.syncStatus?.signedIn != true {
                return "Saved locally on this iPhone."
            }
            if appState.lastSyncError != nil {
                return "Saved locally · will sync when online."
            }
            return "Ready to sync with rem Cloud."
        }()

        return VStack(spacing: 12) {
            ZStack {
                RoundedRectangle(cornerRadius: 16, style: .continuous)
                    .fill(RemColor.goodBg)
                    .frame(width: 54, height: 54)
                Image(systemName: "checkmark")
                    .font(.system(size: 24, weight: .bold))
                    .foregroundStyle(RemColor.good)
            }
            .padding(.bottom, 4)

            Text("All caught up.")
                .font(.system(size: 27, weight: .bold))
                .tracking(-0.6)
                .foregroundStyle(RemColor.text)

            Text("Reviewed \(viewState.reviewed) \(viewState.reviewed == 1 ? "card" : "cards") · \(recallPct)% recall.\n\(syncNote)")
                .font(.system(size: 14))
                .foregroundStyle(RemColor.muted)
                .multilineTextAlignment(.center)
                .lineSpacing(3)

            Button("Back to Today") {
                dismiss()
            }
            .buttonStyle(RemPrimaryButtonStyle())
            .frame(maxWidth: 220)
            .padding(.top, 10)
        }
        .padding(28)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    // MARK: - Actions

    private func startSession() async {
        do {
            let started = try await appState.store.startStudy(deckId: deckId)
            let initial = try started.currentView(nowMs: RemStore.nowMs())
            self.session = started
            self.viewState = initial
            self.sessionPassedCount = 0
        } catch {
            self.loadError = error.localizedDescription
        }
    }

    private func reveal() async {
        guard let session else { return }
        if appState.hapticsEnabled {
            UIImpactFeedbackGenerator(style: .light).impactOccurred()
        }
        do {
            let next = try session.reveal(nowMs: RemStore.nowMs())
            withAnimation(.easeOut(duration: 0.18)) {
                self.viewState = next
            }
        } catch {
            appState.showToast(error.localizedDescription)
        }
    }

    private func submitGrade(_ grade: GradeDto) async {
        guard let session else { return }
        if appState.hapticsEnabled {
            UIImpactFeedbackGenerator(style: grade == .again ? .rigid : .medium).impactOccurred()
        }
        do {
            let outcome = try session.grade(grade: grade, nowMs: RemStore.nowMs())
            if outcome.conflictCardId != nil {
                appState.showToast("Skipped card edited elsewhere")
            } else if grade != .again {
                sessionPassedCount += 1
            }
            self.viewState = outcome.view
        } catch {
            appState.showToast(error.localizedDescription)
        }
    }

    private func undoLastGrade() async {
        guard let session else { return }
        do {
            let restored = try session.undoLastGrade(nowMs: RemStore.nowMs())
            if sessionPassedCount > restored.reviewed {
                sessionPassedCount = restored.reviewed
            }
            withAnimation(.easeOut(duration: 0.16)) {
                self.viewState = restored
            }
            appState.showToast("Restored previous card")
        } catch {
            appState.showToast(error.localizedDescription)
        }
    }

    private func suspendCurrentCard() async {
        guard let session else { return }
        do {
            let next = try session.suspendCurrent(nowMs: RemStore.nowMs())
            self.viewState = next
            appState.showToast("Card suspended")
        } catch {
            appState.showToast(error.localizedDescription)
        }
    }

    private func reloadCurrentCard() async {
        guard let session else { return }
        do {
            let reloaded = try session.reloadCurrent(nowMs: RemStore.nowMs())
            self.viewState = reloaded
        } catch {
            appState.showToast(error.localizedDescription)
        }
    }
}

extension CardDto: Identifiable {}
