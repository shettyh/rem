import SwiftUI
import UIKit

/// Quiet Paper visual tokens mirrored directly from `src/ui/tokens.css`.
enum RemColor {
    static let bg = dynamic(light: 0xFFFFFF, dark: 0x161413)
    static let bgInset = dynamic(light: 0xF5F4F2, dark: 0x201E1C)
    static let groupedBg = dynamic(light: 0xF5F4F2, dark: 0x0E0C0B)
    static let chrome = dynamic(light: 0xFAFAF9, dark: 0x0E0C0B)
    static let surface = dynamic(light: 0xFFFFFF, dark: 0x1B1918)
    static let surfaceInset = dynamic(light: 0xF1F0EE, dark: 0x24211F)
    static let border = dynamic(light: 0xECEAE7, dark: 0x2B2825)
    static let borderStrong = dynamic(light: 0xD9D6D2, dark: 0x3C3835)
    static let text = dynamic(light: 0x1C1917, dark: 0xFAFAF9)
    static let muted = dynamic(light: 0x6F6965, dark: 0xB0AAA5)
    static let faint = dynamic(light: 0x8C8682, dark: 0x8F8984)

    static let accent = dynamic(light: 0x5A47E8, dark: 0x8174FF)
    static let accentHover = dynamic(light: 0x4A39D6, dark: 0x9288FF)
    static let onAccent = dynamic(light: 0xFFFFFF, dark: 0x161413)
    static let accentTint = dynamic(
        light: UIColor(hex: 0x5A47E8, alpha: 0.10),
        dark: UIColor(hex: 0x8174FF, alpha: 0.16)
    )

    static let codeBg = dynamic(light: 0x1E1E22, dark: 0x0E0C0B)
    static let codeFg = Color(uiColor: UIColor(hex: 0xECEAF6))

    static let again = dynamic(light: 0xDC2A2F, dark: 0xE5484D)
    static let againBg = dynamic(
        light: UIColor(hex: 0xDC2A2F, alpha: 0.10),
        dark: UIColor(hex: 0xE5484D, alpha: 0.16)
    )
    static let hard = dynamic(light: 0xD97914, dark: 0xE8922E)
    static let hardBg = dynamic(
        light: UIColor(hex: 0xD97914, alpha: 0.10),
        dark: UIColor(hex: 0xE8922E, alpha: 0.16)
    )
    static let good = dynamic(light: 0x1E9B5E, dark: 0x2FA86B)
    static let goodBg = dynamic(
        light: UIColor(hex: 0x1E9B5E, alpha: 0.10),
        dark: UIColor(hex: 0x2FA86B, alpha: 0.16)
    )
    static let easy = accent
    static let easyBg = accentTint
    static let danger = again

    static let deckPalette: [String] = [
        "#5a47e8",
        "#1e9b5e",
        "#d97914",
        "#dc2a2f",
        "#3ba0e8",
        "#6f6965"
    ]

    private static func dynamic(light: UInt32, dark: UInt32) -> Color {
        dynamic(light: UIColor(hex: light), dark: UIColor(hex: dark))
    }

    private static func dynamic(light: UIColor, dark: UIColor) -> Color {
        Color(uiColor: UIColor { traits in
            traits.userInterfaceStyle == .dark ? dark : light
        })
    }
}

extension UIColor {
    convenience init(hex: UInt32, alpha: CGFloat = 1.0) {
        let r = CGFloat((hex >> 16) & 0xFF) / 255.0
        let g = CGFloat((hex >> 8) & 0xFF) / 255.0
        let b = CGFloat(hex & 0xFF) / 255.0
        self.init(red: r, green: g, blue: b, alpha: alpha)
    }
}

extension Color {
    init(hexString: String, fallback: Color = RemColor.accent) {
        let cleaned = hexString.trimmingCharacters(in: CharacterSet(charactersIn: "# "))
        guard cleaned.count == 6, let value = UInt32(cleaned, radix: 16) else {
            self = fallback
            return
        }
        self = Color(uiColor: UIColor(hex: value))
    }
}

enum CardTextSize: String, CaseIterable, Identifiable {
    case compact
    case `default`
    case large

    var id: String { rawValue }

    var shortLabel: String {
        switch self {
        case .compact: return "S"
        case .default: return "M"
        case .large: return "L"
        }
    }

    var menuLabel: String {
        switch self {
        case .compact: return "Small"
        case .default: return "Medium"
        case .large: return "Large"
        }
    }

    var questionSize: CGFloat {
        switch self {
        case .compact: return 19
        case .default: return 22
        case .large: return 26
        }
    }

    var revealedQuestionSize: CGFloat {
        switch self {
        case .compact: return 15
        case .default: return 17
        case .large: return 19
        }
    }

    var answerSize: CGFloat {
        switch self {
        case .compact: return 15
        case .default: return 17
        case .large: return 19
        }
    }

    var codeSize: CGFloat {
        switch self {
        case .compact: return 12
        case .default: return 13
        case .large: return 14.5
        }
    }
}

enum AppAppearance: String, CaseIterable, Identifiable {
    case system
    case light
    case dark

    var id: String { rawValue }

    var label: String {
        switch self {
        case .system: return "System"
        case .light: return "Light"
        case .dark: return "Dark"
        }
    }

    var colorScheme: ColorScheme? {
        switch self {
        case .system: return nil
        case .light: return .light
        case .dark: return .dark
        }
    }
}

enum RemRadius {
    static let sm: CGFloat = 6
    static let md: CGFloat = 10
    static let lg: CGFloat = 14
}

enum RemDocs {
    static let selfHostingGuideURL = URL(string: "https://github.com/shettyh/rem/blob/main/sync-server/README.md")!
    static let privacyPolicyURL = URL(string: "https://rem-sync.shettyh.fyi/privacy")!
    static let termsOfUseURL = URL(string: "https://rem-sync.shettyh.fyi/terms")!
    static let supportURL = URL(string: "https://rem-sync.shettyh.fyi/support")!
}

/// Official 4-color Google "G" mark drawn as crisp vector paths for Sign-In buttons.
struct GoogleLogoMark: View {
    var size: CGFloat = 16

    var body: some View {
        Canvas { context, canvasSize in
            let scale = min(canvasSize.width, canvasSize.height) / 24.0
            let transform = CGAffineTransform(scaleX: scale, y: scale)

            // Blue
            var blue = Path()
            blue.move(to: CGPoint(x: 23.49, y: 12.27))
            blue.addCurve(to: CGPoint(x: 23.29, y: 10.0), control1: CGPoint(x: 23.49, y: 11.48), control2: CGPoint(x: 23.42, y: 10.72))
            blue.addLine(to: CGPoint(x: 12.0, y: 10.0))
            blue.addLine(to: CGPoint(x: 12.0, y: 14.3))
            blue.addLine(to: CGPoint(x: 18.45, y: 14.3))
            blue.addCurve(to: CGPoint(x: 16.06, y: 17.92), control1: CGPoint(x: 18.17, y: 15.8), control2: CGPoint(x: 17.32, y: 17.07))
            blue.addLine(to: CGPoint(x: 16.06, y: 20.92))
            blue.addLine(to: CGPoint(x: 19.93, y: 20.92))
            blue.addCurve(to: CGPoint(x: 23.49, y: 12.27), control1: CGPoint(x: 22.19, y: 18.84), control2: CGPoint(x: 23.49, y: 15.77))
            blue.closeSubpath()
            context.fill(blue.applying(transform), with: .color(Color(hexString: "4285F4")))

            // Green
            var green = Path()
            green.move(to: CGPoint(x: 12.0, y: 24.0))
            green.addCurve(to: CGPoint(x: 19.93, y: 21.09), control1: CGPoint(x: 15.24, y: 24.0), control2: CGPoint(x: 17.95, y: 22.93))
            green.addLine(to: CGPoint(x: 16.06, y: 18.09))
            green.addCurve(to: CGPoint(x: 12.0, y: 19.24), control1: CGPoint(x: 14.99, y: 18.81), control2: CGPoint(x: 13.62, y: 19.24))
            green.addCurve(to: CGPoint(x: 5.28, y: 14.29), control1: CGPoint(x: 8.88, y: 19.24), control2: CGPoint(x: 6.23, y: 17.13))
            green.addLine(to: CGPoint(x: 1.28, y: 14.29))
            green.addLine(to: CGPoint(x: 1.28, y: 17.39))
            green.addCurve(to: CGPoint(x: 12.0, y: 24.0), control1: CGPoint(x: 3.25, y: 21.31), control2: CGPoint(x: 7.31, y: 24.0))
            green.closeSubpath()
            context.fill(green.applying(transform), with: .color(Color(hexString: "34A853")))

            // Yellow
            var yellow = Path()
            yellow.move(to: CGPoint(x: 5.28, y: 14.29))
            yellow.addCurve(to: CGPoint(x: 4.9, y: 12.0), control1: CGPoint(x: 5.04, y: 13.57), control2: CGPoint(x: 4.9, y: 12.8))
            yellow.addCurve(to: CGPoint(x: 5.28, y: 9.71), control1: CGPoint(x: 4.9, y: 11.2), control2: CGPoint(x: 5.04, y: 10.43))
            yellow.addLine(to: CGPoint(x: 5.28, y: 6.61))
            yellow.addLine(to: CGPoint(x: 1.28, y: 6.61))
            yellow.addCurve(to: CGPoint(x: 0.0, y: 12.0), control1: CGPoint(x: 0.46, y: 8.23), control2: CGPoint(x: 0.0, y: 10.06))
            yellow.addCurve(to: CGPoint(x: 1.28, y: 17.39), control1: CGPoint(x: 0.0, y: 13.94), control2: CGPoint(x: 0.46, y: 15.77))
            yellow.addLine(to: CGPoint(x: 5.28, y: 14.29))
            yellow.closeSubpath()
            context.fill(yellow.applying(transform), with: .color(Color(hexString: "FBBC05")))

            // Red
            var red = Path()
            red.move(to: CGPoint(x: 12.0, y: 4.76))
            red.addCurve(to: CGPoint(x: 16.58, y: 6.56), control1: CGPoint(x: 13.76, y: 4.76), control2: CGPoint(x: 15.34, y: 5.37))
            red.addLine(to: CGPoint(x: 20.02, y: 3.12))
            red.addCurve(to: CGPoint(x: 12.0, y: 0.0), control1: CGPoint(x: 17.95, y: 1.19), control2: CGPoint(x: 15.24, y: 0.0))
            red.addCurve(to: CGPoint(x: 1.28, y: 6.61), control1: CGPoint(x: 7.31, y: 0.0), control2: CGPoint(x: 3.25, y: 2.69))
            red.addLine(to: CGPoint(x: 5.28, y: 9.71))
            red.addCurve(to: CGPoint(x: 12.0, y: 4.76), control1: CGPoint(x: 6.23, y: 6.87), control2: CGPoint(x: 8.88, y: 4.76))
            red.closeSubpath()
            context.fill(red.applying(transform), with: .color(Color(hexString: "EA4335")))
        }
        .frame(width: size, height: size)
        .accessibilityHidden(true)
    }
}

struct RemLogoBadge: View {
    var size: CGFloat = 28

    var body: some View {
        ZStack {
            RoundedRectangle(cornerRadius: size * 0.27, style: .continuous)
                .fill(RemColor.accent)
            Text("r")
                .font(.system(size: size * 0.68, weight: .regular, design: .serif))
                .foregroundStyle(Color.white)
                .offset(y: -size * 0.03)
        }
        .frame(width: size, height: size)
    }
}

struct RemCardSurface<Content: View>: View {
    var padding: CGFloat = 16
    @ViewBuilder var content: Content

    var body: some View {
        content
            .padding(padding)
            .background(RemColor.surface)
            .clipShape(RoundedRectangle(cornerRadius: RemRadius.lg, style: .continuous))
            .overlay(
                RoundedRectangle(cornerRadius: RemRadius.lg, style: .continuous)
                    .strokeBorder(RemColor.border, lineWidth: 1)
            )
    }
}

struct RemGradeButtonStyle: ButtonStyle {
    let tint: Color
    let tintBg: Color
    var emphasized: Bool = false

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .frame(maxWidth: .infinity)
            .frame(height: 54)
            .background(
                configuration.isPressed
                    ? tintBg
                    : (emphasized ? tintBg.opacity(0.62) : RemColor.surface)
            )
            .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
            .overlay(
                RoundedRectangle(cornerRadius: 12, style: .continuous)
                    .strokeBorder(
                        configuration.isPressed
                            ? tint.opacity(0.55)
                            : (emphasized ? tint.opacity(0.35) : RemColor.border),
                        lineWidth: 1
                    )
            )
            .scaleEffect(configuration.isPressed ? 0.965 : 1.0)
            .animation(.easeOut(duration: 0.11), value: configuration.isPressed)
    }
}

struct RemToastOverlayModifier: ViewModifier {
    @Bindable var appState: AppState
    var enabled: Bool = true

    func body(content: Content) -> some View {
        content
            .overlay(alignment: .top) {
                if enabled, let toast = appState.toastMessage {
                    HStack(spacing: 10) {
                        Text(toast)
                            .font(.footnote.weight(.semibold))
                            .foregroundStyle(RemColor.bg)
                            .lineLimit(2)
                        Spacer(minLength: 4)
                        Button("OK") {
                            appState.toastMessage = nil
                        }
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(RemColor.bg.opacity(0.75))
                    }
                    .padding(.horizontal, 14)
                    .padding(.vertical, 11)
                    .background(RemColor.text)
                    .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
                    .shadow(color: Color.black.opacity(0.18), radius: 14, y: 6)
                    .padding(.horizontal, 16)
                    .padding(.top, 8)
                    .transition(.move(edge: .top).combined(with: .opacity))
                    .onTapGesture {
                        appState.toastMessage = nil
                    }
                }
            }
            .animation(.easeOut(duration: 0.18), value: appState.toastMessage)
    }
}

extension View {
    func remToastOverlay(appState: AppState, enabled: Bool = true) -> some View {
        modifier(RemToastOverlayModifier(appState: appState, enabled: enabled))
    }
}

struct RemPillBadge: View {
    enum Tone {
        case accent
        case good
        case hard
        case again
        case neutral
    }

    let text: String
    let tone: Tone

    private var fg: Color {
        switch tone {
        case .accent: return RemColor.accent
        case .good: return RemColor.good
        case .hard: return RemColor.hard
        case .again: return RemColor.again
        case .neutral: return RemColor.muted
        }
    }

    private var bg: Color {
        switch tone {
        case .accent: return RemColor.accentTint
        case .good: return RemColor.goodBg
        case .hard: return RemColor.hardBg
        case .again: return RemColor.againBg
        case .neutral: return RemColor.bgInset
        }
    }

    var body: some View {
        Text(text)
            .font(.system(size: 11, weight: .semibold))
            .foregroundStyle(fg)
            .padding(.horizontal, 8)
            .padding(.vertical, 3)
            .background(bg)
            .clipShape(Capsule())
    }
}
