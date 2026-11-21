import Foundation
import ImageIO
import UIKit

final class CachedAssetImage: Identifiable {
    let hash: String
    let mime: String
    let byteCount: Int
    let image: UIImage

    var id: String { hash }

    init(hash: String, mime: String, byteCount: Int, image: UIImage) {
        self.hash = hash
        self.mime = mime
        self.byteCount = byteCount
        self.image = image
    }
}

/// Thread-safe in-memory cache of decoded `asset:<hash>` images/GIFs loaded from `RemStore`.
final class AssetImageCache: @unchecked Sendable {
    static let shared = AssetImageCache()

    private let cache = NSCache<NSString, CachedAssetImage>()

    private init() {
        cache.countLimit = 80
        cache.totalCostLimit = 48 * 1024 * 1024
    }

    func cached(for hash: String) -> CachedAssetImage? {
        cache.object(forKey: hash as NSString)
    }

    func load(hash: String, store: RemStore) async -> CachedAssetImage? {
        if let existing = cached(for: hash) {
            return existing
        }
        guard let asset = try? await store.getAsset(hash: hash) else {
            return nil
        }
        guard let decoded = Self.decodeImage(data: asset.bytes, mime: asset.mime) else {
            return nil
        }
        let wrapped = CachedAssetImage(
            hash: asset.hash,
            mime: asset.mime,
            byteCount: asset.bytes.count,
            image: decoded
        )
        cache.setObject(wrapped, forKey: hash as NSString, cost: asset.bytes.count)
        return wrapped
    }

    private static func decodeImage(data: Data, mime: String) -> UIImage? {
        if mime.lowercased().contains("gif") {
            if let animated = decodeAnimatedGif(data: data) {
                return animated
            }
        }
        return UIImage(data: data)
    }

    private static func decodeAnimatedGif(data: Data) -> UIImage? {
        guard let source = CGImageSourceCreateWithData(data as CFData, nil) else { return nil }
        let frameCount = CGImageSourceGetCount(source)
        guard frameCount > 1 else { return UIImage(data: data) }

        var frames: [UIImage] = []
        var totalDuration: Double = 0

        for idx in 0..<frameCount {
            guard let cgImage = CGImageSourceCreateImageAtIndex(source, idx, nil) else { continue }
            let delay = gifFrameDelay(source: source, index: idx)
            totalDuration += delay
            frames.append(UIImage(cgImage: cgImage))
        }
        guard !frames.isEmpty else { return nil }
        return UIImage.animatedImage(with: frames, duration: max(0.1, totalDuration))
    }

    private static func gifFrameDelay(source: CGImageSource, index: Int) -> Double {
        guard let properties = CGImageSourceCopyPropertiesAtIndex(source, index, nil) as? [CFString: Any],
              let gifDict = properties[kCGImagePropertyGIFDictionary] as? [CFString: Any]
        else { return 0.1 }

        let raw = (gifDict[kCGImagePropertyGIFUnclampedDelayTime] as? Double)
            ?? (gifDict[kCGImagePropertyGIFDelayTime] as? Double)
            ?? 0.1
        return raw < 0.02 ? 0.1 : raw
    }
}
