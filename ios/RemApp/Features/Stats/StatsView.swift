import SwiftUI

struct StatsView: View {
    @Bindable var appState: AppState
    @State private var rangeDays: UInt32 = 30
    @State private var selectedDeckId: String? = nil
    @State private var stats: StatsDto?

    private var selectedDeckName: String {
        guard let selectedDeckId,
              let deck = appState.decks.first(where: { $0.id == selectedDeckId })
        else { return "All decks" }
        return deck.name
    }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    HStack {
                        if !appState.decks.isEmpty {
                            Menu {
                                Button {
                                    selectedDeckId = nil
                                } label: {
                                    Label("All decks", systemImage: selectedDeckId == nil ? "checkmark" : "")
                                }
                                Divider()
                                ForEach(appState.decks, id: \.id) { deck in
                                    Button {
                                        selectedDeckId = deck.id
                                    } label: {
                                        Label(deck.name, systemImage: selectedDeckId == deck.id ? "checkmark" : "")
                                    }
                                }
                            } label: {
                                HStack(spacing: 6) {
                                    Text(selectedDeckName)
                                        .font(.system(size: 13, weight: .semibold))
                                        .foregroundStyle(RemColor.text)
                                        .lineLimit(1)
                                    Image(systemName: "chevron.down")
                                        .font(.system(size: 10, weight: .semibold))
                                        .foregroundStyle(RemColor.muted)
                                }
                                .padding(.horizontal, 12)
                                .padding(.vertical, 7)
                                .background(RemColor.surface)
                                .clipShape(RoundedRectangle(cornerRadius: 9, style: .continuous))
                                .overlay(
                                    RoundedRectangle(cornerRadius: 9, style: .continuous)
                                        .strokeBorder(RemColor.border, lineWidth: 1)
                                )
                            }
                        }

                        Spacer()

                        Picker("Range", selection: $rangeDays) {
                            Text("7D").tag(UInt32(7))
                            Text("30D").tag(UInt32(30))
                            Text("90D").tag(UInt32(90))
                        }
                        .pickerStyle(.segmented)
                        .frame(width: 162)
                    }

                    if let stats {
                    LazyVGrid(columns: [GridItem(.flexible(), spacing: 10), GridItem(.flexible(), spacing: 10)], spacing: 10) {
                        kpiCard(value: "\(stats.reviews)", label: "Reviews")
                        kpiCard(value: stats.recallRatePercent.map { "\($0)%" } ?? "—", label: "Recall rate")
                        kpiCard(value: "\(stats.currentStreakDays) \(stats.currentStreakDays == 1 ? "day" : "days")", label: "Current streak")
                        kpiCard(value: "\(stats.activeDays)", label: "Active days")
                    }

                    RemCardSurface(padding: 16) {
                        VStack(alignment: .leading, spacing: 12) {
                            HStack {
                                VStack(alignment: .leading, spacing: 2) {
                                    Text("Review activity")
                                        .font(.system(size: 14, weight: .semibold))
                                        .foregroundStyle(RemColor.text)
                                    Text("Reviews completed per local day")
                                        .font(.system(size: 11.5))
                                        .foregroundStyle(RemColor.muted)
                                }
                                Spacer()
                                Text("last \(rangeDays) days")
                                    .font(.system(size: 12))
                                    .foregroundStyle(RemColor.muted)
                            }

                            let maxCount = max(1, stats.dailyActivity.map(\.count).max() ?? 1)
                            HStack(alignment: .bottom, spacing: rangeDays > 45 ? 1.5 : 3) {
                                ForEach(stats.dailyActivity, id: \.day) { entry in
                                    let ratio = CGFloat(entry.count) / CGFloat(maxCount)
                                    RoundedRectangle(cornerRadius: 2.5, style: .continuous)
                                        .fill(entry.count == 0 ? RemColor.border : RemColor.accent)
                                        .frame(height: entry.count == 0 ? 4 : max(8, 86 * ratio))
                                }
                            }
                            .frame(height: 88, alignment: .bottom)

                            if let first = stats.dailyActivity.first?.day, let last = stats.dailyActivity.last?.day {
                                HStack {
                                    Text(shortDayLabel(first))
                                    Spacer()
                                    Text(shortDayLabel(last))
                                }
                                .font(.system(size: 11))
                                .foregroundStyle(RemColor.faint)
                            }
                        }
                    }

                    RemCardSurface(padding: 16) {
                        VStack(alignment: .leading, spacing: 10) {
                            Text("Grade distribution")
                                .font(.system(size: 14, weight: .semibold))
                                .foregroundStyle(RemColor.text)

                            let total = max(1, stats.reviews)
                            gradeRow("Again", count: stats.againCount, total: total, color: RemColor.again)
                            gradeRow("Hard", count: stats.hardCount, total: total, color: RemColor.hard)
                            gradeRow("Good", count: stats.goodCount, total: total, color: RemColor.good)
                            gradeRow("Easy", count: stats.easyCount, total: total, color: RemColor.easy)
                        }
                    }

                    if selectedDeckId == nil && !stats.byDeck.isEmpty {
                        RemCardSurface(padding: 16) {
                            VStack(alignment: .leading, spacing: 12) {
                                Text("By deck")
                                    .font(.system(size: 14, weight: .semibold))
                                    .foregroundStyle(RemColor.text)

                                ForEach(stats.byDeck, id: \.deckId) { item in
                                    HStack(spacing: 8) {
                                        Circle()
                                            .fill(Color(hexString: item.deckColor))
                                            .frame(width: 7, height: 7)
                                        Text(item.deckName)
                                            .font(.system(size: 13.5, weight: .medium))
                                            .foregroundStyle(RemColor.text)
                                            .lineLimit(1)
                                        Spacer()
                                        Text("\(item.reviews)")
                                            .font(.system(size: 13, weight: .semibold, design: .monospaced))
                                            .foregroundStyle(RemColor.text)
                                        Text(item.recallRatePercent.map { "\($0)%" } ?? "—")
                                            .font(.system(size: 12.5, design: .monospaced))
                                            .foregroundStyle(RemColor.muted)
                                            .frame(width: 40, alignment: .trailing)
                                    }
                                }
                            }
                        }
                    }
                } else {
                    ProgressView()
                        .frame(maxWidth: .infinity, minHeight: 160)
                }
                }
                .padding(.horizontal, 18)
                .padding(.top, 8)
                .padding(.bottom, 24)
            }
            .background(RemColor.bg)
            .navigationTitle("Stats")
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
            .task(id: "\(rangeDays)-\(selectedDeckId ?? "all")") { await loadStats() }
        }
    }

    private func kpiCard(value: String, label: String) -> some View {
        RemCardSurface(padding: 14) {
            VStack(alignment: .leading, spacing: 3) {
                Text(value)
                    .font(.system(size: 24, weight: .bold))
                    .tracking(-0.6)
                    .foregroundStyle(RemColor.text)
                Text(label)
                    .font(.system(size: 12))
                    .foregroundStyle(RemColor.muted)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private func gradeRow(_ label: String, count: UInt32, total: UInt32, color: Color) -> some View {
        let pct = Int((Double(count) / Double(total) * 100.0).rounded())
        return HStack(spacing: 10) {
            Text(label)
                .font(.system(size: 12.5))
                .foregroundStyle(RemColor.text)
                .frame(width: 44, alignment: .leading)
            GeometryReader { geo in
                ZStack(alignment: .leading) {
                    Capsule().fill(RemColor.bgInset)
                    Capsule()
                        .fill(color)
                        .frame(width: geo.size.width * CGFloat(count) / CGFloat(total))
                }
            }
            .frame(height: 6)
            Text("\(pct)%")
                .font(.system(size: 12, design: .monospaced))
                .foregroundStyle(RemColor.muted)
                .frame(width: 38, alignment: .trailing)
        }
    }

    private func shortDayLabel(_ iso: String) -> String {
        let parser = DateFormatter()
        parser.dateFormat = "yyyy-MM-dd"
        guard let date = parser.date(from: iso) else { return iso }
        let out = DateFormatter()
        out.dateFormat = "MMM d"
        return out.string(from: date)
    }

    private func loadStats() async {
        do {
            self.stats = try await appState.store.getStats(deckId: selectedDeckId, days: rangeDays)
        } catch {
            appState.showToast(error.localizedDescription)
        }
    }
}
