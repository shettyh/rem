import AVFoundation
import Foundation
import SwiftUI

// MARK: - Cloze Parser & Renderer

struct ClozeSpan: Equatable {
    let range: Range<String.Index>
    let index: UInt32
    let answer: String
    let hint: String?
}

enum ClozeRenderer {
    static let blankPrefix = "\u{E000}"
    static let blankSuffix = "\u{E001}"
    static let revealPrefix = "\u{E002}"
    static let revealSuffix = "\u{E003}"

    static func parseSpans(_ text: String) -> [ClozeSpan] {
        var spans: [ClozeSpan] = []
        var i = text.startIndex

        while i < text.endIndex {
            let next1 = text.index(after: i)
            guard next1 < text.endIndex else { break }
            let next2 = text.index(after: next1)
            guard next2 < text.endIndex else { break }

            if text[i] == "{" && text[next1] == "{" && (text[next2] == "c" || text[next2] == "C") {
                var numIdx = text.index(after: next2)
                let numStart = numIdx
                while numIdx < text.endIndex && text[numIdx].isASCII && text[numIdx].isNumber {
                    numIdx = text.index(after: numIdx)
                }
                if numIdx > numStart,
                   numIdx < text.endIndex,
                   text[numIdx] == ":"
                {
                    let colon2 = text.index(after: numIdx)
                    if colon2 < text.endIndex && text[colon2] == ":" {
                        if let parsedIndex = UInt32(text[numStart..<numIdx]), parsedIndex >= 1 {
                            let bodyStart = text.index(after: colon2)
                            if let (answer, hint, closeEnd) = scanBody(text, from: bodyStart) {
                                spans.append(ClozeSpan(
                                    range: i..<closeEnd,
                                    index: parsedIndex,
                                    answer: answer,
                                    hint: hint
                                ))
                                i = closeEnd
                                continue
                            }
                        }
                    }
                }
            }
            i = text.index(after: i)
        }
        return spans
    }

    private static func scanBody(
        _ text: String,
        from bodyStart: String.Index
    ) -> (String, String?, String.Index)? {
        var depth = 0
        var hintSep: String.Index? = nil
        var j = bodyStart

        while j < text.endIndex {
            let ch = text[j]
            if ch == "{" {
                depth += 1
                j = text.index(after: j)
                continue
            }
            if ch == "}" {
                if depth == 0 {
                    let next = text.index(after: j)
                    if next < text.endIndex && text[next] == "}" {
                        let closeEnd = text.index(after: next)
                        if let sep = hintSep {
                            let afterSep = text.index(sep, offsetBy: 2)
                            let answer = String(text[bodyStart..<sep])
                            let hintRaw = String(text[afterSep..<j]).trimmingCharacters(in: .whitespacesAndNewlines)
                            return (answer, hintRaw.isEmpty ? nil : hintRaw, closeEnd)
                        }
                        return (String(text[bodyStart..<j]), nil, closeEnd)
                    }
                }
                depth = max(0, depth - 1)
                j = text.index(after: j)
                continue
            }
            if depth == 0 && hintSep == nil && ch == ":" {
                let next = text.index(after: j)
                if next < text.endIndex && text[next] == ":" {
                    hintSep = j
                    j = text.index(after: next)
                    continue
                }
            }
            j = text.index(after: j)
        }
        return nil
    }

    static func extractIndices(_ text: String) -> [UInt32] {
        Array(Set(parseSpans(text).map(\.index))).sorted()
    }

    static func nextIndex(in text: String) -> UInt32 {
        (extractIndices(text).last ?? 0) + 1
    }

    static func stripMarkup(_ text: String) -> String {
        let spans = parseSpans(text)
        guard !spans.isEmpty else { return text }
        var out = ""
        var cursor = text.startIndex
        for span in spans {
            out += text[cursor..<span.range.lowerBound]
            out += span.answer
            cursor = span.range.upperBound
        }
        out += text[cursor...]
        return out
    }

    static func render(_ text: String, activeIndex: UInt32, revealed: Bool) -> String {
        let spans = parseSpans(text)
        guard !spans.isEmpty else { return text }
        var out = ""
        var cursor = text.startIndex
        for span in spans {
            out += text[cursor..<span.range.lowerBound]
            if span.index == activeIndex {
                if revealed {
                    out += "\(revealPrefix)\(span.answer)\(revealSuffix)"
                } else {
                    let label = span.hint.map { "[\($0)]" } ?? "[…]"
                    out += "\(blankPrefix)\(label)\(blankSuffix)"
                }
            } else {
                out += span.answer
            }
            cursor = span.range.upperBound
        }
        out += text[cursor...]
        return out
    }
}

// MARK: - Native LaTeX / Math Formatter & View

enum LatexMathFormatter {
    private static let symbols: [(String, String)] = [
        ("\\Rightarrow", "⇒"), ("\\Leftarrow", "⇐"), ("\\Leftrightarrow", "⇔"),
        ("\\rightarrow", "→"), ("\\leftarrow", "←"), ("\\leftrightarrow", "↔"),
        ("\\mapsto", "↦"), ("\\to", "→"),
        ("\\leq", "≤"), ("\\le", "≤"), ("\\geq", "≥"), ("\\ge", "≥"),
        ("\\neq", "≠"), ("\\ne", "≠"), ("\\approx", "≈"), ("\\equiv", "≡"),
        ("\\sim", "∼"), ("\\propto", "∝"), ("\\pm", "±"), ("\\mp", "∓"),
        ("\\times", "×"), ("\\cdot", "·"), ("\\div", "÷"), ("\\circ", "∘"),
        ("\\infty", "∞"), ("\\partial", "∂"), ("\\nabla", "∇"),
        ("\\sum", "∑"), ("\\prod", "∏"), ("\\int", "∫"), ("\\oint", "∮"),
        ("\\forall", "∀"), ("\\exists", "∃"), ("\\neg", "¬"), ("\\land", "∧"), ("\\lor", "∨"),
        ("\\notin", "∉"), ("\\in", "∈"), ("\\subseteq", "⊆"), ("\\subset", "⊂"),
        ("\\supseteq", "⊇"), ("\\supset", "⊃"), ("\\cup", "∪"), ("\\cap", "∩"),
        ("\\emptyset", "∅"), ("\\varnothing", "∅"), ("\\therefore", "∴"), ("\\because", "∵"),
        ("\\degree", "°"), ("\\angle", "∠"), ("\\perp", "⊥"), ("\\parallel", "∥"),
        ("\\cdots", "⋯"), ("\\ldots", "…"), ("\\dots", "…"),
        ("\\alpha", "α"), ("\\beta", "β"), ("\\gamma", "γ"), ("\\delta", "δ"),
        ("\\epsilon", "ε"), ("\\varepsilon", "ε"), ("\\zeta", "ζ"), ("\\eta", "η"),
        ("\\theta", "θ"), ("\\iota", "ι"), ("\\kappa", "κ"), ("\\lambda", "λ"),
        ("\\mu", "μ"), ("\\nu", "ν"), ("\\xi", "ξ"), ("\\pi", "π"),
        ("\\rho", "ρ"), ("\\sigma", "σ"), ("\\tau", "τ"), ("\\upsilon", "υ"),
        ("\\phi", "φ"), ("\\varphi", "φ"), ("\\chi", "χ"), ("\\psi", "ψ"), ("\\omega", "ω"),
        ("\\Gamma", "Γ"), ("\\Delta", "Δ"), ("\\Theta", "Θ"), ("\\Lambda", "Λ"),
        ("\\Xi", "Ξ"), ("\\Pi", "Π"), ("\\Sigma", "Σ"), ("\\Phi", "Φ"),
        ("\\Psi", "Ψ"), ("\\Omega", "Ω"),
        ("\\mathbb{R}", "ℝ"), ("\\mathbb{N}", "ℕ"), ("\\mathbb{Z}", "ℤ"),
        ("\\mathbb{Q}", "ℚ"), ("\\mathbb{C}", "ℂ"),
        ("\\left(", "("), ("\\right)", ")"), ("\\left[", "["), ("\\right]", "]"),
        ("\\left\\{", "{"), ("\\right\\}", "}"), ("\\left|", "|"), ("\\right|", "|"),
        ("\\,", " "), ("\\;", " "), ("\\quad", "  "), ("\\qquad", "    "),
        ("\\{", "{"), ("\\}", "}"),
    ]

    private static let superscripts: [Character: Character] = [
        "0": "⁰", "1": "¹", "2": "²", "3": "³", "4": "⁴", "5": "⁵", "6": "⁶", "7": "⁷", "8": "⁸", "9": "⁹",
        "+": "⁺", "-": "⁻", "=": "⁼", "(": "⁽", ")": "⁾", "n": "ⁿ", "i": "ⁱ", "x": "ˣ", "y": "ʸ", "k": "ᵏ",
    ]

    private static let subscripts: [Character: Character] = [
        "0": "₀", "1": "₁", "2": "₂", "3": "₃", "4": "₄", "5": "₅", "6": "₆", "7": "₇", "8": "₈", "9": "₉",
        "+": "₊", "-": "₋", "=": "₌", "(": "₍", ")": "₎", "a": "ₐ", "e": "ₑ", "i": "ᵢ", "j": "ⱼ",
        "k": "ₖ", "m": "ₘ", "n": "ₙ", "o": "ₒ", "p": "ₚ", "r": "ᵣ", "s": "ₛ", "t": "ₜ", "u": "ᵤ", "v": "ᵥ", "x": "ₓ",
    ]

    /// Normalize Anki `\(...\)` and `\[...\]` delimiters to `$...$` and `$$...$$` outside code blocks.
    static func normalizeDelimiters(_ markdown: String) -> String {
        guard markdown.contains("\\(") || markdown.contains("\\[") else { return markdown }
        var outLines: [String] = []
        var inFence = false
        for line in markdown.components(separatedBy: "\n") {
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            if trimmed.hasPrefix("```") {
                inFence.toggle()
                outLines.append(line)
                continue
            }
            if inFence {
                outLines.append(line)
                continue
            }
            var s = line
            s = replaceDelimitedOutsideCode(s, open: "\\[", close: "\\]", newOpen: "$$", newClose: "$$")
            s = replaceDelimitedOutsideCode(s, open: "\\(", close: "\\)", newOpen: "$", newClose: "$")
            outLines.append(s)
        }
        return outLines.joined(separator: "\n")
    }

    private static func replaceDelimitedOutsideCode(
        _ line: String,
        open: String,
        close: String,
        newOpen: String,
        newClose: String
    ) -> String {
        guard line.contains(open) else { return line }
        var out = ""
        var inCode = false
        var idx = line.startIndex
        while idx < line.endIndex {
            if line[idx] == "`" {
                inCode.toggle()
                out.append("`")
                idx = line.index(after: idx)
                continue
            }
            if !inCode && line[idx...].hasPrefix(open) {
                let bodyStart = line.index(idx, offsetBy: open.count)
                if let closeRange = line[bodyStart...].range(of: close) {
                    let body = line[bodyStart..<closeRange.lowerBound].trimmingCharacters(in: .whitespaces)
                    out += "\(newOpen)\(body)\(newClose)"
                    idx = closeRange.upperBound
                    continue
                }
            }
            out.append(line[idx])
            idx = line.index(after: idx)
        }
        return out
    }

    /// Format a LaTeX expression into clean Unicode math text.
    static func formatUnicode(_ raw: String) -> String {
        var s = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        // Strip wrappers like \text{...}, \mathrm{...}, \mathbf{...}, \operatorname{...}
        for cmd in ["\\text", "\\mathrm", "\\mathbf", "\\mathit", "\\mathsf", "\\operatorname"] {
            s = unwrapSingleArgCommand(s, command: cmd)
        }
        // Convert \sqrt{x} -> √(x) or √x
        s = replaceSqrt(s)
        // Convert \frac{a}{b} -> (a)/(b) for inline/nested fractions
        s = replaceInlineFractions(s)
        // Replace known symbols (longest first)
        for (cmd, glyph) in symbols {
            s = s.replacingOccurrences(of: cmd, with: glyph)
        }
        // Convert ^{...} / ^x and _{...} / _x
        s = replaceScripts(s, trigger: "^", map: superscripts)
        s = replaceScripts(s, trigger: "_", map: subscripts)
        // Collapse spaces
        while s.contains("  ") {
            s = s.replacingOccurrences(of: "  ", with: " ")
        }
        return s.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    /// Extract top-level `\frac{num}{den}` when the entire expression is a single fraction.
    static func parseTopLevelFraction(_ raw: String) -> (String, String)? {
        let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        guard trimmed.hasPrefix("\\frac") else { return nil }
        let afterCmd = trimmed.index(trimmed.startIndex, offsetBy: 5)
        guard let (num, nextIdx) = readBracedGroup(trimmed, from: afterCmd),
              let (den, endIdx) = readBracedGroup(trimmed, from: nextIdx),
              trimmed[endIdx...].trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        else { return nil }
        return (num, den)
    }

    private static func readBracedGroup(_ text: String, from start: String.Index) -> (String, String.Index)? {
        var i = start
        while i < text.endIndex && text[i].isWhitespace {
            i = text.index(after: i)
        }
        guard i < text.endIndex, text[i] == "{" else { return nil }
        let bodyStart = text.index(after: i)
        var depth = 1
        var j = bodyStart
        while j < text.endIndex {
            if text[j] == "{" { depth += 1 }
            else if text[j] == "}" {
                depth -= 1
                if depth == 0 {
                    return (String(text[bodyStart..<j]), text.index(after: j))
                }
            }
            j = text.index(after: j)
        }
        return nil
    }

    private static func unwrapSingleArgCommand(_ input: String, command: String) -> String {
        var s = input
        while let range = s.range(of: command) {
            if let (inner, endIdx) = readBracedGroup(s, from: range.upperBound) {
                s.replaceSubrange(range.lowerBound..<endIdx, with: inner)
            } else {
                break
            }
        }
        return s
    }

    private static func replaceSqrt(_ input: String) -> String {
        var s = input
        while let range = s.range(of: "\\sqrt") {
            if let (inner, endIdx) = readBracedGroup(s, from: range.upperBound) {
                let replacement = inner.count <= 2 ? "√\(inner)" : "√(\(inner))"
                s.replaceSubrange(range.lowerBound..<endIdx, with: replacement)
            } else {
                s.replaceSubrange(range, with: "√")
            }
        }
        return s
    }

    private static func replaceInlineFractions(_ input: String) -> String {
        var s = input
        while let range = s.range(of: "\\frac") {
            if let (num, nextIdx) = readBracedGroup(s, from: range.upperBound),
               let (den, endIdx) = readBracedGroup(s, from: nextIdx)
            {
                let numPart = num.contains(" ") || num.contains("+") || num.contains("-") ? "(\(num))" : num
                let denPart = den.contains(" ") || den.contains("+") || den.contains("-") ? "(\(den))" : den
                s.replaceSubrange(range.lowerBound..<endIdx, with: "\(numPart)/\(denPart)")
            } else {
                break
            }
        }
        return s
    }

    private static func replaceScripts(_ input: String, trigger: Character, map: [Character: Character]) -> String {
        var out = ""
        var i = input.startIndex
        while i < input.endIndex {
            if input[i] == trigger {
                let after = input.index(after: i)
                if after < input.endIndex && input[after] == "{" {
                    if let (inner, endIdx) = readBracedGroup(input, from: after) {
                        if inner.allSatisfy({ map[$0] != nil }) {
                            out += String(inner.compactMap { map[$0] })
                        } else {
                            out += "\(trigger)(\(inner))"
                        }
                        i = endIdx
                        continue
                    }
                } else if after < input.endIndex, let mapped = map[input[after]] {
                    out.append(mapped)
                    i = input.index(after: after)
                    continue
                }
            }
            out.append(input[i])
            i = input.index(after: i)
        }
        return out
    }
}

struct MathBlockView: View {
    let latex: String
    let fontSize: CGFloat

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            Group {
                if let (num, den) = LatexMathFormatter.parseTopLevelFraction(latex) {
                    VStack(spacing: 3) {
                        Text(LatexMathFormatter.formatUnicode(num))
                            .font(.system(size: fontSize, weight: .regular, design: .serif).italic())
                        Rectangle()
                            .fill(RemColor.text)
                            .frame(height: 1.2)
                        Text(LatexMathFormatter.formatUnicode(den))
                            .font(.system(size: fontSize, weight: .regular, design: .serif).italic())
                    }
                    .fixedSize()
                } else {
                    Text(LatexMathFormatter.formatUnicode(latex))
                        .font(.system(size: fontSize * 1.08, weight: .regular, design: .serif).italic())
                }
            }
            .foregroundStyle(RemColor.text)
            .padding(.horizontal, 16)
            .padding(.vertical, 12)
            .frame(maxWidth: .infinity, alignment: .center)
        }
        .background(RemColor.bgInset)
        .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
        .overlay(
            RoundedRectangle(cornerRadius: 10, style: .continuous)
                .strokeBorder(RemColor.border, lineWidth: 1)
        )
    }
}

// MARK: - Native Audio Asset Player

@MainActor
final class InlineAudioPlayerModel: NSObject, ObservableObject, AVAudioPlayerDelegate {
    @Published var isPlaying: Bool = false
    @Published var durationText: String? = nil
    @Published var errorMessage: String? = nil

    private var player: AVAudioPlayer?

    func togglePlay(hash: String, store: RemStore) async {
        if let player {
            if player.isPlaying {
                player.pause()
                isPlaying = false
            } else {
                player.currentTime = 0
                player.play()
                isPlaying = true
            }
            return
        }

        do {
            guard let asset = try await store.getAsset(hash: hash) else {
                errorMessage = "Audio not downloaded yet"
                return
            }
            try? AVAudioSession.sharedInstance().setCategory(.playback, mode: .default, options: [.mixWithOthers])
            try? AVAudioSession.sharedInstance().setActive(true)
            let created = try AVAudioPlayer(data: asset.bytes)
            created.delegate = self
            created.prepareToPlay()
            self.player = created
            if created.duration > 0 {
                let secs = max(1, Int(created.duration.rounded()))
                self.durationText = String(format: "%d:%02d", secs / 60, secs % 60)
            }
            created.play()
            isPlaying = true
        } catch {
            errorMessage = "Could not play audio"
        }
    }

    func stop() {
        player?.stop()
        isPlaying = false
    }

    func reset() {
        player?.stop()
        player = nil
        isPlaying = false
        durationText = nil
        errorMessage = nil
    }

    nonisolated func audioPlayerDidFinishPlaying(_ player: AVAudioPlayer, successfully flag: Bool) {
        Task { @MainActor in
            self.isPlaying = false
        }
    }
}

struct AssetAudioPlayerView: View {
    let label: String
    let hash: String
    let store: RemStore

    @StateObject private var model = InlineAudioPlayerModel()

    var body: some View {
        Button {
            Task { await model.togglePlay(hash: hash, store: store) }
        } label: {
            HStack(spacing: 10) {
                Image(systemName: model.isPlaying ? "stop.circle.fill" : "play.circle.fill")
                    .font(.system(size: 24))
                    .foregroundStyle(RemColor.accent)

                VStack(alignment: .leading, spacing: 1) {
                    Text(label.isEmpty ? "Audio clip" : label)
                        .font(.subheadline.weight(.medium))
                        .foregroundStyle(RemColor.text)
                        .lineLimit(1)
                    if let err = model.errorMessage {
                        Text(err)
                            .font(.caption)
                            .foregroundStyle(RemColor.again)
                    } else if let duration = model.durationText {
                        Text(model.isPlaying ? "Playing · \(duration)" : duration)
                            .font(.caption.monospacedDigit())
                            .foregroundStyle(RemColor.muted)
                    } else {
                        Text(model.isPlaying ? "Playing…" : "Tap to play")
                            .font(.caption)
                            .foregroundStyle(RemColor.muted)
                    }
                }
                Spacer()
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 9)
            .background(RemColor.bgInset)
            .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
            .overlay(
                RoundedRectangle(cornerRadius: 10, style: .continuous)
                    .strokeBorder(RemColor.border, lineWidth: 1)
            )
        }
        .buttonStyle(.plain)
        .onChange(of: hash) { _, _ in
            model.reset()
        }
        .onDisappear {
            model.stop()
        }
    }
}
