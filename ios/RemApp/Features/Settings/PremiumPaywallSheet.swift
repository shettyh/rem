import SwiftUI

/// Calm, native Quiet Paper StoreKit 2 paywall sheet for rem Premium ($4.99/year).
struct PremiumPaywallSheet: View {
    @Bindable var appState: AppState
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 18) {
                    // Header
                    HStack(spacing: 12) {
                        RemLogoBadge(size: 36)
                        VStack(alignment: .leading, spacing: 2) {
                            Text("rem Premium")
                                .font(.system(size: 18, weight: .bold))
                                .tracking(-0.3)
                                .foregroundStyle(RemColor.text)
                            Text("Managed Cloud Sync across all your devices")
                                .font(.system(size: 13))
                                .foregroundStyle(RemColor.muted)
                        }
                    }

                    // Benefits list
                    VStack(alignment: .leading, spacing: 10) {
                        benefitRow("Sync Mac, Linux, Windows, CLI & iPhone")
                        benefitRow("Isolated per-user sync & 500 MB image storage")
                        benefitRow("All local features stay free · Cancel anytime")
                    }
                    .padding(.vertical, 2)

                    // Annual plan card
                    HStack {
                        VStack(alignment: .leading, spacing: 3) {
                            Text("rem Premium Annual")
                                .font(.system(size: 14.5, weight: .semibold))
                                .foregroundStyle(RemColor.text)
                            Text("Auto-renews yearly · Cancel anytime")
                                .font(.system(size: 12))
                                .foregroundStyle(RemColor.muted)
                        }
                        Spacer()
                        if let price = appState.storeKit.localizedPrice {
                            Text("\(price)/yr")
                                .font(.system(size: 16.5, weight: .bold))
                                .foregroundStyle(RemColor.accent)
                        } else if appState.storeKit.isLoadingProduct {
                            ProgressView()
                                .controlSize(.small)
                        } else {
                            Text("Unavailable")
                                .font(.system(size: 13, weight: .semibold))
                                .foregroundStyle(RemColor.muted)
                        }
                    }
                    .padding(14)
                    .background(RemColor.accentTint)
                    .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
                    .overlay(
                        RoundedRectangle(cornerRadius: 12, style: .continuous)
                            .strokeBorder(RemColor.accent, lineWidth: 1.5)
                    )

                    // Subscribe CTA
                    Button {
                        Task {
                            if appState.storeKit.product == nil {
                                await appState.storeKit.loadProducts()
                                guard appState.storeKit.product != nil else { return }
                            }
                            let purchased = await appState.purchasePremiumViaApple()
                            if purchased {
                                dismiss()
                            }
                        }
                    } label: {
                        HStack(spacing: 8) {
                            if appState.storeKit.isPurchasing || appState.storeKit.isLoadingProduct {
                                ProgressView()
                                    .controlSize(.small)
                                    .tint(RemColor.onAccent)
                            }
                            Text(subscribeButtonTitle)
                        }
                    }
                    .buttonStyle(RemPrimaryButtonStyle())
                    .disabled(
                        appState.storeKit.isPurchasing
                            || appState.storeKit.isRestoring
                            || appState.storeKit.isLoadingProduct
                    )

                    if let err = appState.storeKit.lastError, !err.isEmpty {
                        Text(err)
                            .font(.system(size: 12))
                            .foregroundStyle(RemColor.again)
                            .fixedSize(horizontal: false, vertical: true)
                    }

                    // Auto-renewal disclosure (App Store Guideline 3.1.2(c))
                    Text("Payment is charged to your Apple Account at confirmation of purchase. Subscription automatically renews annually unless canceled at least 24 hours before the end of the current period. Manage or cancel anytime in App Store Account Settings.")
                        .font(.system(size: 11.5))
                        .foregroundStyle(RemColor.faint)
                        .lineSpacing(2)

                    // Restore / Privacy / Terms links
                    HStack(spacing: 12) {
                        Spacer()
                        Button {
                            Task {
                                let restored = await appState.restoreApplePurchases()
                                if restored {
                                    dismiss()
                                }
                            }
                        } label: {
                            if appState.storeKit.isRestoring {
                                ProgressView().controlSize(.mini)
                            } else {
                                Text("Restore Purchases")
                            }
                        }
                        .disabled(appState.storeKit.isPurchasing || appState.storeKit.isRestoring)

                        Text("·").foregroundStyle(RemColor.borderStrong)
                        Link("Privacy", destination: RemDocs.privacyPolicyURL)
                        Text("·").foregroundStyle(RemColor.borderStrong)
                        Link("Terms", destination: RemDocs.termsOfUseURL)
                        Spacer()
                    }
                    .font(.system(size: 12, weight: .medium))
                    .foregroundStyle(RemColor.muted)
                    .padding(.top, 2)
                }
                .padding(.horizontal, 20)
                .padding(.top, 8)
                .padding(.bottom, 24)
            }
            .background(RemColor.bg)
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button {
                        dismiss()
                    } label: {
                        Image(systemName: "xmark")
                            .font(.system(size: 13, weight: .semibold))
                            .foregroundStyle(RemColor.muted)
                            .frame(width: 28, height: 28)
                            .background(RemColor.bgInset)
                            .clipShape(Circle())
                    }
                    .accessibilityLabel("Close")
                }
            }
            .task {
                await appState.storeKit.loadProducts()
            }
        }
        .presentationDetents([.medium, .large])
        .presentationDragIndicator(.visible)
    }

    private var subscribeButtonTitle: String {
        if appState.storeKit.isPurchasing {
            return "Completing purchase…"
        }
        if appState.storeKit.isLoadingProduct {
            return "Loading plan…"
        }
        if let price = appState.storeKit.localizedPrice {
            return "Upgrade to Premium · \(price)/year"
        }
        return "Retry loading plan"
    }

    private func benefitRow(_ text: String) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 9) {
            Image(systemName: "checkmark")
                .font(.system(size: 12, weight: .bold))
                .foregroundStyle(RemColor.good)
            Text(text)
                .font(.system(size: 13.5))
                .foregroundStyle(RemColor.text)
        }
    }
}
