import SwiftUI

/// Native iOS `TabView` with 4 top-level tabs (`Today`, `Decks`, `Stats`, `Settings`).
/// Quick Capture (`+`) lives in the native navigation bar toolbar.
struct RootTabView: View {
    @Bindable var appState: AppState

    var body: some View {
        TabView(selection: $appState.selectedTab) {
            TodayView(appState: appState)
                .tabItem {
                    Label("Today", systemImage: "calendar")
                }
                .tag(RemTab.today)

            DecksView(appState: appState)
                .tabItem {
                    Label("Decks", systemImage: "rectangle.stack")
                }
                .tag(RemTab.decks)

            StatsView(appState: appState)
                .tabItem {
                    Label("Stats", systemImage: "chart.bar")
                }
                .tag(RemTab.stats)

            SettingsView(appState: appState)
                .tabItem {
                    Label("Settings", systemImage: "gearshape")
                }
                .tag(RemTab.settings)
        }
        .tint(RemColor.accent)
    }
}
