import SwiftUI

/// 4 tabs (`Today`, `Decks`, `Stats`, `Settings`) + dedicated bottom center `+` Quick Capture button.
struct RootTabView: View {
    @Bindable var appState: AppState

    var body: some View {
        ZStack(alignment: .bottom) {
            Group {
                switch appState.selectedTab {
                case .today:
                    TodayView(appState: appState)
                case .decks:
                    DecksView(appState: appState)
                case .stats:
                    StatsView(appState: appState)
                case .settings:
                    SettingsView(appState: appState)
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background(RemColor.bg)

            RemBottomTabBar(
                selectedTab: $appState.selectedTab,
                onTapCapture: {
                    appState.openQuickCapture()
                }
            )
        }
        .ignoresSafeArea(.keyboard, edges: .bottom)
    }
}

struct RemBottomTabBar: View {
    @Binding var selectedTab: RemTab
    let onTapCapture: () -> Void

    var body: some View {
        VStack(spacing: 0) {
            Rectangle()
                .fill(RemColor.border)
                .frame(height: 1)

            HStack(spacing: 0) {
                tabButton(.today, title: "Today", systemImage: "calendar")
                tabButton(.decks, title: "Decks", systemImage: "rectangle.stack")

                Button(action: onTapCapture) {
                    ZStack {
                        RoundedRectangle(cornerRadius: 12, style: .continuous)
                            .fill(RemColor.text)
                            .frame(width: 40, height: 40)
                            .shadow(color: Color.black.opacity(0.14), radius: 8, y: 3)
                        Image(systemName: "plus")
                            .font(.system(size: 17, weight: .bold))
                            .foregroundStyle(RemColor.bg)
                    }
                    .frame(maxWidth: .infinity)
                    .contentShape(Rectangle())
                }
                .accessibilityLabel("New card")

                tabButton(.stats, title: "Stats", systemImage: "chart.bar")
                tabButton(.settings, title: "Settings", systemImage: "gearshape")
            }
            .padding(.horizontal, 8)
            .padding(.top, 8)
            .padding(.bottom, 6)
        }
        .background(.ultraThinMaterial)
        .background(RemColor.surface.opacity(0.72))
    }

    private func tabButton(_ tab: RemTab, title: String, systemImage: String) -> some View {
        let isSelected = selectedTab == tab
        return Button {
            selectedTab = tab
        } label: {
            VStack(spacing: 3) {
                Image(systemName: systemImage)
                    .font(.system(size: 19, weight: isSelected ? .semibold : .regular))
                Text(title)
                    .font(.system(size: 10.5, weight: isSelected ? .semibold : .medium))
            }
            .foregroundStyle(isSelected ? RemColor.accent : RemColor.muted)
            .frame(maxWidth: .infinity)
            .padding(.vertical, 4)
            .contentShape(Rectangle())
        }
    }
}
