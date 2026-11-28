import SwiftUI
import UIKit

enum MarkdownFormatAction {
    case bold
    case italic
    case inlineCode
    case codeBlock
    case bulletList
    case cloze(index: UInt32)
}

struct MarkdownFormatResult: Equatable {
    let text: String
    let selectedRange: NSRange
}

/// Pure selection-aware Markdown formatter used by `QuickCaptureSheet`'s morphing keyboard bar.
enum MarkdownFormatter {
    static func apply(
        _ action: MarkdownFormatAction,
        to text: String,
        selectedRange: NSRange
    ) -> MarkdownFormatResult {
        let ns = text as NSString
        let clampedLoc = max(0, min(selectedRange.location, ns.length))
        let clampedLen = max(0, min(selectedRange.length, ns.length - clampedLoc))
        let safeRange = NSRange(location: clampedLoc, length: clampedLen)
        let selected = ns.substring(with: safeRange)
        let hasSelection = clampedLen > 0

        let replacement: String
        let nextRange: NSRange

        switch action {
        case .bold:
            let inner = hasSelection ? selected : "text"
            replacement = "**\(inner)**"
            nextRange = hasSelection
                ? NSRange(location: clampedLoc + (replacement as NSString).length, length: 0)
                : NSRange(location: clampedLoc + 2, length: (inner as NSString).length)

        case .italic:
            let inner = hasSelection ? selected : "text"
            replacement = "*\(inner)*"
            nextRange = hasSelection
                ? NSRange(location: clampedLoc + (replacement as NSString).length, length: 0)
                : NSRange(location: clampedLoc + 1, length: (inner as NSString).length)

        case .inlineCode:
            let inner = hasSelection ? selected : "code"
            replacement = "`\(inner)`"
            nextRange = hasSelection
                ? NSRange(location: clampedLoc + (replacement as NSString).length, length: 0)
                : NSRange(location: clampedLoc + 1, length: (inner as NSString).length)

        case .codeBlock:
            let needsLeadingNewline = clampedLoc > 0
                && ns.substring(with: NSRange(location: clampedLoc - 1, length: 1)) != "\n"
            let prefix = needsLeadingNewline ? "\n```\n" : "```\n"
            replacement = "\(prefix)\(selected)\n```\n"
            let cursorLoc = clampedLoc + (prefix as NSString).length + (selected as NSString).length
            nextRange = NSRange(location: cursorLoc, length: 0)

        case .bulletList:
            if hasSelection {
                let lines = selected.components(separatedBy: "\n")
                let bulleted = lines.map { line -> String in
                    let trimmed = line.trimmingCharacters(in: .whitespaces)
                    if trimmed.hasPrefix("- ") { return line }
                    return "- \(line)"
                }.joined(separator: "\n")
                let needsLeadingNewline = clampedLoc > 0
                    && ns.substring(with: NSRange(location: clampedLoc - 1, length: 1)) != "\n"
                replacement = (needsLeadingNewline ? "\n" : "") + bulleted
                nextRange = NSRange(location: clampedLoc + (replacement as NSString).length, length: 0)
            } else {
                let needsLeadingNewline = clampedLoc > 0
                    && ns.substring(with: NSRange(location: clampedLoc - 1, length: 1)) != "\n"
                replacement = (needsLeadingNewline ? "\n" : "") + "- "
                nextRange = NSRange(location: clampedLoc + (replacement as NSString).length, length: 0)
            }

        case .cloze(let index):
            let prefix = "{{c\(index)::"
            let inner = hasSelection ? selected : "answer"
            replacement = "\(prefix)\(inner)}}"
            nextRange = hasSelection
                ? NSRange(location: clampedLoc + (replacement as NSString).length, length: 0)
                : NSRange(location: clampedLoc + (prefix as NSString).length, length: (inner as NSString).length)
        }

        let updated = ns.replacingCharacters(in: safeRange, with: replacement)
        return MarkdownFormatResult(text: updated, selectedRange: nextRange)
    }

    /// Removes `![](asset:<hash>)`, `![...](asset:<hash>)`, and `[...](asset:<hash>)` lines cleanly.
    static func removeAssetReference(hash: String, from text: String) -> String {
        let pattern = "\\n?!?\\[[^\\]]*\\]\\(asset:\(NSRegularExpression.escapedPattern(for: hash))\\)\\n?"
        guard let regex = try? NSRegularExpression(pattern: pattern) else { return text }
        let range = NSRange(location: 0, length: (text as NSString).length)
        let stripped = regex.stringByReplacingMatches(in: text, range: range, withTemplate: "\n")
        return stripped.trimmingCharacters(in: .newlines)
    }
}

/// Bridge allowing `QuickCaptureSheet` to focus/resign or format the active `UITextView` directly.
@MainActor
final class MarkdownFieldController {
    weak var textView: UITextView?

    func focus() {
        textView?.becomeFirstResponder()
    }

    func blur() {
        textView?.resignFirstResponder()
    }

    func apply(_ action: MarkdownFormatAction, binding: Binding<String>) {
        guard let tv = textView else {
            let result = MarkdownFormatter.apply(
                action,
                to: binding.wrappedValue,
                selectedRange: NSRange(location: (binding.wrappedValue as NSString).length, length: 0)
            )
            binding.wrappedValue = result.text
            return
        }
        let current = tv.text ?? ""
        let result = MarkdownFormatter.apply(action, to: current, selectedRange: tv.selectedRange)
        tv.text = result.text
        tv.selectedRange = result.selectedRange
        binding.wrappedValue = result.text
        tv.invalidateIntrinsicContentSize()
    }
}

/// Auto-sizing native `UITextView` with placeholder, live selection tracking, and zero scrolling conflict inside SwiftUI `ScrollView`.
struct AutoSizingMarkdownTextView: UIViewRepresentable {
    @Binding var text: String
    let placeholder: String
    let font: UIFont
    let minHeight: CGFloat
    let controller: MarkdownFieldController
    let onFocusChange: (Bool) -> Void
    let onSelectionChange: (NSRange) -> Void

    func makeCoordinator() -> Coordinator {
        Coordinator(parent: self)
    }

    func makeUIView(context: Context) -> PlaceholderTextView {
        let tv = PlaceholderTextView()
        tv.delegate = context.coordinator
        tv.backgroundColor = .clear
        tv.isScrollEnabled = false
        tv.textContainerInset = .zero
        tv.textContainer.lineFragmentPadding = 0
        tv.font = font
        tv.textColor = UIColor(RemColor.text)
        tv.tintColor = UIColor(RemColor.accent)
        tv.placeholder = placeholder
        tv.placeholderColor = UIColor(RemColor.faint)
        tv.minHeight = minHeight
        tv.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        tv.setContentHuggingPriority(.defaultHigh, for: .vertical)
        tv.text = text
        tv.updatePlaceholderVisibility()
        controller.textView = tv
        return tv
    }

    func updateUIView(_ uiView: PlaceholderTextView, context: Context) {
        context.coordinator.parent = self
        controller.textView = uiView
        if uiView.text != text {
            let previousSelection = uiView.selectedRange
            uiView.text = text
            let len = (text as NSString).length
            let clamped = NSRange(
                location: min(previousSelection.location, len),
                length: min(previousSelection.length, max(0, len - min(previousSelection.location, len)))
            )
            uiView.selectedRange = clamped
            uiView.invalidateIntrinsicContentSize()
        }
        uiView.font = font
        uiView.textColor = UIColor(RemColor.text)
        uiView.tintColor = UIColor(RemColor.accent)
        uiView.placeholder = placeholder
        uiView.placeholderColor = UIColor(RemColor.faint)
        uiView.minHeight = minHeight
        uiView.updatePlaceholderVisibility()
    }

    func sizeThatFits(_ proposal: ProposedViewSize, uiView: PlaceholderTextView, context: Context) -> CGSize? {
        let width = proposal.width ?? UIScreen.main.bounds.width - 40
        let target = CGSize(width: max(1, width), height: .greatestFiniteMagnitude)
        let measured = uiView.sizeThatFits(target)
        return CGSize(width: width, height: max(minHeight, ceil(measured.height)))
    }

    final class Coordinator: NSObject, UITextViewDelegate {
        var parent: AutoSizingMarkdownTextView

        init(parent: AutoSizingMarkdownTextView) {
            self.parent = parent
        }

        func textViewDidBeginEditing(_ textView: UITextView) {
            parent.onFocusChange(true)
            parent.onSelectionChange(textView.selectedRange)
        }

        func textViewDidEndEditing(_ textView: UITextView) {
            parent.onFocusChange(false)
            parent.onSelectionChange(NSRange(location: 0, length: 0))
        }

        func textViewDidChange(_ textView: UITextView) {
            if let ptv = textView as? PlaceholderTextView {
                ptv.updatePlaceholderVisibility()
            }
            let updated = textView.text ?? ""
            if parent.text != updated {
                parent.text = updated
            }
            textView.invalidateIntrinsicContentSize()
        }

        func textViewDidChangeSelection(_ textView: UITextView) {
            parent.onSelectionChange(textView.selectedRange)
        }
    }
}

final class PlaceholderTextView: UITextView {
    private let placeholderLabel = UILabel()
    var minHeight: CGFloat = 44

    var placeholder: String = "" {
        didSet {
            placeholderLabel.text = placeholder
            setNeedsLayout()
        }
    }

    var placeholderColor: UIColor = .placeholderText {
        didSet {
            placeholderLabel.textColor = placeholderColor
        }
    }

    override var font: UIFont? {
        didSet {
            placeholderLabel.font = font
            setNeedsLayout()
        }
    }

    override init(frame: CGRect, textContainer: NSTextContainer?) {
        super.init(frame: frame, textContainer: textContainer)
        placeholderLabel.numberOfLines = 0
        placeholderLabel.isUserInteractionEnabled = false
        addSubview(placeholderLabel)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        let width = max(0, bounds.width - textContainerInset.left - textContainerInset.right)
        let size = placeholderLabel.sizeThatFits(CGSize(width: width, height: .greatestFiniteMagnitude))
        placeholderLabel.frame = CGRect(
            x: textContainerInset.left,
            y: textContainerInset.top,
            width: width,
            height: size.height
        )
    }

    func updatePlaceholderVisibility() {
        placeholderLabel.isHidden = !(text ?? "").isEmpty
    }
}
