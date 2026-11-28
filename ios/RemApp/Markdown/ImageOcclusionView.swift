import PhotosUI
import SwiftUI
import UIKit

enum OcclusionMode: String, Codable, Equatable {
    case hideAll = "hide-all"
    case hideOne = "hide-one"
}

enum OcclusionShapeType: String, Codable, Equatable {
    case rect
    case ellipse
}

struct OcclusionMask: Codable, Equatable {
    var id: UInt32
    var type: OcclusionShapeType
    var x: Double
    var y: Double
    var w: Double
    var h: Double
    var label: String?

    enum CodingKeys: String, CodingKey {
        case id, type, x, y, w, h, label
    }

    init(
        id: UInt32,
        type: OcclusionShapeType = .rect,
        x: Double,
        y: Double,
        w: Double,
        h: Double,
        label: String? = nil
    ) {
        self.id = id
        self.type = type
        self.x = x
        self.y = y
        self.w = w
        self.h = h
        self.label = label
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        self.id = try c.decode(UInt32.self, forKey: .id)
        let rawType = (try? c.decode(String.self, forKey: .type)) ?? "rect"
        self.type = OcclusionShapeType(rawValue: rawType) ?? .rect
        self.x = try c.decode(Double.self, forKey: .x)
        self.y = try c.decode(Double.self, forKey: .y)
        self.w = try c.decode(Double.self, forKey: .w)
        self.h = try c.decode(Double.self, forKey: .h)
        self.label = try c.decodeIfPresent(String.self, forKey: .label)
    }
}

struct OcclusionBlock: Codable, Equatable {
    var v: Int
    var image: String
    var mode: OcclusionMode
    var alt: String?
    var masks: [OcclusionMask]

    enum CodingKeys: String, CodingKey {
        case v, image, mode, alt, masks
    }

    init(
        v: Int = 1,
        image: String,
        mode: OcclusionMode = .hideAll,
        alt: String? = nil,
        masks: [OcclusionMask] = []
    ) {
        self.v = v
        self.image = image
        self.mode = mode
        self.alt = alt
        self.masks = masks
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        self.v = try c.decode(Int.self, forKey: .v)
        self.image = try c.decode(String.self, forKey: .image)
        let rawMode = (try? c.decode(String.self, forKey: .mode)) ?? "hide-all"
        self.mode = OcclusionMode(rawValue: rawMode) ?? .hideAll
        self.alt = try c.decodeIfPresent(String.self, forKey: .alt)
        self.masks = (try? c.decode([OcclusionMask].self, forKey: .masks)) ?? []
    }

    var assetHash: String? {
        guard image.hasPrefix("asset:") else { return nil }
        let hash = String(image.dropFirst("asset:".count))
        guard hash.count == 64,
              hash.utf8.allSatisfy({ ($0 >= 48 && $0 <= 57) || ($0 >= 97 && $0 <= 102) })
        else { return nil }
        return hash
    }
}

struct OcclusionDocument: Equatable {
    var header: String
    var block: OcclusionBlock
    var footer: String
}

enum ImageOcclusionParser {
    static let fenceLang = "rem-occlusion"
    static let maxMasks = 200
    static let minExtent = 0.002

    private static func round4(_ value: Double) -> Double {
        (value * 10_000.0).rounded() / 10_000.0
    }

    private static func clamp(_ value: Double, min lo: Double, max hi: Double) -> Double {
        Swift.min(hi, Swift.max(lo, value))
    }

    static func sanitizeMask(_ mask: OcclusionMask) -> OcclusionMask? {
        guard mask.id >= 1 else { return nil }
        guard mask.x.isFinite, mask.y.isFinite, mask.w.isFinite, mask.h.isFinite else { return nil }
        guard mask.w >= minExtent, mask.h >= minExtent else { return nil }
        let x = clamp(mask.x, min: 0.0, max: 1.0 - minExtent)
        let y = clamp(mask.y, min: 0.0, max: 1.0 - minExtent)
        let w = clamp(mask.w, min: minExtent, max: Swift.max(minExtent, 1.0 - x))
        let h = clamp(mask.h, min: minExtent, max: Swift.max(minExtent, 1.0 - y))
        let trimmedLabel = mask.label?.trimmingCharacters(in: .whitespacesAndNewlines)
        return OcclusionMask(
            id: mask.id,
            type: mask.type,
            x: round4(x),
            y: round4(y),
            w: round4(w),
            h: round4(h),
            label: (trimmedLabel?.isEmpty == false) ? trimmedLabel : nil
        )
    }

    static func parseJson(_ raw: String) -> OcclusionBlock? {
        guard let data = raw.trimmingCharacters(in: .whitespacesAndNewlines).data(using: .utf8),
              let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        else { return nil }
        guard let versionNum = json["v"] as? NSNumber,
              !isJSONBool(versionNum),
              versionNum.intValue == 1,
              versionNum.doubleValue == 1.0,
              let rawImage = json["image"] as? String
        else { return nil }
        let image = rawImage.trimmingCharacters(in: .whitespacesAndNewlines)
        let mode: OcclusionMode = (json["mode"] as? String) == "hide-one" ? .hideOne : .hideAll
        let trimmedAlt = (json["alt"] as? String)?.trimmingCharacters(in: .whitespacesAndNewlines)
        let alt = (trimmedAlt?.isEmpty == false) ? trimmedAlt : nil

        var masks: [OcclusionMask] = []
        if let rawMasks = json["masks"] as? [Any] {
            for item in rawMasks {
                if masks.count >= maxMasks { break }
                guard let dict = item as? [String: Any],
                      let mask = parseMaskDictionary(dict)
                else { continue }
                masks.append(mask)
            }
        }

        let block = OcclusionBlock(v: 1, image: image, mode: mode, alt: alt, masks: masks)
        guard block.assetHash != nil else { return nil }
        return block
    }

    private static func isJSONBool(_ num: NSNumber) -> Bool {
        CFGetTypeID(num) == CFBooleanGetTypeID()
    }

    private static func parseMaskDictionary(_ dict: [String: Any]) -> OcclusionMask? {
        guard let idNum = dict["id"] as? NSNumber, !isJSONBool(idNum) else { return nil }
        let idDouble = idNum.doubleValue
        guard idDouble.isFinite,
              idDouble >= 1,
              idDouble <= Double(UInt32.max),
              idDouble.rounded() == idDouble
        else { return nil }
        guard let xNum = dict["x"] as? NSNumber, !isJSONBool(xNum),
              let yNum = dict["y"] as? NSNumber, !isJSONBool(yNum),
              let wNum = dict["w"] as? NSNumber, !isJSONBool(wNum),
              let hNum = dict["h"] as? NSNumber, !isJSONBool(hNum)
        else { return nil }

        let shapeType: OcclusionShapeType = (dict["type"] as? String) == "ellipse" ? .ellipse : .rect
        let label = dict["label"] as? String
        return sanitizeMask(OcclusionMask(
            id: UInt32(idDouble),
            type: shapeType,
            x: xNum.doubleValue,
            y: yNum.doubleValue,
            w: wNum.doubleValue,
            h: hNum.doubleValue,
            label: label
        ))
    }

    static func serializeBlock(_ block: OcclusionBlock) -> String {
        let trimmedAlt = block.alt?.trimmingCharacters(in: .whitespacesAndNewlines)
        let sanitized = OcclusionBlock(
            v: 1,
            image: block.image.trimmingCharacters(in: .whitespacesAndNewlines),
            mode: block.mode,
            alt: (trimmedAlt?.isEmpty == false) ? trimmedAlt : nil,
            masks: Array(block.masks.compactMap(sanitizeMask).prefix(maxMasks))
        )
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        let json = (try? encoder.encode(sanitized)).flatMap { String(data: $0, encoding: .utf8) } ?? "{}"
        return "```\(fenceLang)\n\(json)\n```"
    }

    private struct Span {
        let startLine: Int
        let endLine: Int // exclusive
        let block: OcclusionBlock
    }

    static func parseFenceOpener(_ line: String) -> (Character, Int, String)? {
        var idx = line.startIndex
        var spaces = 0
        while idx < line.endIndex && spaces < 3 && line[idx] == " " {
            idx = line.index(after: idx)
            spaces += 1
        }
        guard idx < line.endIndex else { return nil }
        let ch = line[idx]
        guard ch == "`" || ch == "~" else { return nil }
        var count = 0
        var scan = idx
        while scan < line.endIndex && line[scan] == ch {
            scan = line.index(after: scan)
            count += 1
        }
        guard count >= 3 else { return nil }
        let rest = String(line[scan...]).trimmingCharacters(in: .whitespaces)
        if ch == "`" && rest.contains("`") { return nil }
        let info = rest.split(whereSeparator: \.isWhitespace).first.map(String.init) ?? ""
        return (ch, count, info)
    }

    static func isFenceCloser(_ line: String, ch: Character, minLen: Int) -> Bool {
        var idx = line.startIndex
        var spaces = 0
        while idx < line.endIndex && spaces < 3 && line[idx] == " " {
            idx = line.index(after: idx)
            spaces += 1
        }
        var count = 0
        var scan = idx
        while scan < line.endIndex && line[scan] == ch {
            scan = line.index(after: scan)
            count += 1
        }
        guard count >= minLen else { return false }
        return line[scan...].trimmingCharacters(in: .whitespaces).isEmpty
    }

    private static func findSpans(in lines: [String]) -> [Span] {
        var spans: [Span] = []
        var i = 0
        while i < lines.count {
            if let (ch, len, info) = parseFenceOpener(lines[i]) {
                var j = i + 1
                var closedAt: Int? = nil
                while j < lines.count {
                    if isFenceCloser(lines[j], ch: ch, minLen: len) {
                        closedAt = j
                        break
                    }
                    j += 1
                }
                guard let closedAt else { break }
                if info.lowercased() == fenceLang {
                    let body = lines[(i + 1)..<closedAt].joined(separator: "\n")
                    if let block = parseJson(body) {
                        spans.append(Span(startLine: i, endLine: closedAt + 1, block: block))
                    }
                }
                i = closedAt + 1
                continue
            }
            i += 1
        }
        return spans
    }

    static func parseBlocks(_ markdown: String) -> [OcclusionBlock] {
        let normalized = markdown
            .replacingOccurrences(of: "\r\n", with: "\n")
            .replacingOccurrences(of: "\r", with: "\n")
        return findSpans(in: normalized.components(separatedBy: "\n")).map(\.block)
    }

    static func hasOcclusion(_ markdown: String) -> Bool {
        !parseBlocks(markdown).isEmpty
    }

    static func extractIndices(_ markdown: String) -> [UInt32] {
        var set = Set<UInt32>()
        for block in parseBlocks(markdown) {
            for mask in block.masks {
                set.insert(mask.id)
            }
        }
        return set.sorted()
    }

    static func extractDocument(_ markdown: String) -> OcclusionDocument? {
        let normalized = markdown
            .replacingOccurrences(of: "\r\n", with: "\n")
            .replacingOccurrences(of: "\r", with: "\n")
        let lines = normalized.components(separatedBy: "\n")
        guard let first = findSpans(in: lines).first else { return nil }
        let header = lines[0..<first.startLine]
            .joined(separator: "\n")
            .trimmingCharacters(in: .whitespacesAndNewlines)
        let footer = (first.endLine < lines.count ? lines[first.endLine..<lines.count] : [])
            .joined(separator: "\n")
            .trimmingCharacters(in: .whitespacesAndNewlines)
        return OcclusionDocument(header: header, block: first.block, footer: footer)
    }

    private static func sanitizeHeaderBeforeOcclusion(_ header: String) -> String {
        let normalized = header
            .replacingOccurrences(of: "\r\n", with: "\n")
            .replacingOccurrences(of: "\r", with: "\n")
        var openFence: (Character, Int)? = nil
        for line in normalized.components(separatedBy: "\n") {
            if let (ch, len) = openFence {
                if isFenceCloser(line, ch: ch, minLen: len) {
                    openFence = nil
                }
            } else if let (ch, len, _) = parseFenceOpener(line) {
                openFence = (ch, len)
            }
        }
        guard let (ch, len) = openFence else { return normalized }
        return "\(normalized.trimmingCharacters(in: .whitespacesAndNewlines))\n\(String(repeating: ch, count: len))"
    }

    static func composeDocument(
        header: String,
        block: OcclusionBlock,
        footer: String = "",
        syncAltFromHeader: Bool = false
    ) -> String {
        let sanitizedHeader = sanitizeHeaderBeforeOcclusion(header)
            .trimmingCharacters(in: .whitespacesAndNewlines)
        let trimmedFooter = footer.trimmingCharacters(in: .whitespacesAndNewlines)
        var updatedBlock = block
        let derivedAlt: String? = {
            guard !sanitizedHeader.isEmpty else { return nil }
            let firstLine = sanitizedHeader.components(separatedBy: "\n").first ?? ""
            let cleaned = firstLine.drop(while: { $0 == "#" || $0 == " " }).trimmingCharacters(in: .whitespaces)
            return cleaned.isEmpty ? nil : cleaned
        }()
        if syncAltFromHeader {
            updatedBlock.alt = derivedAlt
        } else if let derivedAlt {
            updatedBlock.alt = derivedAlt
        }
        var parts: [String] = []
        if !sanitizedHeader.isEmpty { parts.append(sanitizedHeader) }
        parts.append(serializeBlock(updatedBlock))
        if !trimmedFooter.isEmpty { parts.append(trimmedFooter) }
        return parts.joined(separator: "\n\n")
    }

    static func stripMarkup(_ markdown: String) -> String {
        let normalized = markdown
            .replacingOccurrences(of: "\r\n", with: "\n")
            .replacingOccurrences(of: "\r", with: "\n")
        let lines = normalized.components(separatedBy: "\n")
        let spans = findSpans(in: lines)
        guard !spans.isEmpty else { return markdown }
        var out: [String] = []
        var cursor = 0
        for span in spans {
            if span.startLine > cursor {
                out.append(contentsOf: lines[cursor..<span.startLine])
            }
            let groups = Set(span.block.masks.map(\.id)).count
            if let alt = span.block.alt, !alt.isEmpty {
                out.append("[Image occlusion: \(alt)]")
            } else if groups == 1 {
                out.append("[Image occlusion · 1 mask]")
            } else if groups > 1 {
                out.append("[Image occlusion · \(groups) masks]")
            } else {
                out.append("[Image occlusion]")
            }
            cursor = span.endLine
        }
        if cursor < lines.count {
            out.append(contentsOf: lines[cursor..<lines.count])
        }
        return out.joined(separator: "\n").trimmingCharacters(in: .whitespacesAndNewlines)
    }

    static func nextMaskId(in masks: [OcclusionMask]) -> UInt32 {
        (masks.map(\.id).max() ?? 0) + 1
    }

    /// Composite the occlusion masks onto a bounded-resolution `UIImage` for safe zoomable
    /// lightbox inspection without exposing unrevealed labels or allocating huge bitmaps.
    static func renderCompositeImage(
        base: UIImage,
        block: OcclusionBlock,
        activeIndex: UInt32?,
        revealed: Bool,
        peekAll: Bool
    ) -> UIImage {
        let rawSize = base.size
        guard rawSize.width > 0, rawSize.height > 0 else { return base }
        let maxDimension: CGFloat = 2048.0
        let rawPixelW = rawSize.width * base.scale
        let rawPixelH = rawSize.height * base.scale
        let longest = max(rawPixelW, rawPixelH)
        let downscale = longest > maxDimension ? (maxDimension / longest) : 1.0
        let targetSize = CGSize(
            width: max(1, (rawPixelW * downscale).rounded()),
            height: max(1, (rawPixelH * downscale).rounded())
        )

        let format = UIGraphicsImageRendererFormat()
        format.scale = 1.0
        format.opaque = false
        let darkTraits = UITraitCollection(userInterfaceStyle: .dark)
        let accent = UIColor(RemColor.accent).resolvedColor(with: darkTraits)
        let muted = UIColor.systemGray2.resolvedColor(with: darkTraits)

        let renderer = UIGraphicsImageRenderer(size: targetSize, format: format)
        return renderer.image { ctx in
            base.draw(in: CGRect(origin: .zero, size: targetSize))
            let cg = ctx.cgContext
            let strokeWidth = max(2.0, min(targetSize.width, targetSize.height) * 0.004)

            for mask in block.masks {
                let rect = CGRect(
                    x: mask.x * targetSize.width,
                    y: mask.y * targetSize.height,
                    width: mask.w * targetSize.width,
                    height: mask.h * targetSize.height
                )
                let path: UIBezierPath = mask.type == .ellipse
                    ? UIBezierPath(ovalIn: rect)
                    : UIBezierPath(roundedRect: rect, cornerRadius: min(rect.width, rect.height) * 0.08)

                if let activeIndex {
                    let isActive = mask.id == activeIndex
                    if isActive {
                        if revealed {
                            cg.setStrokeColor(accent.cgColor)
                            path.lineWidth = strokeWidth * 1.25
                            path.stroke()
                        } else {
                            cg.setFillColor(accent.cgColor)
                            path.fill()
                            cg.setStrokeColor(UIColor.white.cgColor)
                            path.lineWidth = strokeWidth
                            path.stroke()
                        }
                    } else if block.mode == .hideAll {
                        if revealed && peekAll {
                            cg.setStrokeColor(muted.withAlphaComponent(0.75).cgColor)
                            path.lineWidth = strokeWidth
                            let dashOn = strokeWidth * 3.0
                            let dashOff = strokeWidth * 2.0
                            path.setLineDash([dashOn, dashOff], count: 2, phase: 0)
                            path.stroke()
                        } else {
                            cg.setFillColor(muted.cgColor)
                            path.fill()
                            cg.setStrokeColor(UIColor.white.cgColor)
                            path.lineWidth = strokeWidth * 0.8
                            path.stroke()
                        }
                    }
                } else {
                    cg.setFillColor(accent.withAlphaComponent(0.32).cgColor)
                    path.fill()
                    cg.setStrokeColor(accent.cgColor)
                    path.lineWidth = strokeWidth
                    path.stroke()
                }
            }
        }
    }
}

/// Native SwiftUI Image Occlusion review/preview renderer backed by offline `asset:<hash>` storage.
struct AssetOcclusionImageView: View {
    let block: OcclusionBlock
    let store: RemStore
    let activeIndex: UInt32?
    let revealed: Bool
    var onTapAssetImage: ((CachedAssetImage) -> Void)?

    @State private var loaded: CachedAssetImage?
    @State private var didAttemptLoad: Bool = false
    @State private var peekAll: Bool = false

    private var hasInactiveMasks: Bool {
        guard let activeIndex else { return false }
        return block.masks.contains { $0.id != activeIndex }
    }

    private var showPeekToggle: Bool {
        activeIndex != nil && revealed && block.mode == .hideAll && hasInactiveMasks
    }

    var body: some View {
        Group {
            if let loaded {
                let aspect = loaded.image.size.height > 0
                    ? loaded.image.size.width / loaded.image.size.height
                    : 1.0
                let diagramContent = ZStack(alignment: .bottomTrailing) {
                    Image(uiImage: loaded.image)
                        .resizable()
                        .aspectRatio(aspect, contentMode: .fit)
                        .overlay {
                            GeometryReader { geo in
                                maskOverlay(in: geo.size)
                            }
                        }
                        .clipShape(RoundedRectangle(cornerRadius: 11, style: .continuous))
                        .overlay(
                            RoundedRectangle(cornerRadius: 11, style: .continuous)
                                .strokeBorder(RemColor.border, lineWidth: 1)
                        )

                    if onTapAssetImage != nil {
                        Image(systemName: "arrow.up.left.and.arrow.down.right")
                            .font(.system(size: 11, weight: .semibold))
                            .foregroundStyle(Color.white)
                            .padding(6)
                            .background(Color.black.opacity(0.55))
                            .clipShape(Circle())
                            .padding(8)
                    }
                }
                .frame(maxHeight: 340)
                .frame(maxWidth: .infinity)

                VStack(alignment: .leading, spacing: 8) {
                    if let onTapAssetImage {
                        Button {
                            let composite = ImageOcclusionParser.renderCompositeImage(
                                base: loaded.image,
                                block: block,
                                activeIndex: activeIndex,
                                revealed: revealed,
                                peekAll: peekAll
                            )
                            onTapAssetImage(CachedAssetImage(
                                hash: "\(loaded.hash)-\(activeIndex ?? 0)-\(revealed)-\(peekAll)",
                                mime: loaded.mime,
                                byteCount: loaded.byteCount,
                                image: composite
                            ))
                        } label: {
                            diagramContent
                        }
                        .buttonStyle(.plain)
                        .accessibilityLabel(block.alt ?? "Image occlusion diagram. Tap to zoom")
                    } else {
                        diagramContent
                            .accessibilityLabel(block.alt ?? "Image occlusion diagram")
                    }

                    if showPeekToggle {
                        Button {
                            withAnimation(.easeOut(duration: 0.16)) {
                                peekAll.toggle()
                            }
                        } label: {
                            Label(
                                peekAll ? "Hide other masks" : "Show all masks",
                                systemImage: peekAll ? "eye.slash" : "eye"
                            )
                            .font(.caption.weight(.semibold))
                            .foregroundStyle(RemColor.accent)
                            .padding(.horizontal, 10)
                            .padding(.vertical, 5)
                            .background(RemColor.accentTint)
                            .clipShape(Capsule())
                        }
                        .buttonStyle(.plain)
                    }
                }
            } else if didAttemptLoad {
                HStack(spacing: 8) {
                    Image(systemName: "photo.badge.exclamationmark")
                        .foregroundStyle(RemColor.muted)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(block.alt ?? "Diagram not downloaded yet")
                            .font(.system(size: 12.5, weight: .medium))
                            .foregroundStyle(RemColor.text)
                        Text("\(block.image.prefix(18))… · sync online to cache offline")
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
                    .frame(maxWidth: .infinity, minHeight: 96)
                    .background(RemColor.bgInset)
                    .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
            }
        }
        .task(id: block.image) {
            self.loaded = nil
            self.didAttemptLoad = false
            self.peekAll = false
            if let hash = block.assetHash {
                self.loaded = await AssetImageCache.shared.load(hash: hash, store: store)
            }
            self.didAttemptLoad = true
        }
        .onChange(of: activeIndex) { _, _ in peekAll = false }
        .onChange(of: revealed) { _, _ in peekAll = false }
    }

    @ViewBuilder
    private func maskOverlay(in size: CGSize) -> some View {
        ZStack(alignment: .topLeading) {
            ForEach(Array(block.masks.enumerated()), id: \.offset) { _, mask in
                renderMask(mask, in: size)
            }
        }
        .frame(width: size.width, height: size.height, alignment: .topLeading)
    }

    @ViewBuilder
    private func renderMask(_ mask: OcclusionMask, in size: CGSize) -> some View {
        let w = CGFloat(mask.w) * size.width
        let h = CGFloat(mask.h) * size.height
        let cx = (CGFloat(mask.x) + CGFloat(mask.w) / 2.0) * size.width
        let cy = (CGFloat(mask.y) + CGFloat(mask.h) / 2.0) * size.height
        let radius = min(w, h) * 0.08

        if let activeIndex {
            let isActive = mask.id == activeIndex
            if isActive {
                maskShape(type: mask.type, cornerRadius: radius)
                    .fill(revealed ? Color.clear : RemColor.accent)
                    .overlay {
                        maskShape(type: mask.type, cornerRadius: radius)
                            .stroke(revealed ? RemColor.accent : Color.white, lineWidth: revealed ? 2.2 : 1.5)
                    }
                    .overlay {
                        if !revealed && min(w, h) >= 16 {
                            Text("?")
                                .font(.system(size: min(14, min(w, h) * 0.55), weight: .bold))
                                .foregroundStyle(Color.white)
                        }
                    }
                    .frame(width: w, height: h)
                    .position(x: cx, y: cy)
            } else if block.mode == .hideAll {
                maskShape(type: mask.type, cornerRadius: radius)
                    .fill(revealed && peekAll ? Color.clear : RemColor.borderStrong)
                    .overlay {
                        if revealed && peekAll {
                            maskShape(type: mask.type, cornerRadius: radius)
                                .stroke(RemColor.muted.opacity(0.75), style: StrokeStyle(lineWidth: 1.5, dash: [4, 3]))
                        } else {
                            maskShape(type: mask.type, cornerRadius: radius)
                                .stroke(Color.white.opacity(0.85), lineWidth: 1.2)
                        }
                    }
                    .frame(width: w, height: h)
                    .position(x: cx, y: cy)
            }
        } else {
            maskShape(type: mask.type, cornerRadius: radius)
                .fill(RemColor.accent.opacity(0.32))
                .overlay {
                    maskShape(type: mask.type, cornerRadius: radius)
                        .stroke(RemColor.accent, lineWidth: 1.8)
                }
                .overlay {
                    Text("\(mask.id)")
                        .font(.caption2.weight(.bold).monospaced())
                        .foregroundStyle(Color.white)
                        .shadow(color: .black.opacity(0.7), radius: 1)
                }
                .frame(width: w, height: h)
                .position(x: cx, y: cy)
        }
    }

    private func maskShape(type: OcclusionShapeType, cornerRadius: CGFloat) -> AnyShape {
        switch type {
        case .rect:
            return AnyShape(RoundedRectangle(cornerRadius: min(cornerRadius, 6), style: .continuous))
        case .ellipse:
            return AnyShape(Ellipse())
        }
    }
}

/// Native SwiftUI interactive Image Occlusion mask editor presented from `QuickCaptureSheet`.
struct ImageOcclusionEditorSheet: View {
    let store: RemStore
    let initialHeader: String
    let initialBlock: OcclusionBlock
    let onCommit: (String, OcclusionBlock) -> Void

    @Environment(\.dismiss) private var dismiss
    @State private var header: String = ""
    @State private var block: OcclusionBlock
    @State private var tool: OcclusionShapeType = .rect
    @State private var selectedIndices: Set<Int> = []
    @State private var undoStack: [[OcclusionMask]] = []
    @State private var draftRect: CGRect? = nil
    @State private var loaded: CachedAssetImage?
    @State private var replacePhotoItem: PhotosPickerItem?

    init(
        store: RemStore,
        initialHeader: String,
        initialBlock: OcclusionBlock,
        onCommit: @escaping (String, OcclusionBlock) -> Void
    ) {
        self.store = store
        self.initialHeader = initialHeader
        self.initialBlock = initialBlock
        self.onCommit = onCommit
        _header = State(initialValue: initialHeader)
        _block = State(initialValue: initialBlock)
    }

    private var distinctCardCount: Int {
        Set(block.masks.map(\.id)).count
    }

    var body: some View {
        NavigationStack {
            VStack(spacing: 12) {
                TextField("Optional diagram title…", text: $header)
                    .textFieldStyle(.roundedBorder)
                    .padding(.horizontal, 16)
                    .padding(.top, 8)

                HStack(spacing: 8) {
                    Picker("Shape", selection: $tool) {
                        Text("Rect").tag(OcclusionShapeType.rect)
                        Text("Ellipse").tag(OcclusionShapeType.ellipse)
                    }
                    .pickerStyle(.segmented)
                    .frame(maxWidth: 160)

                    Picker("Mode", selection: $block.mode) {
                        Text("Hide all").tag(OcclusionMode.hideAll)
                        Text("Hide one").tag(OcclusionMode.hideOne)
                    }
                    .pickerStyle(.segmented)
                }
                .padding(.horizontal, 16)

                Group {
                    if let loaded {
                        let aspect = loaded.image.size.height > 0
                            ? loaded.image.size.width / loaded.image.size.height
                            : 1.0
                        Image(uiImage: loaded.image)
                            .resizable()
                            .aspectRatio(aspect, contentMode: .fit)
                            .overlay {
                                GeometryReader { geo in
                                    interactiveOverlay(in: geo.size)
                                }
                            }
                            .clipShape(RoundedRectangle(cornerRadius: 11, style: .continuous))
                            .overlay(
                                RoundedRectangle(cornerRadius: 11, style: .continuous)
                                    .strokeBorder(RemColor.border, lineWidth: 1)
                            )
                            .padding(.horizontal, 16)
                    } else {
                        ProgressView()
                            .frame(maxWidth: .infinity, minHeight: 220)
                    }
                }
                .frame(maxHeight: .infinity)

                HStack(spacing: 10) {
                    Button {
                        undoLast()
                    } label: {
                        Label("Undo", systemImage: "arrow.uturn.backward")
                            .font(.caption.weight(.semibold))
                    }
                    .buttonStyle(.bordered)
                    .disabled(undoStack.isEmpty)

                    Button {
                        groupSelected()
                    } label: {
                        Label("Group", systemImage: "rectangle.on.rectangle")
                            .font(.caption.weight(.semibold))
                    }
                    .buttonStyle(.bordered)
                    .disabled(selectedIndices.count < 2)

                    Button(role: .destructive) {
                        deleteSelected()
                    } label: {
                        Label("Delete", systemImage: "trash")
                            .font(.caption.weight(.semibold))
                    }
                    .buttonStyle(.bordered)
                    .disabled(selectedIndices.isEmpty)

                    Spacer()

                    PhotosPicker(selection: $replacePhotoItem, matching: .images) {
                        Image(systemName: "photo.badge.arrow.down")
                            .font(.body)
                    }
                    .accessibilityLabel("Replace diagram image")
                }
                .padding(.horizontal, 16)

                Text(
                    block.masks.isEmpty
                        ? "Drag on the image to draw masks. Tap a mask to select."
                        : "\(block.masks.count) masks · \(distinctCardCount) \(distinctCardCount == 1 ? "card" : "cards") · Tap masks to select or group"
                )
                .font(.caption)
                .foregroundStyle(RemColor.muted)
                .padding(.bottom, 12)
            }
            .background(RemColor.bg)
            .navigationTitle("Image Occlusion")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") {
                        onCommit(header, block)
                        dismiss()
                    }
                    .fontWeight(.semibold)
                    .disabled(block.masks.isEmpty)
                }
            }
        }
        .tint(RemColor.accent)
        .task(id: block.image) {
            if let hash = block.assetHash {
                self.loaded = await AssetImageCache.shared.load(hash: hash, store: store)
            }
        }
        .onChange(of: replacePhotoItem) { _, item in
            guard let item else { return }
            Task { await replacePhoto(item) }
        }
    }

    @ViewBuilder
    private func interactiveOverlay(in size: CGSize) -> some View {
        ZStack(alignment: .topLeading) {
            ForEach(Array(block.masks.enumerated()), id: \.offset) { idx, mask in
                let isSelected = selectedIndices.contains(idx)
                let w = CGFloat(mask.w) * size.width
                let h = CGFloat(mask.h) * size.height
                let cx = (CGFloat(mask.x) + CGFloat(mask.w) / 2.0) * size.width
                let cy = (CGFloat(mask.y) + CGFloat(mask.h) / 2.0) * size.height
                let shape: AnyShape = mask.type == .ellipse
                    ? AnyShape(Ellipse())
                    : AnyShape(RoundedRectangle(cornerRadius: min(min(w, h) * 0.08, 6), style: .continuous))

                shape
                    .fill(RemColor.accent.opacity(isSelected ? 0.55 : 0.36))
                    .overlay {
                        shape.stroke(isSelected ? RemColor.text : RemColor.accent, lineWidth: isSelected ? 2.5 : 1.8)
                    }
                    .overlay {
                        Text("\(mask.id)")
                            .font(.caption2.weight(.bold).monospaced())
                            .foregroundStyle(Color.white)
                            .shadow(color: .black.opacity(0.75), radius: 1)
                    }
                    .frame(width: w, height: h)
                    .position(x: cx, y: cy)
                    .allowsHitTesting(false)
            }

            if let rect = draftRect {
                let w = rect.width * size.width
                let h = rect.height * size.height
                let cx = (rect.origin.x + rect.width / 2.0) * size.width
                let cy = (rect.origin.y + rect.height / 2.0) * size.height
                let shape: AnyShape = tool == .ellipse
                    ? AnyShape(Ellipse())
                    : AnyShape(Rectangle())
                shape
                    .fill(RemColor.accent.opacity(0.22))
                    .overlay {
                        shape.stroke(RemColor.accent, style: StrokeStyle(lineWidth: 2, dash: [5, 4]))
                    }
                    .frame(width: w, height: h)
                    .position(x: cx, y: cy)
                    .allowsHitTesting(false)
            }

            // Single unified gesture layer so dragging can start over existing masks AND
            // tapping selects/toggles masks or clears selection on empty space.
            Color.clear
                .contentShape(Rectangle())
                .gesture(
                    DragGesture(minimumDistance: 0, coordinateSpace: .local)
                        .onChanged { value in
                            guard size.width > 0, size.height > 0 else { return }
                            let dist = hypot(value.translation.width, value.translation.height)
                            guard dist >= 8 else { return }
                            let x1 = max(0, min(size.width, value.startLocation.x))
                            let y1 = max(0, min(size.height, value.startLocation.y))
                            let x2 = max(0, min(size.width, value.location.x))
                            let y2 = max(0, min(size.height, value.location.y))
                            draftRect = CGRect(
                                x: min(x1, x2) / size.width,
                                y: min(y1, y2) / size.height,
                                width: abs(x2 - x1) / size.width,
                                height: abs(y2 - y1) / size.height
                            )
                        }
                        .onEnded { value in
                            let dist = hypot(value.translation.width, value.translation.height)
                            if dist < 8 {
                                draftRect = nil
                                handleCanvasTap(at: value.location, in: size)
                                return
                            }
                            defer { draftRect = nil }
                            guard let rect = draftRect,
                                  rect.width >= 0.02,
                                  rect.height >= 0.02,
                                  block.masks.count < ImageOcclusionParser.maxMasks
                            else { return }
                            let newMask = OcclusionMask(
                                id: ImageOcclusionParser.nextMaskId(in: block.masks),
                                type: tool,
                                x: rect.origin.x,
                                y: rect.origin.y,
                                w: rect.width,
                                h: rect.height
                            )
                            if let sanitized = ImageOcclusionParser.sanitizeMask(newMask) {
                                pushUndo()
                                block.masks.append(sanitized)
                                selectedIndices = [block.masks.count - 1]
                            }
                        }
                )
        }
        .frame(width: size.width, height: size.height, alignment: .topLeading)
    }

    private func handleCanvasTap(at point: CGPoint, in size: CGSize) {
        guard size.width > 0, size.height > 0 else { return }
        let nx = Double(point.x / size.width)
        let ny = Double(point.y / size.height)
        for idx in block.masks.indices.reversed() {
            let m = block.masks[idx]
            if nx >= m.x && nx <= m.x + m.w && ny >= m.y && ny <= m.y + m.h {
                if selectedIndices.contains(idx) {
                    selectedIndices.remove(idx)
                } else {
                    selectedIndices.insert(idx)
                }
                return
            }
        }
        selectedIndices.removeAll()
    }

    private func pushUndo() {
        undoStack.append(block.masks)
        if undoStack.count > 40 {
            undoStack.removeFirst()
        }
    }

    private func undoLast() {
        guard let previous = undoStack.popLast() else { return }
        block.masks = previous
        selectedIndices.removeAll()
    }

    private func groupSelected() {
        guard selectedIndices.count >= 2 else { return }
        let targetId = selectedIndices.compactMap { idx in
            idx < block.masks.count ? block.masks[idx].id : nil
        }.min() ?? 1
        pushUndo()
        for idx in selectedIndices where idx < block.masks.count {
            block.masks[idx].id = targetId
        }
    }

    private func deleteSelected() {
        guard !selectedIndices.isEmpty else { return }
        pushUndo()
        block.masks = block.masks.enumerated()
            .filter { !selectedIndices.contains($0.offset) }
            .map(\.element)
        selectedIndices.removeAll()
    }

    private func replacePhoto(_ item: PhotosPickerItem) async {
        defer { replacePhotoItem = nil }
        guard let data = try? await item.loadTransferable(type: Data.self) else { return }
        let mime = item.supportedContentTypes.first?.preferredMIMEType ?? "image/jpeg"
        if let hash = try? await store.putAsset(mime: mime, bytes: data) {
            block.image = "asset:\(hash)"
        }
    }
}
