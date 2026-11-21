import SwiftUI

/// Pinch-to-zoom full-screen lightbox for offline `asset:<hash>` images and diagrams.
struct ImageLightboxView: View {
    let asset: CachedAssetImage
    @Environment(\.dismiss) private var dismiss

    @State private var scale: CGFloat = 1.0
    @State private var lastScale: CGFloat = 1.0
    @State private var offset: CGSize = .zero
    @State private var lastOffset: CGSize = .zero

    var body: some View {
        ZStack {
            Color.black.ignoresSafeArea()

            Image(uiImage: asset.image)
                .resizable()
                .scaledToFit()
                .scaleEffect(scale)
                .offset(offset)
                .gesture(
                    MagnificationGesture()
                        .onChanged { value in
                            scale = min(4.0, max(1.0, lastScale * value))
                        }
                        .onEnded { _ in
                            lastScale = scale
                            if scale <= 1.01 {
                                withAnimation(.spring(response: 0.25)) {
                                    scale = 1.0
                                    lastScale = 1.0
                                    offset = .zero
                                    lastOffset = .zero
                                }
                            }
                        }
                )
                .simultaneousGesture(
                    DragGesture()
                        .onChanged { value in
                            guard scale > 1.01 else { return }
                            offset = CGSize(
                                width: lastOffset.width + value.translation.width,
                                height: lastOffset.height + value.translation.height
                            )
                        }
                        .onEnded { _ in
                            lastOffset = offset
                        }
                )
                .onTapGesture(count: 2) {
                    withAnimation(.spring(response: 0.28)) {
                        if scale > 1.05 {
                            scale = 1.0
                            lastScale = 1.0
                            offset = .zero
                            lastOffset = .zero
                        } else {
                            scale = 2.4
                            lastScale = 2.4
                        }
                    }
                }

            VStack {
                HStack {
                    Text("asset:\(asset.hash.prefix(12))… · \(formattedBytes(asset.byteCount))")
                        .font(.system(size: 11.5, design: .monospaced))
                        .foregroundStyle(Color.white.opacity(0.75))
                        .padding(.horizontal, 10)
                        .padding(.vertical, 6)
                        .background(Color.white.opacity(0.12))
                        .clipShape(Capsule())

                    Spacer()

                    Button("Done") {
                        dismiss()
                    }
                    .font(.system(size: 14, weight: .semibold))
                    .foregroundStyle(Color.white)
                    .padding(.horizontal, 14)
                    .padding(.vertical, 7)
                    .background(Color.white.opacity(0.16))
                    .clipShape(Capsule())
                }
                .padding(.horizontal, 16)
                .padding(.top, 12)

                Spacer()

                Text("Pinch or double-tap to zoom")
                    .font(.system(size: 12))
                    .foregroundStyle(Color.white.opacity(0.55))
                    .padding(.bottom, 20)
            }
        }
    }

    private func formattedBytes(_ bytes: Int) -> String {
        if bytes < 1024 { return "\(bytes) B" }
        if bytes < 1024 * 1024 { return "\(bytes / 1024) KB" }
        return String(format: "%.1f MB", Double(bytes) / 1_048_576.0)
    }
}
