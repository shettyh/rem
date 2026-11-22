import SwiftUI

struct FirstRunView: View {
    @Bindable var appState: AppState

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Spacer().frame(height: 36)

            RemLogoBadge(size: 48)
                .padding(.bottom, 22)

            Text("Welcome to rem.")
                .font(.system(size: 32, weight: .bold))
                .tracking(-0.8)
                .foregroundStyle(RemColor.text)
                .padding(.bottom, 10)

            Text("Calm, adaptive spaced repetition. Study offline anytime—sync across your devices whenever you want.")
                .font(.system(size: 15))
                .lineSpacing(3)
                .foregroundStyle(RemColor.muted)
                .padding(.bottom, 24)

            VStack(spacing: 0) {
                featureRow(
                    title: "Private & offline by default",
                    subtitle: "Your cards live on this iPhone. No account required."
                )
                Divider().overlay(RemColor.border)
                featureRow(
                    title: "Optional cloud sync",
                    subtitle: "Keep decks, progress, and images in sync across Mac and iPhone."
                )
            }
            .background(RemColor.surface)
            .clipShape(RoundedRectangle(cornerRadius: RemRadius.lg, style: .continuous))
            .overlay(
                RoundedRectangle(cornerRadius: RemRadius.lg, style: .continuous)
                    .strokeBorder(RemColor.border, lineWidth: 1)
            )

            Spacer()

            VStack(spacing: 10) {
                Button("Start locally") {
                    appState.hasDismissedOnboarding = true
                    appState.selectedTab = .today
                    appState.showNewDeckSheet = true
                }
                .buttonStyle(RemPrimaryButtonStyle())

                Button("Sign in to rem Premium") {
                    appState.hasDismissedOnboarding = true
                    appState.selectedTab = .settings
                }
                .buttonStyle(RemSecondaryButtonStyle())
            }
            .padding(.bottom, 20)
        }
        .padding(.horizontal, 22)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(RemColor.bg)
    }

    private func featureRow(title: String, subtitle: String) -> some View {
        VStack(alignment: .leading, spacing: 3) {
            Text(title)
                .font(.system(size: 14, weight: .semibold))
                .foregroundStyle(RemColor.text)
            Text(subtitle)
                .font(.system(size: 12.5))
                .foregroundStyle(RemColor.muted)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, 15)
        .padding(.vertical, 13)
    }
}
