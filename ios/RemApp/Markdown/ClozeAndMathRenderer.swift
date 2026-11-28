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

    static func stripSentinels(_ text: String) -> String {
        text
            .replacingOccurrences(of: blankPrefix, with: "")
            .replacingOccurrences(of: blankSuffix, with: "")
            .replacingOccurrences(of: revealPrefix, with: "")
            .replacingOccurrences(of: revealSuffix, with: "")
    }

    private static func isEscaped(_ text: String, at pos: String.Index) -> Bool {
        var backslashes = 0
        var cursor = pos
        while cursor > text.startIndex {
            let prev = text.index(before: cursor)
            if text[prev] == "\\" {
                backslashes += 1
                cursor = prev
            } else {
                break
            }
        }
        return backslashes % 2 == 1
    }

    static func parseSpans(_ text: String) -> [ClozeSpan] {
        guard let lastCloseRange = text.range(of: "}}", options: .backwards) else {
            return []
        }
        let scanLimit = lastCloseRange.upperBound
        var spans: [ClozeSpan] = []
        var i = text.startIndex

        while i < lastCloseRange.lowerBound {
            if text[i] == "`" {
                var endTicks = i
                while endTicks < text.endIndex && text[endTicks] == "`" {
                    endTicks = text.index(after: endTicks)
                }
                let tickCount = text.distance(from: i, to: endTicks)
                let tickStr = String(repeating: "`", count: tickCount)
                var search = endTicks
                var matchedEnd: String.Index? = nil
                while let r = text[search...].range(of: tickStr) {
                    let beforeOk = r.lowerBound == endTicks || text[text.index(before: r.lowerBound)] != "`"
                    let afterOk = r.upperBound == text.endIndex || text[r.upperBound] != "`"
                    if beforeOk && afterOk {
                        matchedEnd = r.upperBound
                        break
                    }
                    search = r.upperBound
                }
                if let matchedEnd {
                    i = matchedEnd
                    continue
                }
                i = endTicks
                continue
            }

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
                            if let (answer, hint, closeEnd) = scanBody(text, from: bodyStart, limit: scanLimit) {
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
        from bodyStart: String.Index,
        limit: String.Index
    ) -> (String, String?, String.Index)? {
        var depth = 0
        var hintSep: String.Index? = nil
        var j = bodyStart

        while j < limit {
            let ch = text[j]
            if ch == "{" && !isEscaped(text, at: j) {
                depth += 1
                j = text.index(after: j)
                continue
            }
            if ch == "}" && !isEscaped(text, at: j) {
                if depth == 0 {
                    let next = text.index(after: j)
                    if next < limit && text[next] == "}" {
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
                if next < limit && text[next] == ":" {
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
                    let trimmed = span.answer.trimmingCharacters(in: .whitespacesAndNewlines)
                    if trimmed.isEmpty {
                        out += "\(blankPrefix)[…]\(blankSuffix)"
                    } else if span.answer.contains("\n") {
                        // Avoid splitting PUA sentinel across multiple Markdown paragraphs.
                        out += span.answer
                    } else {
                        out += "\(revealPrefix)\(span.answer)\(revealSuffix)"
                    }
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

// MARK: - Ruby / Furigana Formatter

enum RubyFormatter {
    struct Token: Equatable {
        let base: String
        let reading: String?
    }

    static func splitTokens(_ text: String) -> [Token] {
        guard text.contains("{"), text.contains("|") else {
            return [Token(base: text, reading: nil)]
        }
        var tokens: [Token] = []
        var plain = ""
        var i = text.startIndex

        func flushPlain() {
            if !plain.isEmpty {
                tokens.append(Token(base: plain, reading: nil))
                plain = ""
            }
        }

        while i < text.endIndex {
            if text[i] == "{" {
                let afterOpen = text.index(after: i)
                if let closeIdx = text[afterOpen...].firstIndex(of: "}") {
                    let inner = String(text[afterOpen..<closeIdx])
                    if !inner.contains("{"), !inner.contains("\n"),
                       let pipeIdx = inner.firstIndex(of: "|"),
                       inner[inner.index(after: pipeIdx)...].firstIndex(of: "|") == nil
                    {
                        let base = String(inner[..<pipeIdx]).trimmingCharacters(in: .whitespaces)
                        let reading = String(inner[inner.index(after: pipeIdx)...]).trimmingCharacters(in: .whitespaces)
                        if !base.isEmpty, !reading.isEmpty {
                            flushPlain()
                            tokens.append(Token(base: base, reading: reading))
                            i = text.index(after: closeIdx)
                            continue
                        }
                    }
                }
            }
            plain.append(text[i])
            i = text.index(after: i)
        }
        flushPlain()
        return tokens
    }

    /// Strip `{base|reading}` outside inline code spans into plain `base` for previews.
    static func stripMarkup(_ text: String) -> String {
        guard text.contains("{"), text.contains("|") else { return text }
        var out = ""
        var i = text.startIndex
        var buf = ""

        func flushBuf() {
            guard !buf.isEmpty else { return }
            out += splitTokens(buf).map(\.base).joined()
            buf = ""
        }

        while i < text.endIndex {
            if text[i] == "`" {
                var endTicks = i
                while endTicks < text.endIndex && text[endTicks] == "`" {
                    endTicks = text.index(after: endTicks)
                }
                let tickCount = text.distance(from: i, to: endTicks)
                let tickStr = String(repeating: "`", count: tickCount)
                var search = endTicks
                var matchedEnd: String.Index? = nil
                while let r = text[search...].range(of: tickStr) {
                    let beforeOk = r.lowerBound == endTicks || text[text.index(before: r.lowerBound)] != "`"
                    let afterOk = r.upperBound == text.endIndex || text[r.upperBound] != "`"
                    if beforeOk && afterOk {
                        matchedEnd = r.upperBound
                        break
                    }
                    search = r.upperBound
                }
                if let matchedEnd {
                    flushBuf()
                    out += text[i..<matchedEnd]
                    i = matchedEnd
                    continue
                }
                buf += text[i..<endTicks]
                i = endTicks
                continue
            }
            buf.append(text[i])
            i = text.index(after: i)
        }
        flushBuf()
        return out
    }
}

// MARK: - Native LaTeX / Math Formatter & View

enum LatexMathFormatter {
    private static let commandMap: [String: String] = [
        "\\Rightarrow": "⇒", "\\Leftarrow": "⇐", "\\Leftrightarrow": "⇔",
        "\\rightarrow": "→", "\\leftarrow": "←", "\\leftrightarrow": "↔",
        "\\mapsto": "↦", "\\to": "→",
        "\\leq": "≤", "\\le": "≤", "\\geq": "≥", "\\ge": "≥",
        "\\neq": "≠", "\\ne": "≠", "\\approx": "≈", "\\equiv": "≡",
        "\\sim": "∼", "\\simeq": "≃", "\\propto": "∝", "\\pm": "±", "\\mp": "∓",
        "\\times": "×", "\\cdot": "·", "\\div": "÷", "\\circ": "∘",
        "\\infty": "∞", "\\partial": "∂", "\\nabla": "∇",
        "\\sum": "∑", "\\prod": "∏", "\\int": "∫", "\\oint": "∮",
        "\\forall": "∀", "\\exists": "∃", "\\neg": "¬", "\\land": "∧", "\\lor": "∨",
        "\\notin": "∉", "\\in": "∈", "\\subseteq": "⊆", "\\subset": "⊂",
        "\\supseteq": "⊇", "\\supset": "⊃", "\\cup": "∪", "\\cap": "∩",
        "\\emptyset": "∅", "\\varnothing": "∅", "\\therefore": "∴", "\\because": "∵",
        "\\degree": "°", "\\angle": "∠", "\\perp": "⊥", "\\parallel": "∥",
        "\\cdots": "⋯", "\\ldots": "…", "\\dots": "…", "\\top": "⊤", "\\bot": "⊥",
        "\\alpha": "α", "\\beta": "β", "\\gamma": "γ", "\\delta": "δ",
        "\\epsilon": "ε", "\\varepsilon": "ε", "\\zeta": "ζ", "\\eta": "η",
        "\\theta": "θ", "\\iota": "ι", "\\kappa": "κ", "\\lambda": "λ",
        "\\mu": "μ", "\\nu": "ν", "\\xi": "ξ", "\\pi": "π",
        "\\rho": "ρ", "\\sigma": "σ", "\\tau": "τ", "\\upsilon": "υ",
        "\\phi": "φ", "\\varphi": "φ", "\\chi": "χ", "\\psi": "ψ", "\\omega": "ω",
        "\\Gamma": "Γ", "\\Delta": "Δ", "\\Theta": "Θ", "\\Lambda": "Λ",
        "\\Xi": "Ξ", "\\Pi": "Π", "\\Sigma": "Σ", "\\Phi": "Φ",
        "\\Psi": "Ψ", "\\Omega": "Ω",
        "\\quad": "  ", "\\qquad": "    ",
        "\\implies": "⇒", "\\iff": "⇔",
    ]

    private static let blackboardMap: [String: String] = [
        "R": "ℝ", "N": "ℕ", "Z": "ℤ", "Q": "ℚ", "C": "ℂ",
    ]

    private static let punctCommandMap: [Character: String] = [
        ",": " ", ";": " ", "{": "{", "}": "}", "|": "‖",
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

    /// Normalize Anki `\(...\)` and `\[...\]` delimiters (single-line or multi-line) to
    /// `$...$` and `$$...$$` outside fenced code blocks and inline code spans.
    static func normalizeDelimiters(_ markdown: String) -> String {
        guard markdown.contains("\\(") || markdown.contains("\\[") else { return markdown }
        let normalizedLineEndings = markdown
            .replacingOccurrences(of: "\r\n", with: "\n")
            .replacingOccurrences(of: "\r", with: "\n")
        var outChunks: [String] = []
        var textLines: [String] = []
        var inFence = false
        var fenceChar: Character = "`"
        var fenceLen = 0

        func flushText() {
            guard !textLines.isEmpty else { return }
            let joined = textLines.joined(separator: "\n")
            var converted = replaceDelimitedOutsideCode(joined, open: "\\[", close: "\\]", newOpen: "$$", newClose: "$$", multilineBlock: true)
            converted = replaceDelimitedOutsideCode(converted, open: "\\(", close: "\\)", newOpen: "$", newClose: "$", multilineBlock: false)
            outChunks.append(converted)
            textLines.removeAll()
        }

        for line in normalizedLineEndings.components(separatedBy: "\n") {
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            if let first = trimmed.first, (first == "`" || first == "~") {
                let runLen = trimmed.prefix(while: { $0 == first }).count
                if runLen >= 3 {
                    let rest = trimmed.dropFirst(runLen).trimmingCharacters(in: .whitespaces)
                    if !inFence {
                        flushText()
                        inFence = true
                        fenceChar = first
                        fenceLen = runLen
                        outChunks.append(line)
                        continue
                    } else if first == fenceChar && runLen >= fenceLen && rest.isEmpty {
                        inFence = false
                        outChunks.append(line)
                        continue
                    }
                }
            }
            if inFence {
                outChunks.append(line)
            } else {
                textLines.append(line)
            }
        }
        flushText()
        return outChunks.joined(separator: "\n")
    }

    private static func isEscaped(_ text: String, at pos: String.Index) -> Bool {
        var backslashes = 0
        var cursor = pos
        while cursor > text.startIndex {
            let prev = text.index(before: cursor)
            if text[prev] == "\\" {
                backslashes += 1
                cursor = prev
            } else {
                break
            }
        }
        return backslashes % 2 == 1
    }

    private static func replaceDelimitedOutsideCode(
        _ text: String,
        open: String,
        close: String,
        newOpen: String,
        newClose: String,
        multilineBlock: Bool
    ) -> String {
        guard text.contains(open) else { return text }
        var out = ""
        var idx = text.startIndex
        while idx < text.endIndex {
            if text[idx] == "`" {
                var endTicks = idx
                while endTicks < text.endIndex && text[endTicks] == "`" {
                    endTicks = text.index(after: endTicks)
                }
                let tickCount = text.distance(from: idx, to: endTicks)
                let tickStr = String(repeating: "`", count: tickCount)
                // Find matching closing run of exact length `tickCount`
                var search = endTicks
                var matchedEnd: String.Index? = nil
                while let r = text[search...].range(of: tickStr) {
                    let beforeOk = r.lowerBound == endTicks || text[text.index(before: r.lowerBound)] != "`"
                    let afterOk = r.upperBound == text.endIndex || text[r.upperBound] != "`"
                    if beforeOk && afterOk {
                        matchedEnd = r.upperBound
                        break
                    }
                    search = r.upperBound
                }
                if let matchedEnd {
                    out += text[idx..<matchedEnd]
                    idx = matchedEnd
                    continue
                }
                out += text[idx..<endTicks]
                idx = endTicks
                continue
            }

            if text[idx...].hasPrefix(open) && !isEscaped(text, at: idx) {
                let bodyStart = text.index(idx, offsetBy: open.count)
                var search = bodyStart
                var closeRange: Range<String.Index>? = nil
                while let r = text[search...].range(of: close) {
                    if !isEscaped(text, at: r.lowerBound) {
                        closeRange = r
                        break
                    }
                    search = text.index(after: r.lowerBound)
                }
                if let closeRange {
                    let rawBody = String(text[bodyStart..<closeRange.lowerBound])
                    let body = rawBody.trimmingCharacters(in: .whitespacesAndNewlines)
                    if multilineBlock && rawBody.contains("\n") {
                        out += "\n\(newOpen)\n\(body)\n\(newClose)\n"
                    } else {
                        out += "\(newOpen)\(body)\(newClose)"
                    }
                    idx = closeRange.upperBound
                    continue
                }
            }
            out.append(text[idx])
            idx = text.index(after: idx)
        }
        return out
    }

    /// Format a LaTeX expression into clean Unicode math text.
    static func formatUnicode(_ raw: String) -> String {
        var s = ClozeRenderer.stripSentinels(raw).trimmingCharacters(in: .whitespacesAndNewlines)
        // Strip wrappers like \text{...}, \mathrm{...}, \mathbf{...}, \operatorname{...}
        for cmd in ["\\text", "\\mathrm", "\\mathbf", "\\mathit", "\\mathsf", "\\operatorname"] {
            s = unwrapSingleArgCommand(s, command: cmd)
        }
        // Convert \mathbb{R} -> ℝ, etc.
        s = replaceBlackboardCommands(s)
        // Convert \sqrt{x} -> √(x) or √x
        s = replaceSqrt(s)
        // Convert \frac{a}{b} -> (a)/(b) for inline/nested fractions
        s = replaceInlineFractions(s)
        // Tokenize and replace \commands cleanly without prefix collisions
        s = replaceLatexCommands(s)
        // Convert ^{...} / ^x and _{...} / _x (recursively handling nested scripts)
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
        let trimmed = ClozeRenderer.stripSentinels(raw).trimmingCharacters(in: .whitespacesAndNewlines)
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
            if text[j] == "{" && !isEscaped(text, at: j) {
                depth += 1
            } else if text[j] == "}" && !isEscaped(text, at: j) {
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
        var searchStart = s.startIndex
        while searchStart < s.endIndex, let range = s[searchStart...].range(of: command) {
            // Ensure command boundary: next char must not be an ASCII letter (avoid matching \text inside \textbf)
            if range.upperBound < s.endIndex && s[range.upperBound].isASCII && s[range.upperBound].isLetter {
                searchStart = range.upperBound
                continue
            }
            if let (inner, endIdx) = readBracedGroup(s, from: range.upperBound) {
                let offset = s.distance(from: s.startIndex, to: range.lowerBound)
                s.replaceSubrange(range.lowerBound..<endIdx, with: inner)
                searchStart = s.index(s.startIndex, offsetBy: offset + inner.count)
            } else {
                searchStart = range.upperBound
            }
        }
        return s
    }

    private static func replaceBlackboardCommands(_ input: String) -> String {
        var s = input
        var searchStart = s.startIndex
        while searchStart < s.endIndex, let range = s[searchStart...].range(of: "\\mathbb") {
            if range.upperBound < s.endIndex && s[range.upperBound].isASCII && s[range.upperBound].isLetter {
                searchStart = range.upperBound
                continue
            }
            if let (inner, endIdx) = readBracedGroup(s, from: range.upperBound) {
                let replacement = blackboardMap[inner.trimmingCharacters(in: .whitespaces)] ?? inner
                let offset = s.distance(from: s.startIndex, to: range.lowerBound)
                s.replaceSubrange(range.lowerBound..<endIdx, with: replacement)
                searchStart = s.index(s.startIndex, offsetBy: offset + replacement.count)
            } else {
                searchStart = range.upperBound
            }
        }
        return s
    }

    private static func replaceSqrt(_ input: String) -> String {
        var s = input
        var searchStart = s.startIndex
        while searchStart < s.endIndex, let range = s[searchStart...].range(of: "\\sqrt") {
            if range.upperBound < s.endIndex && s[range.upperBound].isASCII && s[range.upperBound].isLetter {
                searchStart = range.upperBound
                continue
            }
            if let (inner, endIdx) = readBracedGroup(s, from: range.upperBound) {
                let replacement = inner.count <= 2 ? "√\(inner)" : "√(\(inner))"
                let offset = s.distance(from: s.startIndex, to: range.lowerBound)
                s.replaceSubrange(range.lowerBound..<endIdx, with: replacement)
                searchStart = s.index(s.startIndex, offsetBy: offset + replacement.count)
            } else {
                let offset = s.distance(from: s.startIndex, to: range.lowerBound)
                s.replaceSubrange(range, with: "√")
                searchStart = s.index(s.startIndex, offsetBy: offset + 1)
            }
        }
        return s
    }

    private static func needsFractionParens(_ part: String) -> Bool {
        part.contains(" ") || part.contains("+") || part.contains("-") || part.contains("/")
    }

    private static func replaceInlineFractions(_ input: String) -> String {
        var s = input
        var searchStart = s.startIndex
        while searchStart < s.endIndex, let range = s[searchStart...].range(of: "\\frac") {
            if range.upperBound < s.endIndex && s[range.upperBound].isASCII && s[range.upperBound].isLetter {
                searchStart = range.upperBound
                continue
            }
            if let (rawNum, nextIdx) = readBracedGroup(s, from: range.upperBound),
               let (rawDen, endIdx) = readBracedGroup(s, from: nextIdx)
            {
                let num = replaceInlineFractions(rawNum)
                let den = replaceInlineFractions(rawDen)
                let numPart = needsFractionParens(num) ? "(\(num))" : num
                let denPart = needsFractionParens(den) ? "(\(den))" : den
                let replacement = "\(numPart)/\(denPart)"
                let offset = s.distance(from: s.startIndex, to: range.lowerBound)
                s.replaceSubrange(range.lowerBound..<endIdx, with: replacement)
                searchStart = s.index(s.startIndex, offsetBy: offset + replacement.count)
            } else {
                searchStart = range.upperBound
            }
        }
        return s
    }

    private static func replaceLatexCommands(_ input: String) -> String {
        var out = ""
        var i = input.startIndex
        while i < input.endIndex {
            if input[i] == "\\" {
                let next = input.index(after: i)
                if next < input.endIndex {
                    let nextCh = input[next]
                    if nextCh.isASCII && nextCh.isLetter {
                        var end = next
                        while end < input.endIndex && input[end].isASCII && input[end].isLetter {
                            end = input.index(after: end)
                        }
                        let cmd = String(input[i..<end])
                        if cmd == "\\left" || cmd == "\\right" {
                            // Consume optional delimiter immediately following \left / \right
                            var delimIdx = end
                            while delimIdx < input.endIndex && input[delimIdx] == " " {
                                delimIdx = input.index(after: delimIdx)
                            }
                            if delimIdx < input.endIndex {
                                let d = input[delimIdx]
                                if d == "." {
                                    i = input.index(after: delimIdx)
                                    continue
                                }
                                if d == "\\" {
                                    let afterEsc = input.index(after: delimIdx)
                                    if afterEsc < input.endIndex && (input[afterEsc] == "{" || input[afterEsc] == "}") {
                                        out.append(input[afterEsc])
                                        i = input.index(after: afterEsc)
                                        continue
                                    }
                                }
                                if d == "(" || d == ")" || d == "[" || d == "]" || d == "|" {
                                    out.append(d)
                                    i = input.index(after: delimIdx)
                                    continue
                                }
                            }
                            i = end
                            continue
                        }
                        if let glyph = commandMap[cmd] {
                            out += glyph
                            i = end
                            continue
                        }
                        out += cmd
                        i = end
                        continue
                    } else if let mapped = punctCommandMap[nextCh] {
                        out += mapped
                        i = input.index(after: next)
                        continue
                    }
                }
            }
            out.append(input[i])
            i = input.index(after: i)
        }
        return out
    }

    private static func replaceScripts(_ input: String, trigger: Character, map: [Character: Character]) -> String {
        var out = ""
        var i = input.startIndex
        while i < input.endIndex {
            if input[i] == trigger {
                let after = input.index(after: i)
                if after < input.endIndex && input[after] == "{" {
                    if let (rawInner, endIdx) = readBracedGroup(input, from: after) {
                        let inner = replaceScripts(rawInner, trigger: trigger, map: map)
                        if inner.isEmpty {
                            i = endIdx
                            continue
                        }
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
    private var loadedHash: String?
    private var isLoading: Bool = false
    private var loadGeneration: Int = 0

    private static func fileTypeHint(for mime: String) -> String? {
        switch mime.lowercased() {
        case "audio/mpeg", "audio/mp3": return AVFileType.mp3.rawValue
        case "audio/mp4", "audio/m4a", "audio/aac": return AVFileType.m4a.rawValue
        case "audio/wav", "audio/x-wav": return AVFileType.wav.rawValue
        case "audio/aiff": return AVFileType.aiff.rawValue
        default: return nil
        }
    }

    func togglePlay(hash: String, store: RemStore) async {
        errorMessage = nil

        if let player, loadedHash == hash {
            if player.isPlaying {
                player.pause()
                isPlaying = false
            } else {
                player.currentTime = 0
                if player.play() {
                    isPlaying = true
                } else {
                    isPlaying = false
                    errorMessage = "Could not play audio"
                }
            }
            return
        }

        guard !isLoading else { return }
        isLoading = true
        loadGeneration += 1
        let expectedGen = loadGeneration
        defer { isLoading = false }

        do {
            guard let asset = try await store.getAsset(hash: hash) else {
                guard expectedGen == loadGeneration else { return }
                errorMessage = "Audio not downloaded yet"
                return
            }
            guard expectedGen == loadGeneration else { return }
            try? AVAudioSession.sharedInstance().setCategory(.playback, mode: .default, options: [.mixWithOthers])
            try? AVAudioSession.sharedInstance().setActive(true)
            let hint = Self.fileTypeHint(for: asset.mime)
            let created: AVAudioPlayer
            if let hint {
                created = try AVAudioPlayer(data: asset.bytes, fileTypeHint: hint)
            } else {
                created = try AVAudioPlayer(data: asset.bytes)
            }
            created.delegate = self
            created.prepareToPlay()
            self.player?.stop()
            self.player = created
            self.loadedHash = hash
            if created.duration > 0 {
                let secs = max(1, Int(created.duration.rounded()))
                self.durationText = String(format: "%d:%02d", secs / 60, secs % 60)
            }
            if created.play() {
                isPlaying = true
            } else {
                isPlaying = false
                errorMessage = "Could not play audio"
            }
        } catch {
            guard expectedGen == loadGeneration else { return }
            errorMessage = "Audio format unsupported on iOS"
        }
    }

    func stop() {
        loadGeneration += 1
        player?.stop()
        isPlaying = false
        try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
    }

    func reset() {
        loadGeneration += 1
        player?.stop()
        player = nil
        loadedHash = nil
        isPlaying = false
        durationText = nil
        errorMessage = nil
        try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
    }

    nonisolated func audioPlayerDidFinishPlaying(_ player: AVAudioPlayer, successfully flag: Bool) {
        Task { @MainActor in
            guard self.player === player else { return }
            self.isPlaying = false
            try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
        }
    }

    nonisolated func audioPlayerDecodeErrorDidOccur(_ player: AVAudioPlayer, error: Error?) {
        Task { @MainActor in
            guard self.player === player else { return }
            self.isPlaying = false
            self.errorMessage = "Could not play audio"
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
