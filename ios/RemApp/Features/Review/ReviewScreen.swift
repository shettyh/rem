import SwiftUI
import UIKit

/// Focused Quiet Paper review screen driven by shared Rust `RemStudySession` (FSRS).
struct ReviewScreen: View {
    @Bindable var appState: AppState
    let deckId: String?
    let customStudy: CustomStudySpec?

    init(appState: AppState, deckId: String?, customStudy: CustomStudySpec? = nil) {
        self.appState = appState
        self.deckId = deckId
        self.customStudy = customStudy
    }

    @Environment(\.dismiss) private var dismiss
    @State private var session: RemStudySession?
    @State private var viewState: StudyViewDto?
    @State private var loadError: String?

    @State private var sessionPassedCount: UInt32 = 0
    @State private var lastGradedPassed: Bool?
    @State private var sessionStartedAt: Date = Date()
    @State private var sessionFinishedAt: Date?
    @State private var isMutating: Bool = false
    @State private var showCardInfo: Bool = false
    @State private var editingCard: CardDto?
    @State private var lightboxAsset: CachedAssetImage?
    @State private var completeBounceTrigger: Int = 0
    @State private var cardElapsedSeconds: Int = 0
    @State private var timerCardId: String?

    private var timerTaskId: String {
        let cardId = viewState?.current?.id ?? ""
        let revealed = viewState?.revealed ?? false
        let showTimer = viewState?.showTimer ?? false
        return "\(cardId):\(revealed):\(showTimer)"
    }

    private var sessionProgress: (currentIndex: Int, total: Int, fraction: Double) {
        let total = max(1, Int((viewState?.reviewed ?? 0) + (viewState?.remaining ?? 0)))
        let currentIndex = min(total, Int(viewState?.reviewed ?? 0) + (viewState?.current == nil ? 0 : 1))
        let fraction = viewState?.current == nil ? 1.0 : Double(currentIndex) / Double(total)
        return (currentIndex, total, fraction)
    }

    private var currentDeckColor: Color? {
        guard let card = viewState?.current,
              let hex = appState.decks.first(where: { $0.id == card.deckId })?.color
        else { return nil }
        return Color(hexString: hex)
    }

    var body: some View {
        NavigationStack {
            VStack(spacing: 0) {
                if viewState?.current != nil {
                    progressBar
                }

                if let loadError {
                    ContentUnavailableView {
                        Label("Could not start session", systemImage: "exclamationmark.triangle")
                    } description: {
                        Text(loadError)
                    } actions: {
                        Button("Back to Today") { dismiss() }
                            .buttonStyle(.borderedProminent)
                            .tint(RemColor.accent)
                    }
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
            .toolbarBackground(RemColor.bg, for: .navigationBar)
            .toolbarBackground(.visible, for: .navigationBar)
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
                        HStack(spacing: 6) {
                            if let dotColor = currentDeckColor, viewState?.current != nil {
                                Circle()
                                    .fill(dotColor)
                                    .frame(width: 7, height: 7)
                            }
                            Text(principalTitle)
                                .font(.subheadline.weight(.semibold))
                                .foregroundStyle(RemColor.text)
                                .lineLimit(1)
                        }
                        if viewState?.current != nil {
                            Text("\(info.currentIndex) / \(info.total)")
                                .font(.caption2.weight(.semibold).monospacedDigit())
                                .foregroundStyle(RemColor.muted)
                        }
                    }
                }

                ToolbarItemGroup(placement: .topBarTrailing) {
                    if viewState?.canUndo == true {
                        Button {
                            Task { await undoLastGrade() }
                        } label: {
                            Image(systemName: "arrow.uturn.backward")
                        }
                        .disabled(isMutating)
                        .accessibilityLabel("Undo last grade")
                    }

                    if viewState?.current != nil {
                        Menu {
                            Button {
                                editingCard = viewState?.current
                            } label: {
                                Label("Edit card", systemImage: "pencil")
                            }

                            Button {
                                showCardInfo = true
                            } label: {
                                Label("Card info", systemImage: "info.circle")
                            }

                            Menu {
                                Picker("Text size", selection: $appState.cardTextSize) {
                                    ForEach(CardTextSize.allCases) { size in
                                        Text(size.menuLabel).tag(size)
                                    }
                                }
                            } label: {
                                Label("Text size", systemImage: "textformat.size")
                            }

                            if viewState?.preview != true {
                                Divider()

                                Button(role: .destructive) {
                                    Task { await suspendCurrentCard() }
                                } label: {
                                    Label("Suspend card", systemImage: "pause.circle")
                                }
                            }
                        } label: {
                            Image(systemName: "ellipsis.circle")
                        }
                        .disabled(isMutating)
                        .accessibilityLabel("Card and session options")
                    }
                }
            }
        }
        .tint(RemColor.accent)
        .remToastOverlay(appState: appState, enabled: editingCard == nil && !showCardInfo)
        .task { await startSession() }
        .task(id: timerTaskId) {
            guard let viewState, let card = viewState.current else {
                timerCardId = nil
                cardElapsedSeconds = 0
                return
            }
            if timerCardId != card.id {
                timerCardId = card.id
                cardElapsedSeconds = 0
            }
            guard viewState.showTimer && !viewState.revealed else { return }
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(1))
                guard !Task.isCancelled else { break }
                cardElapsedSeconds += 1
            }
        }
        .sheet(isPresented: $showCardInfo) {
            if let card = viewState?.current {
                ReviewCardInfoSheet(card: card, deckName: viewState?.deckName)
            }
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

    private var principalTitle: String {
        guard let viewState else { return "Review" }
        if viewState.current == nil {
            if viewState.reviewed == 0 {
                return customStudy != nil ? "No matching cards" : "Nothing due"
            }
            return viewState.preview ? "Preview complete" : "Session complete"
        }
        return viewState.deckName ?? "Review"
    }

    private var progressBar: some View {
        let progress = sessionProgress.fraction
        return GeometryReader { geo in
            ZStack(alignment: .leading) {
                Rectangle().fill(RemColor.border.opacity(0.65))
                Rectangle()
                    .fill(RemColor.accent)
                    .frame(width: geo.size.width * progress)
                    .animation(.easeOut(duration: 0.22), value: progress)
            }
        }
        .frame(height: 2.5)
    }

    private func reviewCanvas(card: CardDto, viewState: StudyViewDto) -> some View {
        GeometryReader { viewport in
            ScrollViewReader { proxy in
                ScrollView {
                    VStack(alignment: .leading, spacing: 14) {
                        Color.clear
                            .frame(height: 0)
                            .id("card-top")

                        if let notice = viewState.leechNotice {
                            HStack(spacing: 8) {
                                Image(systemName: "exclamationmark.triangle.fill")
                                    .font(.caption)
                                    .foregroundStyle(RemColor.hard)
                                Text(notice == .suspended
                                     ? "Previous card was suspended as a leech."
                                     : "Previous card was tagged as a leech.")
                                    .font(.footnote.weight(.medium))
                                    .foregroundStyle(RemColor.hard)
                            }
                            .padding(.horizontal, 12)
                            .padding(.vertical, 9)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .background(RemColor.hardBg)
                            .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
                        }

                        let isCloze = card.clozeIndex != nil
                        let hasBack = !card.back.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty

                        questionEyebrow(card: card, viewState: viewState)

                        RemMarkdownView(
                            markdown: card.front,
                            store: appState.store,
                            baseFontSize: (!isCloze && viewState.revealed)
                                ? appState.cardTextSize.revealedQuestionSize
                                : appState.cardTextSize.questionSize,
                            codeFontSize: appState.cardTextSize.codeSize,
                            textColor: (!isCloze && viewState.revealed) ? RemColor.muted : RemColor.text,
                            baseWeight: (!isCloze && viewState.revealed) ? .regular : .medium,
                            clozeIndex: card.clozeIndex,
                            clozeRevealed: viewState.revealed,
                            onTapAssetImage: { asset in
                                lightboxAsset = asset
                            }
                        )

                        if viewState.revealed {
                            VStack(alignment: .leading, spacing: 14) {
                                if !isCloze || hasBack {
                                    Divider()
                                        .overlay(RemColor.border)
                                        .padding(.vertical, 6)
                                        .id("answer-section")

                                    Text(isCloze ? "EXTRA" : "ANSWER")
                                        .font(.caption2.weight(.bold))
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
                                } else {
                                    Color.clear
                                        .frame(height: 0)
                                        .id("answer-section")
                                }

                                if !card.tags.isEmpty {
                                    ScrollView(.horizontal, showsIndicators: false) {
                                        HStack(spacing: 6) {
                                            ForEach(card.tags, id: \.self) { tag in
                                                Text("#\(tag)")
                                                    .font(.caption.weight(.medium))
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

                                if card.hasProvenance {
                                    CardProvenanceView(card: card, style: .disclosure)
                                        .padding(.top, 8)
                                }
                            }
                            .transition(.opacity.combined(with: .offset(y: 6)))
                        }
                    }
                    .id(card.id)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 22)
                    .padding(.top, viewState.revealed ? 20 : 28)
                    .padding(.bottom, viewState.revealed ? 32 : 76)
                    .frame(
                        minHeight: viewport.size.height,
                        alignment: viewState.revealed ? .top : .center
                    )
                    .contentShape(Rectangle())
                    .onTapGesture {
                        if !viewState.revealed && !isMutating {
                            Task { await reveal() }
                        }
                    }
                }
                .onChange(of: card.id) { _, _ in
                    proxy.scrollTo("card-top", anchor: .top)
                }
                .onChange(of: viewState.revealed) { _, revealed in
                    if revealed {
                        DispatchQueue.main.asyncAfter(deadline: .now() + 0.05) {
                            withAnimation(.easeOut(duration: 0.22)) {
                                proxy.scrollTo("answer-section")
                            }
                        }
                    }
                }
            }
        }
    }

    @ViewBuilder
    private func questionEyebrow(card: CardDto, viewState: StudyViewDto) -> some View {
        HStack(spacing: 6) {
            Text(card.clozeIndex != nil ? "CLOZE" : "QUESTION")
                .font(.caption2.weight(.bold))
                .tracking(0.9)
                .foregroundStyle(RemColor.faint)

            if let clozeIdx = card.clozeIndex {
                Text("· c\(clozeIdx)")
                    .font(.caption2.weight(.bold).monospaced())
                    .foregroundStyle(RemColor.accent)
            }

            if card.state == 0 {
                Text("· NEW")
                    .font(.caption2.weight(.bold))
                    .tracking(0.7)
                    .foregroundStyle(RemColor.accent)
            } else if card.state == 1 || card.state == 3 {
                Text("· LEARNING")
                    .font(.caption2.weight(.bold))
                    .tracking(0.7)
                    .foregroundStyle(RemColor.hard)
            }

            Spacer(minLength: 8)

            if viewState.showTimer {
                Text(String(format: "%d:%02d", cardElapsedSeconds / 60, cardElapsedSeconds % 60))
                    .font(.caption2.weight(.semibold).monospacedDigit())
                    .foregroundStyle(RemColor.muted)
                    .accessibilityLabel("Elapsed answer time")
            }
        }
    }

    private func bottomDock(viewState: StudyViewDto) -> some View {
        VStack(spacing: 0) {
            Divider().overlay(RemColor.border)
            Group {
                if !viewState.revealed {
                    Button {
                        Task { await reveal() }
                    } label: {
                        Text("Show answer")
                            .font(.body.weight(.semibold))
                            .foregroundStyle(RemColor.onAccent)
                            .frame(maxWidth: .infinity)
                            .frame(height: 54)
                            .background(RemColor.accent)
                            .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
                    }
                    .buttonStyle(.plain)
                    .disabled(isMutating)
                } else if viewState.preview {
                    Button {
                        Task { await advancePreview() }
                    } label: {
                        Text("Next card")
                            .font(.body.weight(.semibold))
                            .foregroundStyle(RemColor.onAccent)
                            .frame(maxWidth: .infinity)
                            .frame(height: 54)
                            .background(RemColor.accent)
                            .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
                    }
                    .buttonStyle(.plain)
                    .disabled(isMutating)
                } else {
                    let intervals = viewState.intervals
                    HStack(spacing: 8) {
                        gradeButton(
                            .again,
                            title: "Again",
                            interval: intervals?.again ?? "",
                            color: RemColor.again,
                            bg: RemColor.againBg
                        )
                        gradeButton(
                            .hard,
                            title: "Hard",
                            interval: intervals?.hard ?? "",
                            color: RemColor.hard,
                            bg: RemColor.hardBg
                        )
                        gradeButton(
                            .good,
                            title: "Good",
                            interval: intervals?.good ?? "",
                            color: RemColor.good,
                            bg: RemColor.goodBg,
                            emphasized: true
                        )
                        gradeButton(
                            .easy,
                            title: "Easy",
                            interval: intervals?.easy ?? "",
                            color: RemColor.easy,
                            bg: RemColor.easyBg
                        )
                    }
                }
            }
            .padding(.horizontal, 14)
            .padding(.top, 12)
            .padding(.bottom, 16)
        }
        .background(RemColor.bg)
    }

    private func gradeButton(
        _ grade: GradeDto,
        title: String,
        interval: String,
        color: Color,
        bg: Color,
        emphasized: Bool = false
    ) -> some View {
        Button {
            Task { await submitGrade(grade) }
        } label: {
            VStack(spacing: 3) {
                Text(title)
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(color)
                    .lineLimit(1)
                    .minimumScaleFactor(0.85)
                Text(interval)
                    .font(.caption2.weight(.medium).monospaced())
                    .foregroundStyle(RemColor.muted)
                    .lineLimit(1)
                    .minimumScaleFactor(0.8)
            }
            .padding(.horizontal, 4)
        }
        .buttonStyle(RemGradeButtonStyle(tint: color, tintBg: bg, emphasized: emphasized))
        .disabled(isMutating)
    }

    private func sessionCompleteView(viewState: StudyViewDto) -> some View {
        if viewState.reviewed == 0 {
            let emptyTitle = customStudy != nil ? "No matching cards" : "Nothing due"
            let emptyDescription: String = {
                if customStudy != nil {
                    return "No cards match this custom study filter right now."
                }
                return deckId != nil
                    ? "Nothing due in this deck right now."
                    : "Nothing due across your decks right now."
            }()
            return AnyView(
                ContentUnavailableView {
                    Label(emptyTitle, systemImage: "checkmark.circle")
                } description: {
                    Text(emptyDescription)
                } actions: {
                    Button("Back to Today") {
                        dismiss()
                    }
                    .buttonStyle(.borderedProminent)
                    .tint(RemColor.accent)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            )
        }

        let recallPct = Int((Double(sessionPassedCount) / Double(viewState.reviewed) * 100.0).rounded())
        let elapsedSeconds = max(1, Int(((sessionFinishedAt ?? Date()).timeIntervalSince(sessionStartedAt)).rounded()))
        let syncNote: String = {
            if appState.syncStatus?.signedIn != true {
                return "Saved locally on this iPhone."
            }
            if appState.lastSyncError != nil {
                return "Saved locally · will sync when online."
            }
            return "Ready to sync with rem Cloud."
        }()

        return AnyView(
            VStack(spacing: 18) {
                ZStack {
                    RoundedRectangle(cornerRadius: 18, style: .continuous)
                        .fill(RemColor.goodBg)
                        .frame(width: 60, height: 60)
                    Image(systemName: "checkmark")
                        .font(.title2.weight(.bold))
                        .foregroundStyle(RemColor.good)
                        .symbolEffect(.bounce, value: completeBounceTrigger)
                }

                VStack(spacing: 6) {
                    Text(viewState.preview ? "Preview complete." : "All caught up.")
                        .font(.title.bold())
                        .tracking(-0.5)
                        .foregroundStyle(RemColor.text)
                    Text(viewState.preview ? "No schedules were changed." : syncNote)
                        .font(.subheadline)
                        .foregroundStyle(RemColor.muted)
                        .multilineTextAlignment(.center)
                }

                HStack(spacing: 0) {
                    sessionStatColumn(
                        value: "\(viewState.reviewed)",
                        label: viewState.preview ? "Previewed" : "Reviewed"
                    )
                    if !viewState.preview {
                        Divider().frame(height: 32).overlay(RemColor.border)
                        sessionStatColumn(value: "\(recallPct)%", label: "Recall")
                    }
                    Divider().frame(height: 32).overlay(RemColor.border)
                    sessionStatColumn(value: Self.formatDuration(seconds: elapsedSeconds), label: "Time")
                }
                .padding(.vertical, 14)
                .background(RemColor.surface)
                .clipShape(RoundedRectangle(cornerRadius: RemRadius.lg, style: .continuous))
                .overlay(
                    RoundedRectangle(cornerRadius: RemRadius.lg, style: .continuous)
                        .strokeBorder(RemColor.border, lineWidth: 1)
                )
                .padding(.top, 4)

                Button {
                    dismiss()
                } label: {
                    Text("Back to Today")
                        .fontWeight(.semibold)
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(.borderedProminent)
                .controlSize(.large)
                .tint(RemColor.accent)
                .frame(maxWidth: 240)
                .padding(.top, 6)
            }
            .padding(28)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .onAppear {
                completeBounceTrigger += 1
            }
        )
    }

    private func sessionStatColumn(value: String, label: String) -> some View {
        VStack(spacing: 3) {
            Text(value)
                .font(.title3.weight(.bold).monospacedDigit())
                .foregroundStyle(RemColor.text)
            Text(label)
                .font(.caption)
                .foregroundStyle(RemColor.muted)
        }
        .frame(maxWidth: .infinity)
    }

    static func formatDuration(seconds: Int) -> String {
        let clamped = max(1, seconds)
        if clamped < 60 { return "\(clamped)s" }
        let minutes = clamped / 60
        let remainder = clamped % 60
        return remainder == 0 ? "\(minutes)m" : "\(minutes)m \(remainder)s"
    }

    // MARK: - Actions

    private func startSession() async {
        do {
            let started: RemStudySession
            if let customStudy, let deckId {
                started = try await appState.store.startCustomStudy(
                    deckId: deckId,
                    mode: customStudy.mode,
                    amount: customStudy.amount
                )
            } else {
                started = try await appState.store.startStudy(deckId: deckId)
            }
            let initial = try await appState.store.studyCurrentView(session: started)
            self.session = started
            self.viewState = initial
            self.sessionPassedCount = 0
            self.lastGradedPassed = nil
            self.sessionStartedAt = Date()
            self.sessionFinishedAt = initial.current == nil ? Date() : nil
        } catch {
            self.loadError = error.remDisplayMessage
        }
    }

    private func advancePreview() async {
        guard let session, !isMutating else { return }
        isMutating = true
        defer { isMutating = false }

        if appState.hapticsEnabled {
            UIImpactFeedbackGenerator(style: .light).impactOccurred()
        }
        do {
            let next = try await appState.store.studyAdvancePreview(session: session)
            if next.current == nil {
                sessionFinishedAt = Date()
            }
            withAnimation(.easeOut(duration: 0.16)) {
                self.viewState = next
            }
        } catch {
            appState.showToast(error.remDisplayMessage)
        }
    }

    private func reveal() async {
        guard let session, !isMutating else { return }
        isMutating = true
        defer { isMutating = false }

        if appState.hapticsEnabled {
            UIImpactFeedbackGenerator(style: .light).impactOccurred()
        }
        do {
            let next = try await appState.store.studyReveal(session: session)
            withAnimation(.spring(response: 0.30, dampingFraction: 0.88)) {
                self.viewState = next
            }
        } catch {
            appState.showToast(error.remDisplayMessage)
        }
    }

    private func submitGrade(_ grade: GradeDto) async {
        guard let session, !isMutating else { return }
        isMutating = true
        defer { isMutating = false }

        if appState.hapticsEnabled {
            UIImpactFeedbackGenerator(style: grade == .again ? .rigid : .medium).impactOccurred()
        }
        do {
            let outcome = try await appState.store.studyGrade(session: session, grade: grade)
            if outcome.conflictCardId != nil {
                appState.showToast("Skipped card edited elsewhere")
            } else {
                let passed = grade != .again
                lastGradedPassed = passed
                if passed {
                    sessionPassedCount += 1
                }
            }
            if outcome.view.current == nil {
                sessionFinishedAt = Date()
                if appState.hapticsEnabled && outcome.view.reviewed > 0 {
                    UINotificationFeedbackGenerator().notificationOccurred(.success)
                }
            }
            withAnimation(.easeOut(duration: 0.16)) {
                self.viewState = outcome.view
            }
        } catch {
            appState.showToast(error.remDisplayMessage)
        }
    }

    private func undoLastGrade() async {
        guard let session, !isMutating else { return }
        isMutating = true
        defer { isMutating = false }

        if appState.hapticsEnabled {
            UIImpactFeedbackGenerator(style: .soft).impactOccurred()
        }
        do {
            let restored = try await appState.store.studyUndoLastGrade(session: session)
            if lastGradedPassed == true && sessionPassedCount > 0 {
                sessionPassedCount -= 1
            }
            lastGradedPassed = nil
            if sessionPassedCount > restored.reviewed {
                sessionPassedCount = restored.reviewed
            }
            sessionFinishedAt = nil
            withAnimation(.easeOut(duration: 0.18)) {
                self.viewState = restored
            }
            appState.showToast("Restored previous card")
        } catch {
            lastGradedPassed = nil
            if let refreshed = try? await appState.store.studyCurrentView(session: session) {
                self.viewState = refreshed
            }
            appState.showToast(error.remDisplayMessage)
        }
    }

    private func suspendCurrentCard() async {
        guard let session, !isMutating else { return }
        isMutating = true
        defer { isMutating = false }

        do {
            let next = try await appState.store.studySuspendCurrent(session: session)
            lastGradedPassed = nil
            if next.current == nil {
                sessionFinishedAt = Date()
            }
            withAnimation(.easeOut(duration: 0.16)) {
                self.viewState = next
            }
            appState.showToast("Card suspended")
        } catch {
            if let reloaded = try? await appState.store.studyReloadCurrent(session: session) {
                if reloaded.current == nil {
                    sessionFinishedAt = Date()
                }
                self.viewState = reloaded
            }
            appState.showToast(error.remDisplayMessage)
        }
    }

    private func reloadCurrentCard() async {
        guard let session, !isMutating else { return }
        isMutating = true
        defer { isMutating = false }

        do {
            let reloaded = try await appState.store.studyReloadCurrent(session: session)
            if reloaded.current == nil {
                sessionFinishedAt = Date()
            }
            self.viewState = reloaded
        } catch {
            appState.showToast(error.remDisplayMessage)
        }
    }
}

extension CardDto: Identifiable {}
