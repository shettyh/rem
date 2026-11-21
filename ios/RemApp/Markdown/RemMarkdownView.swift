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
    var onTapAssetImage: ((CachedAssetImage) -> Void)?

    private var blocks: [MarkdownBlock] {
        MarkdownBlockParser.parse(markdown)
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
        switch block {
        case .heading(let level, let text):
            let scale: CGFloat = level == 1 ? 1.32 : (level == 2 ? 1.18 : 1.08)
            Text(inlineMarkdown(text))
                .font(.system(size: baseFontSize * scale, weight: .bold))
                .foregroundStyle(textColor)
                .lineSpacing(3)

        case .paragraph(let text):
            Text(inlineMarkdown(text))
                .font(.system(size: baseFontSize, weight: baseWeight))
                .foregroundStyle(textColor)
                .lineSpacing(5)
                .fixedSize(horizontal: false, vertical: true)

        case .codeBlock(let language, let code):
            CodeBlockView(code: code, language: language, fontSize: codeFontSize)

        case .blockquote(let text):
            HStack(alignment: .top, spacing: 12) {
                RoundedRectangle(cornerRadius: 1.5, style: .continuous)
                    .fill(RemColor.borderStrong)
                    .frame(width: 2.5)
                Text(inlineMarkdown(text))
                    .font(.system(size: max(14, baseFontSize - 1.5)))
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
                            .font(.system(size: baseFontSize, weight: .semibold))
                            .foregroundStyle(RemColor.muted)
                        Text(inlineMarkdown(item))
                            .font(.system(size: baseFontSize, weight: baseWeight))
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
                            .font(.system(size: baseFontSize - 1, weight: .semibold, design: .monospaced))
                            .foregroundStyle(RemColor.muted)
                        Text(inlineMarkdown(item))
                            .font(.system(size: baseFontSize, weight: baseWeight))
                            .foregroundStyle(textColor)
                            .lineSpacing(4)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }
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

        case .divider:
            Divider().overlay(RemColor.border)
        }
    }

    private func inlineMarkdown(_ raw: String) -> AttributedString {
        var attributed = (try? AttributedString(
            markdown: raw,
            options: .init(interpretedSyntax: .inlineOnlyPreservingWhitespace)
        )) ?? AttributedString(raw)

        // Style inline code spans with monospace font + subtle inset background
        for run in attributed.runs {
            if let intent = run.inlinePresentationIntent, intent.contains(.code) {
                let range = run.range
                attributed[range].font = .system(size: max(12, baseFontSize * 0.85), design: .monospaced)
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
                        Image(uiImage: loaded.image)
                            .resizable()
                            .scaledToFit()
                            .frame(maxWidth: .infinity, maxHeight: 320)
                            .clipShape(RoundedRectangle(cornerRadius: 9, style: .continuous))

                        HStack {
                            if !alt.isEmpty {
                                Text(alt)
                                    .font(.system(size: 11.5))
                                    .foregroundStyle(RemColor.muted)
                                    .lineLimit(1)
                            }
                            Spacer()
                            Label("Tap to zoom", systemImage: "arrow.up.left.and.arrow.down.right")
                                .font(.system(size: 11, weight: .medium))
                                .foregroundStyle(RemColor.muted)
                        }
                        .padding(.horizontal, 2)
                    }
                    .padding(10)
                    .background(RemColor.chrome)
                    .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
                    .overlay(
                        RoundedRectangle(cornerRadius: 12, style: .continuous)
                            .strokeBorder(RemColor.border, lineWidth: 1)
                    )
                }
                .buttonStyle(.plain)
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
