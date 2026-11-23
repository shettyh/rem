import Charts
import SwiftUI

struct StatsView: View {
    private struct ActivityPoint: Identifiable {
        let id: String
        let date: Date
        let count: Int
    }

    private static let isoDayParser: DateFormatter = {
        let formatter = DateFormatter()
        formatter.calendar = Calendar.current
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.dateFormat = "yyyy-MM-dd"
        return formatter
    }()

    private static let shortDayFormatter: DateFormatter = {
        let formatter = DateFormatter()
        formatter.dateFormat = "MMM d"
        return formatter
    }()

    @Bindable var appState: AppState
    @State private var rangeDays: UInt32 = 30
    @State private var selectedDeckId: String? = nil
    @State private var stats: StatsDto?
    @State private var selectedActivityDate: Date? = nil

    private var selectedDeckName: String {
        guard let selectedDeckId,
              let deck = appState.decks.first(where: { $0.id == selectedDeckId })
        else { return "All decks" }
        return deck.name
    }

    private var activityPoints: [ActivityPoint] {
        guard let stats else { return [] }
        return stats.dailyActivity.compactMap { entry in
            guard let date = Self.isoDayParser.date(from: entry.day) else { return nil }
            return ActivityPoint(id: entry.day, date: date, count: Int(entry.count))
        }
    }

    private var selectedActivityPoint: ActivityPoint? {
        guard let selectedActivityDate else { return nil }
        return activityPoints.first {
            Calendar.current.isDate($0.date, inSameDayAs: selectedActivityDate)
        }
    }

    private var xAxisStrideDays: Int {
        switch rangeDays {
        case ...7: return 2
        case ...30: return 7
        default: return 28
        }
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
                        let points = activityPoints
                        let maxCount = points.map(\.count).max() ?? 0
                        let yUpperBound = max(4, maxCount)
                        let selected = selectedActivityPoint

                        VStack(alignment: .leading, spacing: 12) {
                            HStack(alignment: .top) {
                                VStack(alignment: .leading, spacing: 2) {
                                    Text("Review activity")
                                        .font(.subheadline.weight(.semibold))
                                        .foregroundStyle(RemColor.text)
                                    if let selected {
                                        Text("\(Self.shortDayFormatter.string(from: selected.date)) · \(selected.count) \(selected.count == 1 ? "review" : "reviews")")
                                            .font(.caption.weight(.semibold))
                                            .foregroundStyle(RemColor.accent)
                                    } else if stats.reviews == 0 {
                                        Text("No reviews in the last \(rangeDays) days")
                                            .font(.caption)
                                            .foregroundStyle(RemColor.muted)
                                    } else {
                                        Text("Touch and drag to inspect daily reviews")
                                            .font(.caption)
                                            .foregroundStyle(RemColor.muted)
                                    }
                                }
                                Spacer()
                                Text("last \(rangeDays) days")
                                    .font(.caption)
                                    .foregroundStyle(RemColor.muted)
                            }

                            Chart {
                                if let selected {
                                    RuleMark(x: .value("Selected", selected.date, unit: .day))
                                        .foregroundStyle(RemColor.accent.opacity(0.25))
                                        .lineStyle(StrokeStyle(lineWidth: 1, dash: [3, 3]))
                                }

                                ForEach(points) { point in
                                    BarMark(
                                        x: .value("Day", point.date, unit: .day),
                                        y: .value("Reviews", point.count)
                                    )
                                    .foregroundStyle(
                                        selected == nil || selected?.id == point.id
                                            ? RemColor.accent
                                            : RemColor.accent.opacity(0.32)
                                    )
                                    .cornerRadius(3)
                                }
                            }
                            .chartXSelection(value: $selectedActivityDate)
                            .chartYScale(domain: 0...yUpperBound)
                            .chartXAxis {
                                AxisMarks(values: .stride(by: .day, count: xAxisStrideDays)) { _ in
                                    AxisGridLine(stroke: StrokeStyle(lineWidth: 0.5))
                                        .foregroundStyle(RemColor.border.opacity(0.5))
                                    AxisValueLabel(format: .dateTime.month(.abbreviated).day(), centered: false)
                                        .font(.caption2)
                                        .foregroundStyle(RemColor.faint)
                                }
                            }
                            .chartYAxis {
                                AxisMarks(position: .leading, values: .automatic(desiredCount: 3)) {
                                    AxisGridLine(stroke: StrokeStyle(lineWidth: 0.5))
                                        .foregroundStyle(RemColor.border)
                                    AxisValueLabel()
                                        .font(.caption2)
                                        .foregroundStyle(RemColor.faint)
                                }
                            }
                            .frame(height: 136)
                        }
                    }

                    RemCardSurface(padding: 16) {
                        VStack(alignment: .leading, spacing: 10) {
                            Text("Grade distribution")
                                .font(.subheadline.weight(.semibold))
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
                            VStack(alignment: .leading, spacing: 10) {
                                Text("By deck")
                                    .font(.subheadline.weight(.semibold))
                                    .foregroundStyle(RemColor.text)

                                ForEach(stats.byDeck, id: \.deckId) { item in
                                    Button {
                                        withAnimation(.easeOut(duration: 0.16)) {
                                            selectedDeckId = item.deckId
                                        }
                                    } label: {
                                        HStack(spacing: 8) {
                                            Circle()
                                                .fill(Color(hexString: item.deckColor))
                                                .frame(width: 7, height: 7)
                                            Text(item.deckName)
                                                .font(.subheadline.weight(.medium))
                                                .foregroundStyle(RemColor.text)
                                                .lineLimit(1)
                                            Spacer()
                                            Text("\(item.reviews)")
                                                .font(.footnote.weight(.semibold).monospacedDigit())
                                                .foregroundStyle(RemColor.text)
                                            Text(item.recallRatePercent.map { "\($0)%" } ?? "—")
                                                .font(.footnote.monospacedDigit())
                                                .foregroundStyle(RemColor.muted)
                                                .frame(width: 40, alignment: .trailing)
                                            Image(systemName: "chevron.right")
                                                .font(.caption2.weight(.semibold))
                                                .foregroundStyle(RemColor.faint)
                                        }
                                        .padding(.vertical, 3)
                                        .contentShape(Rectangle())
                                    }
                                    .buttonStyle(.plain)
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
            .background(RemColor.groupedBg)
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
                    .font(.title2.weight(.bold).monospacedDigit())
                    .tracking(-0.5)
                    .foregroundStyle(RemColor.text)
                Text(label)
                    .font(.caption)
                    .foregroundStyle(RemColor.muted)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private func gradeRow(_ label: String, count: UInt32, total: UInt32, color: Color) -> some View {
        let pct = Int((Double(count) / Double(total) * 100.0).rounded())
        return HStack(spacing: 10) {
            Text(label)
                .font(.footnote)
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
                .font(.caption.monospacedDigit())
                .foregroundStyle(RemColor.muted)
                .frame(width: 38, alignment: .trailing)
        }
    }

    private func loadStats() async {
        selectedActivityDate = nil
        do {
            self.stats = try await appState.store.getStats(deckId: selectedDeckId, days: rangeDays)
        } catch {
            appState.showToast(error.localizedDescription)
        }
    }
}
