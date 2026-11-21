import SwiftUI
import UIKit

/// Quiet Paper visual tokens mirrored directly from `src/ui/tokens.css`.
enum RemColor {
    static let bg = dynamic(light: 0xFFFFFF, dark: 0x161413)
    static let bgInset = dynamic(light: 0xF5F4F2, dark: 0x201E1C)
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

struct RemPrimaryButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 15, weight: .semibold))
            .foregroundStyle(RemColor.onAccent)
            .frame(maxWidth: .infinity)
            .padding(.vertical, 13.5)
            .padding(.horizontal, 16)
            .background(configuration.isPressed ? RemColor.accentHover : RemColor.accent)
            .clipShape(RoundedRectangle(cornerRadius: 11, style: .continuous))
            .scaleEffect(configuration.isPressed ? 0.985 : 1.0)
            .animation(.easeOut(duration: 0.11), value: configuration.isPressed)
    }
}

struct RemSecondaryButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 14, weight: .semibold))
            .foregroundStyle(RemColor.text)
            .frame(maxWidth: .infinity)
            .padding(.vertical, 11.5)
            .padding(.horizontal, 14)
            .background(configuration.isPressed ? RemColor.bgInset : RemColor.surface)
            .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
            .overlay(
                RoundedRectangle(cornerRadius: 10, style: .continuous)
                    .strokeBorder(RemColor.borderStrong, lineWidth: 1)
            )
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
