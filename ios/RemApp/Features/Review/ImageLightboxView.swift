import SwiftUI
import UIKit

/// Native `UIScrollView`-backed pinch-to-zoom full-screen lightbox for offline `asset:<hash>` images and diagrams.
struct ImageLightboxView: View {
    let asset: CachedAssetImage
    @Environment(\.dismiss) private var dismiss
    @State private var dragDismissOffset: CGFloat = 0

    var body: some View {
        NavigationStack {
            ZStack {
                Color.black
                    .opacity(max(0.35, 1.0 - Double(abs(dragDismissOffset)) / 360.0))
                    .ignoresSafeArea()

                ZoomableImageScrollView(
                    image: asset.image,
                    onSwipeDismissChanged: { dy in
                        dragDismissOffset = dy
                    },
                    onSwipeDismissEnded: { shouldDismiss in
                        if shouldDismiss {
                            dismiss()
                        } else {
                            withAnimation(.spring(response: 0.25, dampingFraction: 0.85)) {
                                dragDismissOffset = 0
                            }
                        }
                    }
                )
                .offset(y: dragDismissOffset)
                .ignoresSafeArea()
            }
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .principal) {
                    Text(formattedBytes(asset.byteCount))
                        .font(.caption.monospaced())
                        .foregroundStyle(Color.white.opacity(0.75))
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") {
                        dismiss()
                    }
                    .fontWeight(.semibold)
                }
            }
            .toolbarColorScheme(.dark, for: .navigationBar)
            .toolbarBackground(.hidden, for: .navigationBar)
        }
        .preferredColorScheme(.dark)
    }

    private func formattedBytes(_ bytes: Int) -> String {
        if bytes < 1024 { return "\(bytes) B" }
        if bytes < 1024 * 1024 { return "\(bytes / 1024) KB" }
        return String(format: "%.1f MB", Double(bytes) / 1_048_576.0)
    }
}

private struct ZoomableImageScrollView: UIViewRepresentable {
    let image: UIImage
    let onSwipeDismissChanged: (CGFloat) -> Void
    let onSwipeDismissEnded: (Bool) -> Void

    func makeCoordinator() -> Coordinator {
        Coordinator(
            onSwipeDismissChanged: onSwipeDismissChanged,
            onSwipeDismissEnded: onSwipeDismissEnded
        )
    }

    func makeUIView(context: Context) -> UIScrollView {
        let scrollView = UIScrollView()
        scrollView.delegate = context.coordinator
        scrollView.minimumZoomScale = 1.0
        scrollView.maximumZoomScale = 4.0
        scrollView.bouncesZoom = true
        scrollView.showsHorizontalScrollIndicator = false
        scrollView.showsVerticalScrollIndicator = false
        scrollView.backgroundColor = .clear
        scrollView.contentInsetAdjustmentBehavior = .never

        let imageView = UIImageView(image: image)
        imageView.contentMode = .scaleAspectFit
        imageView.translatesAutoresizingMaskIntoConstraints = false
        scrollView.addSubview(imageView)

        NSLayoutConstraint.activate([
            imageView.widthAnchor.constraint(equalTo: scrollView.frameLayoutGuide.widthAnchor),
            imageView.heightAnchor.constraint(equalTo: scrollView.frameLayoutGuide.heightAnchor),
            imageView.leadingAnchor.constraint(equalTo: scrollView.contentLayoutGuide.leadingAnchor),
            imageView.trailingAnchor.constraint(equalTo: scrollView.contentLayoutGuide.trailingAnchor),
            imageView.topAnchor.constraint(equalTo: scrollView.contentLayoutGuide.topAnchor),
            imageView.bottomAnchor.constraint(equalTo: scrollView.contentLayoutGuide.bottomAnchor),
        ])

        context.coordinator.imageView = imageView
        context.coordinator.scrollView = scrollView

        let doubleTap = UITapGestureRecognizer(target: context.coordinator, action: #selector(Coordinator.handleDoubleTap(_:)))
        doubleTap.numberOfTapsRequired = 2
        scrollView.addGestureRecognizer(doubleTap)

        let panToDismiss = UIPanGestureRecognizer(target: context.coordinator, action: #selector(Coordinator.handleDismissPan(_:)))
        panToDismiss.delegate = context.coordinator
        scrollView.addGestureRecognizer(panToDismiss)

        return scrollView
    }

    func updateUIView(_ uiView: UIScrollView, context: Context) {
        context.coordinator.imageView?.image = image
    }

    final class Coordinator: NSObject, UIScrollViewDelegate, UIGestureRecognizerDelegate {
        weak var scrollView: UIScrollView?
        weak var imageView: UIImageView?
        let onSwipeDismissChanged: (CGFloat) -> Void
        let onSwipeDismissEnded: (Bool) -> Void

        init(
            onSwipeDismissChanged: @escaping (CGFloat) -> Void,
            onSwipeDismissEnded: @escaping (Bool) -> Void
        ) {
            self.onSwipeDismissChanged = onSwipeDismissChanged
            self.onSwipeDismissEnded = onSwipeDismissEnded
        }

        func viewForZooming(in scrollView: UIScrollView) -> UIView? {
            imageView
        }

        @objc func handleDoubleTap(_ gesture: UITapGestureRecognizer) {
            guard let scrollView, let imageView else { return }
            if scrollView.zoomScale > scrollView.minimumZoomScale + 0.05 {
                scrollView.setZoomScale(scrollView.minimumZoomScale, animated: true)
            } else {
                let targetScale: CGFloat = 2.5
                let point = gesture.location(in: imageView)
                let width = scrollView.bounds.width / targetScale
                let height = scrollView.bounds.height / targetScale
                let rect = CGRect(
                    x: point.x - width / 2,
                    y: point.y - height / 2,
                    width: width,
                    height: height
                )
                scrollView.zoom(to: rect, animated: true)
            }
        }

        @objc func handleDismissPan(_ gesture: UIPanGestureRecognizer) {
            guard let scrollView, scrollView.zoomScale <= scrollView.minimumZoomScale + 0.01 else { return }
            let translation = gesture.translation(in: scrollView)
            let velocity = gesture.velocity(in: scrollView)
            switch gesture.state {
            case .changed:
                onSwipeDismissChanged(max(0, translation.y))
            case .ended, .cancelled:
                let shouldDismiss = translation.y > 110 || velocity.y > 750
                onSwipeDismissEnded(shouldDismiss)
            default:
                break
            }
        }

        func gestureRecognizerShouldBegin(_ gestureRecognizer: UIGestureRecognizer) -> Bool {
            guard let pan = gestureRecognizer as? UIPanGestureRecognizer,
                  let scrollView else { return true }
            guard scrollView.zoomScale <= scrollView.minimumZoomScale + 0.01 else { return false }
            let velocity = pan.velocity(in: scrollView)
            return velocity.y > 0 && abs(velocity.y) > abs(velocity.x)
        }
    }
}
