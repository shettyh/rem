import Foundation
import SwiftUI

enum MarkdownImageSource: Equatable {
    case asset(hash: String)
    case remote(URL)
}

enum MarkdownBlock: Equatable, Identifiable {
    case heading(level: Int, text: String)
    case codeBlock(language: String?, code: String)
    case mathBlock(latex: String)
    case blockquote(text: String)
    case bulletList(items: [String])
    case orderedList(items: [String])
    case table(headers: [String], rows: [[String]])
    case image(alt: String, source: MarkdownImageSource)
    case imageOcclusion(OcclusionBlock)
    case audio(label: String, hash: String)
    case divider
    case paragraph(text: String)

    var id: String {
        switch self {
        case .heading(let level, let text): return "h\(level)-\(text.hashValue)"
        case .codeBlock(let lang, let code): return "code-\(lang ?? "")-\(code.hashValue)"
        case .mathBlock(let latex): return "math-\(latex.hashValue)"
        case .blockquote(let text): return "quote-\(text.hashValue)"
        case .bulletList(let items): return "ul-\(items.joined().hashValue)"
        case .orderedList(let items): return "ol-\(items.joined().hashValue)"
        case .table(let headers, let rows): return "tbl-\(headers.joined().hashValue)-\(rows.count)"
        case .image(let alt, let src): return "img-\(alt)-\(String(describing: src).hashValue)"
        case .imageOcclusion(let block): return "io-\(block.image)-\(block.masks.count)"
        case .audio(let label, let hash): return "audio-\(label)-\(hash)"
        case .divider: return "hr"
        case .paragraph(let text): return "p-\(text.hashValue)"
        }
    }
}

enum MarkdownBlockParser {
    static func parse(_ markdown: String) -> [MarkdownBlock] {
        let normalized = markdown
            .replacingOccurrences(of: "\r\n", with: "\n")
            .replacingOccurrences(of: "\r", with: "\n")
        let lines = normalized.components(separatedBy: "\n")
        var blocks: [MarkdownBlock] = []
        var i = 0

        while i < lines.count {
            let line = lines[i]
            let trimmed = line.trimmingCharacters(in: .whitespaces)

            if trimmed.isEmpty {
                i += 1
                continue
            }

            // Fenced code or rem-occlusion block (``` or ~~~, 3+ chars, optional trailing info)
            if let (fenceChar, fenceLen, info) = ImageOcclusionParser.parseFenceOpener(line) {
                let language = info.isEmpty ? nil : info
                i += 1
                var codeLines: [String] = []
                while i < lines.count {
                    if ImageOcclusionParser.isFenceCloser(lines[i], ch: fenceChar, minLen: fenceLen) {
                        i += 1
                        break
                    }
                    codeLines.append(lines[i])
                    i += 1
                }
                let body = codeLines.joined(separator: "\n")
                if language?.lowercased() == ImageOcclusionParser.fenceLang,
                   let occBlock = ImageOcclusionParser.parseJson(body) {
                    blocks.append(.imageOcclusion(occBlock))
                } else {
                    blocks.append(.codeBlock(language: language, code: body))
                }
                continue
            }

            // Display math block $$...$$ (single-line or multi-line).
            // Never swallow the rest of a card if a line has trailing text after $$...$$
            // (e.g. `$$E = mc^2$$ is Einstein's formula`) or is empty `$$$$`.
            if let (mathBlock, nextIdx) = parseMathBlock(lines: lines, startingAt: i) {
                blocks.append(mathBlock)
                i = nextIdx
                continue
            }

            // Horizontal rule
            if trimmed == "---" || trimmed == "***" || trimmed == "___" {
                blocks.append(.divider)
                i += 1
                continue
            }

            // Standalone image ![alt](src)
            if let img = parseStandaloneImage(trimmed) {
                blocks.append(img)
                i += 1
                continue
            }

            // Standalone audio link [label](asset:<hash>)
            if let audio = parseStandaloneAudio(trimmed) {
                blocks.append(audio)
                i += 1
                continue
            }

            // ATX Heading #..######
            if let heading = parseHeading(trimmed) {
                blocks.append(heading)
                i += 1
                continue
            }

            // Blockquote > ...
            if trimmed.hasPrefix(">") {
                var quoteLines: [String] = []
                while i < lines.count {
                    let qt = lines[i].trimmingCharacters(in: .whitespaces)
                    if !qt.hasPrefix(">") { break }
                    var content = String(qt.dropFirst(1))
                    if content.hasPrefix(" ") { content.removeFirst() }
                    quoteLines.append(content)
                    i += 1
                }
                blocks.append(.blockquote(text: quoteLines.joined(separator: "\n")))
                continue
            }

            // Markdown pipe table (header row + separator row + optional body rows)
            if i + 1 < lines.count,
               let headers = parseTableRow(trimmed),
               isTableSeparator(lines[i + 1].trimmingCharacters(in: .whitespaces), expectedColumns: headers.count)
            {
                i += 2
                var rows: [[String]] = []
                while i < lines.count {
                    let rt = lines[i].trimmingCharacters(in: .whitespaces)
                    guard !rt.isEmpty, rt.contains("|"), let cells = parseTableRow(rt) else { break }
                    var normalizedRow = Array(cells.prefix(headers.count))
                    while normalizedRow.count < headers.count {
                        normalizedRow.append("")
                    }
                    rows.append(normalizedRow)
                    i += 1
                }
                blocks.append(.table(headers: headers, rows: rows))
                continue
            }

            // Bullet list (- or * or +)
            if isBulletLine(trimmed) {
                var items: [String] = []
                while i < lines.count {
                    let lt = lines[i].trimmingCharacters(in: .whitespaces)
                    guard isBulletLine(lt) else { break }
                    items.append(String(lt.dropFirst(2)).trimmingCharacters(in: .whitespaces))
                    i += 1
                }
                blocks.append(.bulletList(items: items))
                continue
            }

            // Ordered list (1. ...)
            if orderedListContent(trimmed) != nil {
                var items: [String] = []
                while i < lines.count {
                    let lt = lines[i].trimmingCharacters(in: .whitespaces)
                    guard let content = orderedListContent(lt) else { break }
                    items.append(content)
                    i += 1
                }
                blocks.append(.orderedList(items: items))
                continue
            }

            // Paragraph (also splits out any embedded standalone images if placed on their own line)
            var paraLines: [String] = []
            while i < lines.count {
                let current = lines[i]
                let ct = current.trimmingCharacters(in: .whitespaces)
                if !paraLines.isEmpty && (
                    ct.isEmpty
                    || ImageOcclusionParser.parseFenceOpener(current) != nil
                    || parseMathBlock(lines: lines, startingAt: i) != nil
                    || ct.hasPrefix(">")
                    || ct == "---" || ct == "***" || ct == "___"
                    || parseHeading(ct) != nil
                    || parseStandaloneImage(ct) != nil
                    || parseStandaloneAudio(ct) != nil
                    || isBulletLine(ct)
                    || orderedListContent(ct) != nil
                    || (i + 1 < lines.count
                        && parseTableRow(ct) != nil
                        && isTableSeparator(
                            lines[i + 1].trimmingCharacters(in: .whitespaces),
                            expectedColumns: parseTableRow(ct)?.count ?? 0
                        ))
                ) {
                    break
                }
                paraLines.append(current)
                i += 1
            }
            if !paraLines.isEmpty {
                let joined = paraLines.joined(separator: "\n")
                appendParagraphOrInlineImages(joined, into: &blocks)
            }
        }

        return blocks
    }

    private static func parseMathBlock(lines: [String], startingAt i: Int) -> (MarkdownBlock, Int)? {
        let trimmed = lines[i].trimmingCharacters(in: .whitespaces)
        guard trimmed.hasPrefix("$$") else { return nil }
        let innerSingle = trimmed.count > 4 && trimmed.hasSuffix("$$")
            ? String(trimmed.dropFirst(2).dropLast(2))
            : nil
        if let innerSingle, !innerSingle.contains("$$") {
            let latex = innerSingle.trimmingCharacters(in: .whitespaces)
            if !latex.isEmpty {
                return (.mathBlock(latex: latex), i + 1)
            }
            return nil
        }
        guard trimmed == "$$" || !String(trimmed.dropFirst(2)).contains("$$") else {
            return nil
        }
        // Only enter multi-line math block when a matching closing `$$` line exists ahead.
        var closingIdx: Int? = nil
        var scan = i + 1
        while scan < lines.count {
            let candidate = lines[scan].trimmingCharacters(in: .whitespaces)
            if candidate.hasSuffix("$$") {
                closingIdx = scan
                break
            }
            scan += 1
        }
        guard let closingIdx else { return nil }
        var mathLines: [String] = []
        let restFirst = String(trimmed.dropFirst(2)).trimmingCharacters(in: .whitespaces)
        if !restFirst.isEmpty { mathLines.append(restFirst) }
        if i + 1 < closingIdx {
            mathLines.append(contentsOf: lines[(i + 1)..<closingIdx])
        }
        let closingLine = lines[closingIdx].trimmingCharacters(in: .whitespaces)
        let before = String(closingLine.dropLast(2)).trimmingCharacters(in: .whitespaces)
        if !before.isEmpty { mathLines.append(before) }
        return (.mathBlock(latex: mathLines.joined(separator: "\n")), closingIdx + 1)
    }

    private static func parseHeading(_ line: String) -> MarkdownBlock? {
        var hashes = 0
        for ch in line {
            if ch == "#" { hashes += 1 } else { break }
        }
        guard (1...6).contains(hashes) else { return nil }
        let rest = line.dropFirst(hashes)
        guard rest.first == " " else { return nil }
        let text = rest.trimmingCharacters(in: .whitespaces)
        return text.isEmpty ? nil : .heading(level: hashes, text: text)
    }

    private static func isBulletLine(_ line: String) -> Bool {
        line.hasPrefix("- ") || line.hasPrefix("* ") || line.hasPrefix("+ ")
    }

    private static func orderedListContent(_ line: String) -> String? {
        guard let dotIdx = line.firstIndex(of: ".") else { return nil }
        let prefix = line[..<dotIdx]
        guard !prefix.isEmpty, prefix.allSatisfy(\.isNumber) else { return nil }
        let afterDot = line.index(after: dotIdx)
        guard afterDot < line.endIndex, line[afterDot] == " " else { return nil }
        return String(line[line.index(after: afterDot)...]).trimmingCharacters(in: .whitespaces)
    }

    private static func parseTableRow(_ line: String) -> [String]? {
        guard line.contains("|") else { return nil }
        var trimmed = line.trimmingCharacters(in: .whitespaces)
        if trimmed.hasPrefix("|") { trimmed.removeFirst() }
        if trimmed.hasSuffix("|") { trimmed.removeLast() }
        let cells = trimmed.split(separator: "|", omittingEmptySubsequences: false)
            .map { $0.trimmingCharacters(in: .whitespaces) }
        return cells.count >= 2 ? cells : nil
    }

    private static func isTableSeparator(_ line: String, expectedColumns: Int) -> Bool {
        guard expectedColumns >= 2, let cells = parseTableRow(line), cells.count == expectedColumns else {
            return false
        }
        return cells.allSatisfy { cell in
            let core = cell.trimmingCharacters(in: CharacterSet(charactersIn: ":"))
            return core.count >= 3 && core.allSatisfy { $0 == "-" }
        }
    }

    /// Produces a clean single-paragraph plain-text snippet for compact list rows (stripping
    /// headings, bold/italic/code delimiters, image links, and code fence markers).
    static func plainTextPreview(_ markdown: String) -> String {
        let strippedOcclusion = ImageOcclusionParser.stripMarkup(markdown)
        let normalized = LatexMathFormatter.normalizeDelimiters(ClozeRenderer.stripMarkup(strippedOcclusion))
            .replacingOccurrences(of: "\r\n", with: "\n")
            .replacingOccurrences(of: "\r", with: "\n")
        var cleanedLines: [String] = []
        for rawLine in normalized.components(separatedBy: "\n") {
            var line = rawLine.trimmingCharacters(in: .whitespaces)
            if line.isEmpty || line.hasPrefix("```") || line == "$$" || line == "---" || line == "***" || line == "___" {
                continue
            }
            if line.hasPrefix("$$") && line.hasSuffix("$$") && line.count > 4 {
                line = LatexMathFormatter.formatUnicode(String(line.dropFirst(2).dropLast(2)))
            }
            // Strip ATX heading prefix
            while line.hasPrefix("#") { line.removeFirst() }
            line = line.trimmingCharacters(in: .whitespaces)
            // Strip blockquote or bullet prefix
            if line.hasPrefix("> ") {
                line = String(line.dropFirst(2))
            } else if isBulletLine(line) {
                line = String(line.dropFirst(2))
            }
            // Replace markdown images ![alt](url) with alt text or "[Image]"
            if let imgRegex = try? NSRegularExpression(pattern: #"!\[([^\]]*)\]\([^)]+\)"#) {
                let ns = line as NSString
                let range = NSRange(location: 0, length: ns.length)
                var out = line
                for match in imgRegex.matches(in: line, range: range).reversed() {
                    let alt = ns.substring(with: match.range(at: 1)).trimmingCharacters(in: .whitespaces)
                    let replacement = alt.isEmpty ? "[Image]" : alt
                    if let swiftRange = Range(match.range, in: out) {
                        out.replaceSubrange(swiftRange, with: replacement)
                    }
                }
                line = out
            }
            // Replace inline markdown links [label](url) -> label
            if let linkRegex = try? NSRegularExpression(pattern: #"\[([^\]]+)\]\([^)]+\)"#) {
                line = linkRegex.stringByReplacingMatches(
                    in: line,
                    range: NSRange(location: 0, length: (line as NSString).length),
                    withTemplate: "$1"
                )
            }
            // Strip common inline formatting markers
            line = line
                .replacingOccurrences(of: "**", with: "")
                .replacingOccurrences(of: "__", with: "")
                .replacingOccurrences(of: "~~", with: "")
                .replacingOccurrences(of: "`", with: "")
                .trimmingCharacters(in: .whitespaces)
            if !line.isEmpty {
                cleanedLines.append(line)
            }
        }
        return cleanedLines.joined(separator: " ")
    }

    private static func parseStandaloneImage(_ line: String) -> MarkdownBlock? {
        guard line.hasPrefix("!["), line.hasSuffix(")"),
              let mid = line.range(of: "](")
        else { return nil }
        let alt = String(line[line.index(line.startIndex, offsetBy: 2)..<mid.lowerBound])
        let rawSrc = String(line[mid.upperBound..<line.index(before: line.endIndex)])
            .trimmingCharacters(in: .whitespaces)
        guard !rawSrc.contains(")") else { return nil }
        guard let source = parseImageSource(rawSrc) else { return nil }
        return .image(alt: alt, source: source)
    }

    private static func cleanAudioLabel(_ raw: String) -> String {
        let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmed.lowercased().hasPrefix("audio:") {
            return String(trimmed.dropFirst("audio:".count)).trimmingCharacters(in: .whitespacesAndNewlines)
        }
        return trimmed
    }

    private static func parseStandaloneAudio(_ line: String) -> MarkdownBlock? {
        guard line.hasPrefix("["), !line.hasPrefix("!["), line.hasSuffix(")"),
              let mid = line.range(of: "](")
        else { return nil }
        let label = cleanAudioLabel(String(line[line.index(after: line.startIndex)..<mid.lowerBound]))
        let rawSrc = String(line[mid.upperBound..<line.index(before: line.endIndex)])
            .trimmingCharacters(in: .whitespaces)
        guard !rawSrc.contains(")"), let source = parseImageSource(rawSrc),
              case .asset(let hash) = source
        else { return nil }
        return .audio(label: label, hash: hash)
    }

    private static func parseImageSource(_ raw: String) -> MarkdownImageSource? {
        if raw.hasPrefix("asset:") {
            let hash = String(raw.dropFirst("asset:".count))
            guard hash.count == 64,
                  hash.utf8.allSatisfy({ ($0 >= 48 && $0 <= 57) || ($0 >= 97 && $0 <= 102) })
            else { return nil }
            return .asset(hash: hash)
        }
        if let url = URL(string: raw), let scheme = url.scheme?.lowercased(),
           scheme == "https" || scheme == "http" {
            return .remote(url)
        }
        return nil
    }

    private static func appendParagraphOrInlineImages(_ text: String, into blocks: inout [MarkdownBlock]) {
        guard let regex = try? NSRegularExpression(pattern: #"(!?)\[([^\]]*)\]\(([^)\s]+)\)"#) else {
            blocks.append(.paragraph(text: text))
            return
        }
        let ns = text as NSString
        let matches = regex.matches(in: text, range: NSRange(location: 0, length: ns.length))
        let mediaMatches = matches.filter { match in
            let isImage = ns.substring(with: match.range(at: 1)) == "!"
            let srcRaw = ns.substring(with: match.range(at: 3))
            if isImage { return parseImageSource(srcRaw) != nil }
            if case .asset = parseImageSource(srcRaw) { return true }
            return false
        }
        if mediaMatches.isEmpty {
            blocks.append(.paragraph(text: text))
            return
        }

        var cursor = 0
        for match in mediaMatches {
            if match.range.location > cursor {
                let before = ns.substring(with: NSRange(location: cursor, length: match.range.location - cursor))
                    .trimmingCharacters(in: .whitespacesAndNewlines)
                if !before.isEmpty {
                    blocks.append(.paragraph(text: before))
                }
            }
            let isImage = ns.substring(with: match.range(at: 1)) == "!"
            let label = ns.substring(with: match.range(at: 2))
            let srcRaw = ns.substring(with: match.range(at: 3))
            if isImage, let source = parseImageSource(srcRaw) {
                blocks.append(.image(alt: label, source: source))
            } else if !isImage, case .asset(let hash) = parseImageSource(srcRaw) {
                blocks.append(.audio(label: cleanAudioLabel(label), hash: hash))
            } else {
                blocks.append(.paragraph(text: ns.substring(with: match.range)))
            }
            cursor = match.range.location + match.range.length
        }

        if cursor < ns.length {
            let tail = ns.substring(from: cursor).trimmingCharacters(in: .whitespacesAndNewlines)
            if !tail.isEmpty {
                blocks.append(.paragraph(text: tail))
            }
        }
    }
}

/// Lightweight GitHub-Dark syntax highlighter for fenced code blocks on mobile.
enum CodeHighlighter {
    private static let kwColor = Color(uiColor: UIColor(hex: 0xFF7B72))
    private static let strColor = Color(uiColor: UIColor(hex: 0xA5D6FF))
    private static let typeColor = Color(uiColor: UIColor(hex: 0xD2A8FF))
    private static let numColor = Color(uiColor: UIColor(hex: 0x79C0FF))
    private static let commentColor = Color(uiColor: UIColor(hex: 0x8B949E))
    private static let plainColor = RemColor.codeFg

    private static let keywords: Set<String> = [
        "fn", "let", "mut", "pub", "struct", "enum", "impl", "trait", "for", "in",
        "match", "if", "else", "while", "loop", "return", "async", "await", "use",
        "mod", "where", "const", "static", "type", "self", "Self", "true", "false",
        "func", "var", "class", "actor", "protocol", "extension", "guard", "switch",
        "case", "default", "import", "throws", "try", "catch", "final", "private",
        "public", "internal", "nil", "function", "export", "from", "satisfies", "as",
        "interface", "extends", "new", "null", "undefined", "def", "None", "True",
        "False", "select", "SELECT", "FROM", "WHERE", "INSERT", "INTO", "VALUES",
        "UPDATE", "SET", "DELETE", "CREATE", "TABLE", "PRIMARY", "KEY", "ORDER", "BY"
    ]

    static func highlight(_ code: String, language: String?) -> AttributedString {
        var result = AttributedString()
        let lines = code.components(separatedBy: "\n")
        for (lineIdx, line) in lines.enumerated() {
            if lineIdx > 0 {
                result.append(AttributedString("\n"))
            }
            result.append(highlightLine(line, language: language))
        }
        return result
    }

    private static func highlightLine(_ line: String, language: String?) -> AttributedString {
        var out = AttributedString()
        let chars = Array(line)
        var i = 0

        let usesHashComment = matchesHashCommentLang(language)

        while i < chars.count {
            let c = chars[i]

            // Line comments: //, --, or # (for py/sh/yaml/toml)
            if (c == "/" && i + 1 < chars.count && chars[i + 1] == "/")
                || (c == "-" && i + 1 < chars.count && chars[i + 1] == "-")
                || (usesHashComment && c == "#")
            {
                let rest = String(chars[i...])
                out.append(styled(rest, color: commentColor))
                break
            }

            // Strings ('...', "...", `...`)
            if c == "\"" || c == "'" || c == "`" {
                let quote = c
                var j = i + 1
                while j < chars.count {
                    if chars[j] == "\\" && j + 1 < chars.count {
                        j += 2
                        continue
                    }
                    if chars[j] == quote {
                        j += 1
                        break
                    }
                    j += 1
                }
                let token = String(chars[i..<j])
                out.append(styled(token, color: strColor))
                i = j
                continue
            }

            // Numbers
            if c.isNumber {
                var j = i + 1
                while j < chars.count && (chars[j].isNumber || chars[j] == "." || chars[j] == "_" || chars[j] == "x" || chars[j].isHexDigit) {
                    j += 1
                }
                let token = String(chars[i..<j])
                out.append(styled(token, color: numColor))
                i = j
                continue
            }

            // Identifiers / Keywords / PascalCase Types
            if c.isLetter || c == "_" {
                var j = i + 1
                while j < chars.count && (chars[j].isLetter || chars[j].isNumber || chars[j] == "_") {
                    j += 1
                }
                let word = String(chars[i..<j])
                if keywords.contains(word) {
                    out.append(styled(word, color: kwColor))
                } else if let first = word.first, first.isUppercase, word.count > 1 {
                    out.append(styled(word, color: typeColor))
                } else {
                    out.append(styled(word, color: plainColor))
                }
                i = j
                continue
            }

            // Plain punctuation / whitespace
            var j = i + 1
            while j < chars.count {
                let nc = chars[j]
                if nc.isLetter || nc.isNumber || nc == "_" || nc == "\"" || nc == "'" || nc == "`" || nc == "/" || nc == "-" || (usesHashComment && nc == "#") {
                    break
                }
                j += 1
            }
            out.append(styled(String(chars[i..<j]), color: plainColor))
            i = j
        }

        return out
    }

    private static func matchesHashCommentLang(_ language: String?) -> Bool {
        guard let lang = language?.lowercased() else { return false }
        return ["py", "python", "sh", "bash", "zsh", "shell", "yaml", "yml", "toml", "rb", "ruby"].contains(lang)
    }

    private static func styled(_ text: String, color: Color) -> AttributedString {
        var s = AttributedString(text)
        s.foregroundColor = color
        return s
    }
}
