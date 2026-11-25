import SwiftUI

struct FirstRunView: View {
    @Bindable var appState: AppState

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                RemLogoBadge(size: 52)
                    .padding(.top, 44)
                    .padding(.bottom, 22)

                Text("Welcome to rem.")
                    .font(.largeTitle.bold())
                    .tracking(-0.6)
                    .foregroundStyle(RemColor.text)
                    .padding(.bottom, 10)

                Text("Calm, adaptive spaced repetition. Study offline anytime—sync across your devices whenever you want.")
                    .font(.body)
                    .foregroundStyle(RemColor.muted)
                    .padding(.bottom, 32)

                VStack(alignment: .leading, spacing: 22) {
                    featureRow(
                        systemImage: "iphone",
                        title: "Private & offline by default",
                        subtitle: "Your cards live on this iPhone. No account required."
                    )
                    featureRow(
                        systemImage: "icloud",
                        title: "Optional cloud sync",
                        subtitle: "Keep decks, progress, and images in sync across Mac and iPhone."
                    )
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 24)
            .padding(.bottom, 24)
        }
        .scrollBounceBehavior(.basedOnSize)
        .safeAreaInset(edge: .bottom) {
            VStack(spacing: 12) {
                Button {
                    appState.hasDismissedOnboarding = true
                    appState.selectedTab = .today
                    appState.showNewDeckSheet = true
                } label: {
                    Text("Start locally")
                        .fontWeight(.semibold)
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(.borderedProminent)
                .controlSize(.large)

                Button {
                    appState.hasDismissedOnboarding = true
                    appState.selectedTab = .settings
                } label: {
                    Text("Sign in to rem Premium")
                        .fontWeight(.semibold)
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(.bordered)
                .controlSize(.large)
            }
            .padding(.horizontal, 24)
            .padding(.top, 12)
            .padding(.bottom, 20)
            .background(RemColor.bg)
        }
        .background(RemColor.bg)
        .tint(RemColor.accent)
    }

    private func featureRow(systemImage: String, title: String, subtitle: String) -> some View {
        HStack(alignment: .top, spacing: 14) {
            Image(systemName: systemImage)
                .font(.title2)
                .foregroundStyle(RemColor.accent)
                .frame(width: 32, alignment: .center)
                .padding(.top, 2)
                .accessibilityHidden(true)

            VStack(alignment: .leading, spacing: 3) {
                Text(title)
                    .font(.headline)
                    .foregroundStyle(RemColor.text)
                Text(subtitle)
                    .font(.subheadline)
                    .foregroundStyle(RemColor.muted)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}
