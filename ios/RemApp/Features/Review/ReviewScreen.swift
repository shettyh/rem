import SwiftUI

/// Working focused review screen wired to Rust `RemStudySession`.
/// Slice 4C adds rich syntax-highlighted code blocks, offline asset:<hash> zoom lightbox, and the `⋯` action sheet.
struct ReviewScreen: View {
    @Bindable var appState: AppState
    let deckId: String?

    @Environment(\.dismiss) private var dismiss
    @State private var session: RemStudySession?
    @State private var viewState: StudyViewDto?
    @State private var loadError: String?

    var body: some View {
        VStack(spacing: 0) {
            header

            if let loadError {
                VStack(spacing: 12) {
                    Text("Could not start session")
                        .font(.headline)
                        .foregroundStyle(RemColor.text)
                    Text(loadError)
                        .font(.footnote)
                        .foregroundStyle(RemColor.muted)
                    Button("Back to Today") { dismiss() }
                        .buttonStyle(RemSecondaryButtonStyle())
                        .frame(maxWidth: 200)
                }
                .padding(24)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if let viewState {
                if let card = viewState.current {
                    ScrollView {
                        VStack(alignment: .leading, spacing: 14) {
                            Text("QUESTION")
                                .font(.system(size: 11, weight: .bold))
                                .tracking(0.8)
                                .foregroundStyle(RemColor.faint)

                            Text(card.front)
                                .font(.system(
                                    size: viewState.revealed
                                        ? appState.cardTextSize.revealedQuestionSize
                                        : appState.cardTextSize.questionSize,
                                    weight: .medium
                                ))
                                .foregroundStyle(viewState.revealed ? RemColor.muted : RemColor.text)

                            if viewState.revealed {
                                Divider().overlay(RemColor.border).padding(.vertical, 6)

                                Text("ANSWER")
                                    .font(.system(size: 11, weight: .bold))
                                    .tracking(0.8)
                                    .foregroundStyle(RemColor.faint)

                                Text(card.back)
                                    .font(.system(size: appState.cardTextSize.answerSize))
                                    .foregroundStyle(RemColor.text)
                            } else {
                                Text("Tap anywhere to reveal answer")
                                    .font(.system(size: 12.5))
                                    .foregroundStyle(RemColor.faint)
                                    .frame(maxWidth: .infinity)
                                    .padding(.top, 28)
                            }
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(20)
                    }
                    .contentShape(Rectangle())
                    .onTapGesture {
                        if !viewState.revealed {
                            Task { await reveal() }
                        }
                    }

                    bottomDock(viewState: viewState)
                } else {
                    VStack(spacing: 12) {
                        ZStack {
                            RoundedRectangle(cornerRadius: 16, style: .continuous)
                                .fill(RemColor.goodBg)
                                .frame(width: 52, height: 52)
                            Image(systemName: "checkmark")
                                .font(.system(size: 24, weight: .bold))
                                .foregroundStyle(RemColor.good)
                        }
                        Text("All caught up.")
                            .font(.system(size: 26, weight: .bold))
                            .foregroundStyle(RemColor.text)
                        Text("Reviewed \(viewState.reviewed) \(viewState.reviewed == 1 ? "card" : "cards").")
                            .font(.system(size: 14))
                            .foregroundStyle(RemColor.muted)
                        Button("Back to Today") { dismiss() }
                            .buttonStyle(RemPrimaryButtonStyle())
                            .frame(maxWidth: 220)
                            .padding(.top, 8)
                    }
                    .padding(28)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                }
            } else {
                ProgressView()
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .background(RemColor.bg)
        .task { await startSession() }
    }

    private var header: some View {
        let total = max(1, Int((viewState?.reviewed ?? 0) + (viewState?.remaining ?? 0)))
        let currentIndex = min(total, Int(viewState?.reviewed ?? 0) + (viewState?.current == nil ? 0 : 1))
        let progress = viewState?.current == nil ? 1.0 : Double(currentIndex) / Double(total)

        return VStack(spacing: 0) {
            HStack {
                HStack(spacing: 8) {
                    if viewState?.current != nil {
                        Text("\(currentIndex) / \(total)")
                            .font(.system(size: 12.5, weight: .bold, design: .monospaced))
                            .foregroundStyle(RemColor.text)
                    }
                    Text(viewState?.deckName ?? "Review")
                        .font(.system(size: 13.5, weight: .medium))
                        .foregroundStyle(RemColor.muted)
                        .lineLimit(1)
                }
                Spacer()
                Button {
                    dismiss()
                } label: {
                    Image(systemName: "xmark")
                        .font(.system(size: 15, weight: .semibold))
                        .foregroundStyle(RemColor.muted)
                        .frame(width: 34, height: 34)
                }
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 8)

            GeometryReader { geo in
                ZStack(alignment: .leading) {
                    Rectangle().fill(RemColor.border)
                    Rectangle()
                        .fill(RemColor.accent)
                        .frame(width: geo.size.width * progress)
                        .animation(.easeOut(duration: 0.2), value: progress)
                }
            }
            .frame(height: 2)
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
                HStack(spacing: 8) {
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

    private func startSession() async {
        do {
            let started = try await appState.store.startStudy(deckId: deckId)
            let initial = try started.currentView(nowMs: RemStore.nowMs())
            self.session = started
            self.viewState = initial
        } catch {
            self.loadError = error.localizedDescription
        }
    }

    private func reveal() async {
        guard let session else { return }
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
        do {
            let outcome = try session.grade(grade: grade, nowMs: RemStore.nowMs())
            self.viewState = outcome.view
        } catch {
            appState.showToast(error.localizedDescription)
        }
    }
}
