import SwiftUI

struct StatsView: View {
    @Bindable var appState: AppState
    @State private var rangeDays: UInt32 = 30
    @State private var stats: StatsDto?

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 14) {
                HStack {
                    Text("Stats")
                        .font(.system(size: 28, weight: .bold))
                        .tracking(-0.6)
                        .foregroundStyle(RemColor.text)
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
                                Text("Review activity")
                                    .font(.system(size: 14, weight: .semibold))
                                    .foregroundStyle(RemColor.text)
                                Spacer()
                                Text("last \(rangeDays) days")
                                    .font(.system(size: 12))
                                    .foregroundStyle(RemColor.muted)
                            }

                            let maxCount = max(1, stats.dailyActivity.map(\.count).max() ?? 1)
                            HStack(alignment: .bottom, spacing: 3) {
                                ForEach(stats.dailyActivity, id: \.day) { entry in
                                    let ratio = CGFloat(entry.count) / CGFloat(maxCount)
                                    RoundedRectangle(cornerRadius: 2.5, style: .continuous)
                                        .fill(entry.count == 0 ? RemColor.border : RemColor.accent)
                                        .frame(height: entry.count == 0 ? 4 : max(8, 84 * ratio))
                                }
                            }
                            .frame(height: 86, alignment: .bottom)
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
                } else {
                    ProgressView()
                        .frame(maxWidth: .infinity, minHeight: 160)
                }
            }
            .padding(.horizontal, 18)
            .padding(.top, 10)
            .padding(.bottom, 104)
        }
        .background(RemColor.bg)
        .task(id: rangeDays) { await loadStats() }
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

    private func loadStats() async {
        do {
            self.stats = try await appState.store.getStats(deckId: nil, days: rangeDays)
        } catch {
            appState.showToast(error.localizedDescription)
        }
    }
}
