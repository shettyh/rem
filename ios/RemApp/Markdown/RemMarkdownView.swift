import SwiftUI
import UIKit

/// Native Quiet Paper Markdown renderer for cards (headings, inline code, dark syntax-highlighted
/// code blocks, blockquotes, lists, and offline `asset:<hash>` images/GIFs).
struct RemMarkdownView: View {
    let markdown: String
    let store: RemStore
    let baseFontSize: CGFloat
    let codeFontSize: CGFloat
    var textColor: Color = RemColor.text
    var baseWeight: Font.Weight = .regular
    var clozeIndex: UInt32? = nil
    var clozeRevealed: Bool = false
    var onTapAssetImage: ((CachedAssetImage) -> Void)?

    private var preparedMarkdown: String {
        let normalized = LatexMathFormatter.normalizeDelimiters(markdown)
        if let clozeIndex, clozeIndex >= 1 {
            return ClozeRenderer.render(normalized, activeIndex: clozeIndex, revealed: clozeRevealed)
        }
        return normalized
    }

    private var blocks: [MarkdownBlock] {
        MarkdownBlockParser.parse(preparedMarkdown)
    }

    private var scaledBaseFontSize: CGFloat {
        UIFontMetrics(forTextStyle: .body).scaledValue(for: baseFontSize)
    }

    private var scaledCodeFontSize: CGFloat {
        UIFontMetrics(forTextStyle: .callout).scaledValue(for: codeFontSize)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            ForEach(Array(blocks.enumerated()), id: \.offset) { _, block in
                renderBlock(block)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    @ViewBuilder
    private func renderBlock(_ block: MarkdownBlock) -> some View {
        let bodySize = scaledBaseFontSize
        switch block {
        case .heading(let level, let text):
            let scale: CGFloat = level == 1 ? 1.32 : (level == 2 ? 1.18 : 1.08)
            Text(inlineMarkdown(text, fontSize: bodySize))
                .font(.system(size: bodySize * scale, weight: .bold))
                .foregroundStyle(textColor)
                .lineSpacing(3)

        case .paragraph(let text):
            Text(inlineMarkdown(text, fontSize: bodySize))
                .font(.system(size: bodySize, weight: baseWeight))
                .foregroundStyle(textColor)
                .lineSpacing(5)
                .fixedSize(horizontal: false, vertical: true)

        case .codeBlock(let language, let code):
            CodeBlockView(code: code, language: language, fontSize: scaledCodeFontSize)

        case .mathBlock(let latex):
            MathBlockView(latex: latex, fontSize: bodySize)

        case .blockquote(let text):
            HStack(alignment: .top, spacing: 12) {
                RoundedRectangle(cornerRadius: 1.5, style: .continuous)
                    .fill(RemColor.borderStrong)
                    .frame(width: 2.5)
                Text(inlineMarkdown(text, fontSize: max(14, bodySize - 1.5)))
                    .font(.system(size: max(14, bodySize - 1.5)))
                    .foregroundStyle(RemColor.muted)
                    .lineSpacing(4)
                    .fixedSize(horizontal: false, vertical: true)
            }
            .padding(.vertical, 2)

        case .bulletList(let items):
            VStack(alignment: .leading, spacing: 7) {
                ForEach(Array(items.enumerated()), id: \.offset) { _, item in
                    HStack(alignment: .top, spacing: 9) {
                        Text("•")
                            .font(.system(size: bodySize, weight: .semibold))
                            .foregroundStyle(RemColor.muted)
                        Text(inlineMarkdown(item, fontSize: bodySize))
                            .font(.system(size: bodySize, weight: baseWeight))
                            .foregroundStyle(textColor)
                            .lineSpacing(4)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }
            }

        case .orderedList(let items):
            VStack(alignment: .leading, spacing: 7) {
                ForEach(Array(items.enumerated()), id: \.offset) { idx, item in
                    HStack(alignment: .top, spacing: 8) {
                        Text("\(idx + 1).")
                            .font(.system(size: bodySize - 1, weight: .semibold, design: .monospaced))
                            .foregroundStyle(RemColor.muted)
                        Text(inlineMarkdown(item, fontSize: bodySize))
                            .font(.system(size: bodySize, weight: baseWeight))
                            .foregroundStyle(textColor)
                            .lineSpacing(4)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }
            }

        case .table(let headers, let rows):
            ScrollView(.horizontal, showsIndicators: false) {
                Grid(alignment: .leading, horizontalSpacing: 0, verticalSpacing: 0) {
                    GridRow {
                        ForEach(Array(headers.enumerated()), id: \.offset) { _, header in
                            Text(inlineMarkdown(header, fontSize: max(12.5, bodySize - 2)))
                                .font(.system(size: max(12.5, bodySize - 2), weight: .semibold))
                                .foregroundStyle(RemColor.text)
                                .padding(.horizontal, 11)
                                .padding(.vertical, 8)
                                .frame(maxWidth: .infinity, alignment: .leading)
                        }
                    }
                    .background(RemColor.bgInset)

                    ForEach(Array(rows.enumerated()), id: \.offset) { _, row in
                        Divider().overlay(RemColor.border)
                        GridRow {
                            ForEach(Array(row.enumerated()), id: \.offset) { _, cell in
                                Text(inlineMarkdown(cell, fontSize: max(12.5, bodySize - 2)))
                                    .font(.system(size: max(12.5, bodySize - 2)))
                                    .foregroundStyle(textColor)
                                    .padding(.horizontal, 11)
                                    .padding(.vertical, 8)
                                    .frame(maxWidth: .infinity, alignment: .leading)
                            }
                        }
                    }
                }
                .background(RemColor.surface)
                .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
                .overlay(
                    RoundedRectangle(cornerRadius: 10, style: .continuous)
                        .strokeBorder(RemColor.border, lineWidth: 1)
                )
            }

        case .image(let alt, let source):
            switch source {
            case .asset(let hash):
                AssetImageView(
                    hash: hash,
                    alt: alt,
                    store: store,
                    onTap: onTapAssetImage
                )
            case .remote(let url):
                AsyncImage(url: url) { phase in
                    switch phase {
                    case .success(let image):
                        image
                            .resizable()
                            .scaledToFit()
                            .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
                    case .failure:
                        offlineImagePlaceholder(label: alt.isEmpty ? "Remote image unavailable offline" : alt)
                    default:
                        ProgressView()
                            .frame(maxWidth: .infinity, minHeight: 80)
                    }
                }
            }

        case .imageOcclusion(let block):
            AssetOcclusionImageView(
                block: block,
                store: store,
                activeIndex: clozeIndex,
                revealed: clozeRevealed,
                onTapAssetImage: onTapAssetImage
            )

        case .audio(let label, let hash):
            AssetAudioPlayerView(label: label, hash: hash, store: store)

        case .divider:
            Divider().overlay(RemColor.border)
        }
    }

    private func inlineMarkdown(_ raw: String, fontSize: CGFloat) -> AttributedString {
        // Split out cloze highlight sentinels and inline $...$ math spans before markdown styling.
        var result = AttributedString()
        var cursor = raw.startIndex

        while cursor < raw.endIndex {
            let ch = raw[cursor]
            if String(ch) == ClozeRenderer.blankPrefix,
               let end = raw[raw.index(after: cursor)...].range(of: ClozeRenderer.blankSuffix)
            {
                let inner = String(raw[raw.index(after: cursor)..<end.lowerBound])
                var pill = AttributedString(" \(inner) ")
                pill.foregroundColor = RemColor.accent
                pill.backgroundColor = RemColor.accentTint
                pill.font = .system(size: fontSize, weight: .semibold)
                result.append(pill)
                cursor = end.upperBound
                continue
            }

            if String(ch) == ClozeRenderer.revealPrefix,
               let end = raw[raw.index(after: cursor)...].range(of: ClozeRenderer.revealSuffix)
            {
                let inner = String(raw[raw.index(after: cursor)..<end.lowerBound])
                var pill = renderInlineMarkdownOrMath(inner, fontSize: fontSize)
                pill.foregroundColor = RemColor.accent
                pill.backgroundColor = RemColor.accentTint
                // Preserve inner inline code/math fonts while emphasizing plain text runs.
                for run in pill.runs where run.font == nil {
                    pill[run.range].font = .system(size: fontSize, weight: .semibold)
                }
                result.append(pill)
                cursor = end.upperBound
                continue
            }

            // Scan plain chunk up to next cloze sentinel
            var nextSpecial = raw.index(after: cursor)
            while nextSpecial < raw.endIndex {
                let s = String(raw[nextSpecial])
                if s == ClozeRenderer.blankPrefix || s == ClozeRenderer.revealPrefix {
                    break
                }
                nextSpecial = raw.index(after: nextSpecial)
            }
            let chunk = String(raw[cursor..<nextSpecial])
            result.append(renderInlineMarkdownOrMath(chunk, fontSize: fontSize))
            cursor = nextSpecial
        }
        return result
    }

    private func renderInlineMarkdownOrMath(_ raw: String, fontSize: CGFloat) -> AttributedString {
        let cleaned = ClozeRenderer.stripSentinels(raw)
        guard cleaned.contains("$") else {
            return parsePlainInlineMarkdown(cleaned, fontSize: fontSize)
        }
        var out = AttributedString()
        var idx = cleaned.startIndex
        var buffer = ""

        func isEscaped(_ pos: String.Index) -> Bool {
            var count = 0
            var c = pos
            while c > cleaned.startIndex {
                let p = cleaned.index(before: c)
                if cleaned[p] == "\\" {
                    count += 1
                    c = p
                } else {
                    break
                }
            }
            return count % 2 == 1
        }

        while idx < cleaned.endIndex {
            let ch = cleaned[idx]
            if ch == "`" {
                var endTicks = idx
                while endTicks < cleaned.endIndex && cleaned[endTicks] == "`" {
                    endTicks = cleaned.index(after: endTicks)
                }
                let tickCount = cleaned.distance(from: idx, to: endTicks)
                let tickStr = String(repeating: "`", count: tickCount)
                var search = endTicks
                var matchedEnd: String.Index? = nil
                while let r = cleaned[search...].range(of: tickStr) {
                    let beforeOk = r.lowerBound == endTicks || cleaned[cleaned.index(before: r.lowerBound)] != "`"
                    let afterOk = r.upperBound == cleaned.endIndex || cleaned[r.upperBound] != "`"
                    if beforeOk && afterOk {
                        matchedEnd = r.upperBound
                        break
                    }
                    search = r.upperBound
                }
                if let matchedEnd {
                    buffer += cleaned[idx..<matchedEnd]
                    idx = matchedEnd
                    continue
                }
                buffer += cleaned[idx..<endTicks]
                idx = endTicks
                continue
            }
            if ch == "$" && !isEscaped(idx) {
                let after = cleaned.index(after: idx)
                let isDouble = after < cleaned.endIndex && cleaned[after] == "$"
                let delim = isDouble ? "$$" : "$"
                let bodyStart = isDouble ? cleaned.index(after: after) : after
                if bodyStart < cleaned.endIndex {
                    var search = bodyStart
                    var closeRange: Range<String.Index>? = nil
                    while let r = cleaned[search...].range(of: delim) {
                        if !isEscaped(r.lowerBound) {
                            if !isDouble {
                                let afterClose = r.upperBound
                                let followedByDigit = afterClose < cleaned.endIndex && cleaned[afterClose].isASCII && cleaned[afterClose].isNumber
                                if followedByDigit {
                                    search = r.upperBound
                                    continue
                                }
                            }
                            closeRange = r
                            break
                        }
                        search = cleaned.index(after: r.lowerBound)
                    }
                    if let closeRange {
                        let body = String(cleaned[bodyStart..<closeRange.lowerBound])
                        let looksLikeCurrency = !isDouble && (body.hasPrefix(" ") || body.hasSuffix(" ") || body.isEmpty)
                        if !looksLikeCurrency {
                            if !buffer.isEmpty {
                                out.append(parsePlainInlineMarkdown(buffer, fontSize: fontSize))
                                buffer = ""
                            }
                            var math = AttributedString(LatexMathFormatter.formatUnicode(body))
                            math.font = .system(size: fontSize, weight: .regular, design: .serif).italic()
                            out.append(math)
                            idx = closeRange.upperBound
                            continue
                        }
                    }
                }
            }
            buffer.append(ch)
            idx = cleaned.index(after: idx)
        }
        if !buffer.isEmpty {
            out.append(parsePlainInlineMarkdown(buffer, fontSize: fontSize))
        }
        return out
    }

    private func parsePlainInlineMarkdown(_ raw: String, fontSize: CGFloat) -> AttributedString {
        var attributed = (try? AttributedString(
            markdown: raw,
            options: .init(interpretedSyntax: .inlineOnlyPreservingWhitespace)
        )) ?? AttributedString(raw)

        // Style inline code spans with monospace font + subtle inset background
        for run in attributed.runs {
            if let intent = run.inlinePresentationIntent, intent.contains(.code) {
                let range = run.range
                attributed[range].font = .system(size: max(12, fontSize * 0.85), design: .monospaced)
                attributed[range].backgroundColor = RemColor.surfaceInset
                attributed[range].foregroundColor = RemColor.text
            }
            if run.link != nil {
                attributed[run.range].foregroundColor = RemColor.accent
            }
        }
        return attributed
    }

    private func offlineImagePlaceholder(label: String) -> some View {
        HStack(spacing: 8) {
            Image(systemName: "photo")
                .foregroundStyle(RemColor.muted)
            Text(label)
                .font(.system(size: 12.5))
                .foregroundStyle(RemColor.muted)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(12)
        .background(RemColor.bgInset)
        .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
        .overlay(
            RoundedRectangle(cornerRadius: 10, style: .continuous)
                .strokeBorder(RemColor.border, lineWidth: 1)
        )
    }
}

struct CodeBlockView: View {
    let code: String
    let language: String?
    let fontSize: CGFloat

    @State private var copied: Bool = false

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text(language?.lowercased() ?? "code")
                    .font(.system(size: 10.5, weight: .semibold, design: .monospaced))
                    .foregroundStyle(Color.white.opacity(0.45))
                Spacer()
                Button {
                    UIPasteboard.general.string = code
                    copied = true
                    Task {
                        try? await Task.sleep(for: .seconds(1.5))
                        copied = false
                    }
                } label: {
                    Text(copied ? "Copied" : "Copy")
                        .font(.system(size: 10.5, weight: .semibold))
                        .foregroundStyle(copied ? RemColor.good : Color.white.opacity(0.55))
                }
            }
            .padding(.horizontal, 13)
            .padding(.top, 9)
            .padding(.bottom, 6)

            ScrollView(.horizontal, showsIndicators: false) {
                Text(CodeHighlighter.highlight(code, language: language))
                    .font(.system(size: fontSize, design: .monospaced))
                    .lineSpacing(4)
                    .padding(.horizontal, 13)
                    .padding(.bottom, 12)
                    .textSelection(.enabled)
            }
        }
        .background(RemColor.codeBg)
        .clipShape(RoundedRectangle(cornerRadius: 11, style: .continuous))
        .overlay(
            RoundedRectangle(cornerRadius: 11, style: .continuous)
                .strokeBorder(Color.white.opacity(0.07), lineWidth: 1)
        )
    }
}

struct AssetImageView: View {
    let hash: String
    let alt: String
    let store: RemStore
    var onTap: ((CachedAssetImage) -> Void)?

    @State private var loaded: CachedAssetImage?
    @State private var didAttemptLoad: Bool = false

    var body: some View {
        Group {
            if let loaded {
                Button {
                    onTap?(loaded)
                } label: {
                    VStack(alignment: .leading, spacing: 6) {
                        ZStack(alignment: .bottomTrailing) {
                            Image(uiImage: loaded.image)
                                .resizable()
                                .scaledToFit()
                                .frame(maxWidth: .infinity, maxHeight: 320)
                                .clipShape(RoundedRectangle(cornerRadius: 11, style: .continuous))
                                .overlay(
                                    RoundedRectangle(cornerRadius: 11, style: .continuous)
                                        .strokeBorder(RemColor.border, lineWidth: 1)
                                )

                            Image(systemName: "arrow.up.left.and.arrow.down.right")
                                .font(.system(size: 11, weight: .semibold))
                                .foregroundStyle(Color.white)
                                .padding(6)
                                .background(Color.black.opacity(0.55))
                                .clipShape(Circle())
                                .padding(8)
                        }

                        if !alt.isEmpty {
                            Text(alt)
                                .font(.caption)
                                .foregroundStyle(RemColor.muted)
                                .lineLimit(2)
                                .padding(.horizontal, 2)
                        }
                    }
                }
                .buttonStyle(.plain)
                .accessibilityLabel(alt.isEmpty ? "Image. Tap to zoom" : "\(alt). Tap to zoom")
            } else if didAttemptLoad {
                HStack(spacing: 8) {
                    Image(systemName: "photo.badge.exclamationmark")
                        .foregroundStyle(RemColor.muted)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(alt.isEmpty ? "Image not downloaded yet" : alt)
                            .font(.system(size: 12.5, weight: .medium))
                            .foregroundStyle(RemColor.text)
                        Text("asset:\(hash.prefix(12))… · sync online to cache offline")
                            .font(.system(size: 11, design: .monospaced))
                            .foregroundStyle(RemColor.muted)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(12)
                .background(RemColor.bgInset)
                .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
                .overlay(
                    RoundedRectangle(cornerRadius: 10, style: .continuous)
                        .strokeBorder(RemColor.border, lineWidth: 1)
                )
            } else {
                ProgressView()
                    .frame(maxWidth: .infinity, minHeight: 72)
                    .background(RemColor.bgInset)
                    .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
            }
        }
        .task(id: hash) {
            self.loaded = await AssetImageCache.shared.load(hash: hash, store: store)
            self.didAttemptLoad = true
        }
    }
}
