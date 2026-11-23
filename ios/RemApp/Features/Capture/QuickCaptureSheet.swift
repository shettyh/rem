import PhotosUI
import SwiftUI
import UIKit

/// Unified paper-canvas card editor with an in-place morphing keyboard accessory bar (Concept A).
struct QuickCaptureSheet: View {
    @Bindable var appState: AppState
    let initialDeckId: String?
    let editingCard: CardDto?
    let onSaved: (() -> Void)?

    @Environment(\.dismiss) private var dismiss

    @State private var selectedDeckId: String = ""
    @State private var front: String = ""
    @State private var back: String = ""
    @State private var tags: [String] = []
    @State private var newTagText: String = ""
    @State private var isPreview: Bool = false
    @State private var isSaving: Bool = false
    @State private var showDeleteConfirm: Bool = false
    @State private var selectedPhotoItem: PhotosPickerItem?
    @State private var attachedAssetHashes: [String] = []

    @State private var activeEditorField: ActiveEditorField? = nil
    @State private var hasTextSelection: Bool = false
    @State private var manualFormatMode: Bool = false
    @FocusState private var isTagFieldFocused: Bool

    @State private var frontController = MarkdownFieldController()
    @State private var backController = MarkdownFieldController()

    enum ActiveEditorField {
        case front
        case back
    }

    private var isEditing: Bool { editingCard != nil }

    private var canSave: Bool {
        !front.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            && !selectedDeckId.isEmpty
            && !isSaving
    }

    private var selectedDeck: DeckDto? {
        appState.decks.first { $0.id == selectedDeckId }
    }

    private var showKeyboardBar: Bool {
        !isPreview && (activeEditorField != nil || isTagFieldFocused)
    }

    private var isFormatBarMode: Bool {
        activeEditorField != nil && (manualFormatMode || hasTextSelection)
    }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 0) {
                    if isPreview {
                        previewCanvas
                    } else {
                        paperCanvas
                    }
                }
                .padding(.horizontal, 20)
                .padding(.top, 8)
                .padding(.bottom, 28)
            }
            .scrollDismissesKeyboard(.interactively)
            .background(RemColor.bg)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") {
                        dismissKeyboard()
                        dismiss()
                    }
                }
                ToolbarItem(placement: .principal) {
                    deckPickerPill
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button(isSaving ? "Saving…" : "Save") {
                        Task { await save() }
                    }
                    .fontWeight(.semibold)
                    .disabled(!canSave)
                }
            }
            .safeAreaInset(edge: .bottom, spacing: 0) {
                if showKeyboardBar {
                    morphingKeyboardBar
                        .transition(.move(edge: .bottom).combined(with: .opacity))
                }
            }
            .animation(.easeOut(duration: 0.16), value: showKeyboardBar)
            .animation(.easeOut(duration: 0.15), value: isFormatBarMode)
        }
        .tint(RemColor.accent)
        .remToastOverlay(appState: appState)
        .onAppear {
            if let card = editingCard {
                selectedDeckId = card.deckId
                front = card.front
                back = card.back
                tags = card.tags
                attachedAssetHashes = card.assetHashes
            } else {
                selectedDeckId = initialDeckId ?? appState.decks.first?.id ?? ""
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.12) {
                    frontController.focus()
                }
            }
        }
        .onChange(of: selectedPhotoItem) { _, item in
            guard let item else { return }
            Task { await attachPhoto(item) }
        }
        .alert("Delete card?", isPresented: $showDeleteConfirm) {
            Button("Delete", role: .destructive) {
                Task { await deleteCard() }
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("This permanently deletes the card and its review history.")
        }
    }

    // MARK: - Header

    @ViewBuilder
    private var deckPickerPill: some View {
        if isEditing, let deck = selectedDeck {
            HStack(spacing: 6) {
                Circle()
                    .fill(Color(hexString: deck.color))
                    .frame(width: 7, height: 7)
                Text(deck.name)
                    .font(.system(size: 13, weight: .semibold))
                    .foregroundStyle(RemColor.text)
                    .lineLimit(1)
            }
            .padding(.horizontal, 11)
            .padding(.vertical, 5.5)
            .background(RemColor.bgInset)
            .clipShape(Capsule())
            .overlay(Capsule().strokeBorder(RemColor.border, lineWidth: 1))
        } else {
            Menu {
                ForEach(appState.decks, id: \.id) { deck in
                    Button {
                        selectedDeckId = deck.id
                    } label: {
                        Label(deck.name, systemImage: deck.id == selectedDeckId ? "checkmark" : "")
                    }
                }
            } label: {
                HStack(spacing: 6) {
                    Circle()
                        .fill(Color(hexString: selectedDeck?.color ?? "#5a47e8"))
                        .frame(width: 7, height: 7)
                    Text(selectedDeck?.name ?? "Select deck")
                        .font(.system(size: 13, weight: .semibold))
                        .foregroundStyle(RemColor.text)
                        .lineLimit(1)
                    Image(systemName: "chevron.down")
                        .font(.system(size: 9.5, weight: .bold))
                        .foregroundStyle(RemColor.muted)
                }
                .padding(.horizontal, 11)
                .padding(.vertical, 5.5)
                .background(RemColor.bgInset)
                .clipShape(Capsule())
                .overlay(Capsule().strokeBorder(RemColor.border, lineWidth: 1))
            }
        }
    }

    // MARK: - Unified Paper Canvas

    private var paperCanvas: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text("QUESTION")
                    .font(.system(size: 10.5, weight: .bold))
                    .tracking(0.8)
                    .foregroundStyle(RemColor.faint)
                Spacer()
                previewTogglePill
            }
            .padding(.bottom, 8)

            AutoSizingMarkdownTextView(
                text: $front,
                placeholder: "What do you want to remember?",
                font: .systemFont(ofSize: 19, weight: .semibold),
                minHeight: 54,
                controller: frontController,
                onFocusChange: { focused in
                    if focused {
                        activeEditorField = .front
                    } else if activeEditorField == .front {
                        activeEditorField = nil
                        hasTextSelection = false
                        manualFormatMode = false
                    }
                },
                onSelectionChange: { range in
                    if activeEditorField == .front {
                        hasTextSelection = range.length > 0
                    }
                }
            )

            Divider()
                .overlay(RemColor.border)
                .padding(.vertical, 16)

            Text("ANSWER")
                .font(.system(size: 10.5, weight: .bold))
                .tracking(0.8)
                .foregroundStyle(RemColor.faint)
                .padding(.bottom, 8)

            AutoSizingMarkdownTextView(
                text: $back,
                placeholder: "Write a concise recall answer…",
                font: .systemFont(ofSize: 15.5, weight: .regular),
                minHeight: 110,
                controller: backController,
                onFocusChange: { focused in
                    if focused {
                        activeEditorField = .back
                    } else if activeEditorField == .back {
                        activeEditorField = nil
                        hasTextSelection = false
                        manualFormatMode = false
                    }
                },
                onSelectionChange: { range in
                    if activeEditorField == .back {
                        hasTextSelection = range.length > 0
                    }
                }
            )

            if !attachedAssetHashes.isEmpty {
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 10) {
                        ForEach(attachedAssetHashes, id: \.self) { hash in
                            CaptureAssetThumbnail(hash: hash, store: appState.store) {
                                removeAttachment(hash: hash)
                            }
                        }
                    }
                    .padding(.vertical, 2)
                }
                .padding(.top, 14)
            }

            tagsRow
                .padding(.top, 18)

            if isEditing {
                Button(role: .destructive) {
                    showDeleteConfirm = true
                } label: {
                    Text("Delete card")
                        .font(.system(size: 14, weight: .semibold))
                        .foregroundStyle(RemColor.danger)
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 12)
                }
                .padding(.top, 20)
            }
        }
    }

    private var previewTogglePill: some View {
        Button {
            withAnimation(.easeOut(duration: 0.14)) {
                isPreview.toggle()
            }
            if isPreview {
                dismissKeyboard()
            }
        } label: {
            HStack(spacing: 4) {
                Image(systemName: isPreview ? "pencil" : "eye")
                    .font(.system(size: 11, weight: .semibold))
                Text(isPreview ? "Edit" : "Preview")
                    .font(.system(size: 12, weight: .semibold))
            }
            .foregroundStyle(isPreview ? RemColor.accent : RemColor.muted)
            .padding(.horizontal, 10)
            .padding(.vertical, 4.5)
            .background(isPreview ? RemColor.accentTint : RemColor.surface)
            .clipShape(Capsule())
            .overlay(
                Capsule()
                    .strokeBorder(isPreview ? Color.clear : RemColor.border, lineWidth: 1)
            )
        }
        .buttonStyle(.plain)
    }

    private var tagsRow: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 6) {
                ForEach(tags, id: \.self) { tag in
                    HStack(spacing: 4) {
                        Text("#\(tag)")
                            .font(.system(size: 12, weight: .medium))
                        Button {
                            tags.removeAll { $0 == tag }
                        } label: {
                            Image(systemName: "xmark")
                                .font(.system(size: 8.5, weight: .bold))
                        }
                    }
                    .foregroundStyle(RemColor.muted)
                    .padding(.horizontal, 9)
                    .padding(.vertical, 4.5)
                    .background(RemColor.bgInset)
                    .clipShape(Capsule())
                    .overlay(Capsule().strokeBorder(RemColor.border, lineWidth: 1))
                }

                HStack(spacing: 2) {
                    Text(newTagText.isEmpty ? "+" : "#")
                        .font(.system(size: 12.5, weight: .medium))
                        .foregroundStyle(RemColor.faint)
                    TextField("add tag", text: $newTagText)
                        .font(.system(size: 12.5))
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .focused($isTagFieldFocused)
                        .onSubmit { commitTag() }
                        .frame(minWidth: 64)
                }
                .padding(.horizontal, 9)
                .padding(.vertical, 4.5)
            }
        }
    }

    private var previewCanvas: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text("QUESTION")
                    .font(.system(size: 10.5, weight: .bold))
                    .tracking(0.8)
                    .foregroundStyle(RemColor.faint)
                Spacer()
                previewTogglePill
            }
            .padding(.bottom, 10)

            RemMarkdownView(
                markdown: front.isEmpty ? "_Empty question_" : front,
                store: appState.store,
                baseFontSize: appState.cardTextSize.revealedQuestionSize,
                codeFontSize: appState.cardTextSize.codeSize,
                textColor: RemColor.text,
                baseWeight: .semibold
            )

            Divider()
                .overlay(RemColor.border)
                .padding(.vertical, 16)

            Text("ANSWER")
                .font(.system(size: 10.5, weight: .bold))
                .tracking(0.8)
                .foregroundStyle(RemColor.faint)
                .padding(.bottom, 10)

            RemMarkdownView(
                markdown: back.isEmpty ? "_No answer yet_" : back,
                store: appState.store,
                baseFontSize: appState.cardTextSize.answerSize,
                codeFontSize: appState.cardTextSize.codeSize,
                textColor: RemColor.text
            )
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    // MARK: - Single-Row Morphing Keyboard Bar (44pt)

    private var morphingKeyboardBar: some View {
        VStack(spacing: 0) {
            Divider().overlay(RemColor.border)
            HStack(spacing: 4) {
                if isFormatBarMode {
                    formatBarContent
                } else {
                    typingBarContent
                }
            }
            .padding(.horizontal, 10)
            .frame(height: 44)
            .background(RemColor.chrome)
        }
    }

    @ViewBuilder
    private var typingBarContent: some View {
        PhotosPicker(selection: $selectedPhotoItem, matching: .images) {
            Image(systemName: "photo")
                .font(.system(size: 16, weight: .medium))
                .foregroundStyle(RemColor.text)
                .frame(width: 36, height: 34)
                .contentShape(Rectangle())
        }
        .accessibilityLabel("Attach photo")

        Button {
            isTagFieldFocused = true
        } label: {
            Text("#")
                .font(.system(size: 16, weight: .semibold))
                .foregroundStyle(isTagFieldFocused ? RemColor.accent : RemColor.text)
                .frame(width: 34, height: 34)
                .background(isTagFieldFocused ? RemColor.accentTint : Color.clear)
                .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
        }
        .accessibilityLabel("Add tag")

        if activeEditorField != nil {
            Button {
                manualFormatMode = true
            } label: {
                Text("Aa")
                    .font(.system(size: 14.5, weight: .semibold))
                    .foregroundStyle(RemColor.text)
                    .frame(width: 36, height: 34)
            }
            .accessibilityLabel("Format text")
        }

        Spacer()

        if activeEditorField == .front {
            Button {
                backController.focus()
            } label: {
                HStack(spacing: 4) {
                    Image(systemName: "arrow.down")
                        .font(.system(size: 10.5, weight: .bold))
                    Text("Answer")
                        .font(.system(size: 12.5, weight: .semibold))
                }
                .foregroundStyle(RemColor.text)
                .padding(.horizontal, 11)
                .padding(.vertical, 5.5)
                .background(RemColor.bgInset)
                .clipShape(Capsule())
                .overlay(Capsule().strokeBorder(RemColor.border, lineWidth: 1))
            }
            .buttonStyle(.plain)
        } else if activeEditorField == .back || isTagFieldFocused {
            Button {
                frontController.focus()
            } label: {
                HStack(spacing: 4) {
                    Image(systemName: "arrow.up")
                        .font(.system(size: 10.5, weight: .bold))
                    Text("Question")
                        .font(.system(size: 12.5, weight: .semibold))
                }
                .foregroundStyle(RemColor.text)
                .padding(.horizontal, 11)
                .padding(.vertical, 5.5)
                .background(RemColor.bgInset)
                .clipShape(Capsule())
                .overlay(Capsule().strokeBorder(RemColor.border, lineWidth: 1))
            }
            .buttonStyle(.plain)
        }

        Button {
            dismissKeyboard()
        } label: {
            Image(systemName: "keyboard.chevron.compact.down")
                .font(.system(size: 16, weight: .medium))
                .foregroundStyle(RemColor.muted)
                .frame(width: 36, height: 34)
        }
        .accessibilityLabel("Hide keyboard")
    }

    @ViewBuilder
    private var formatBarContent: some View {
        formatIconButton(systemName: "bold", label: "Bold") {
            applyFormat(.bold)
        }
        formatIconButton(systemName: "italic", label: "Italic") {
            applyFormat(.italic)
        }
        formatIconButton(systemName: "chevron.left.forwardslash.chevron.right", label: "Inline code") {
            applyFormat(.inlineCode)
        }
        formatIconButton(systemName: "curlybraces", label: "Code block") {
            applyFormat(.codeBlock)
        }
        formatIconButton(systemName: "list.bullet", label: "Bullet list") {
            applyFormat(.bulletList)
        }

        Spacer()

        Button {
            manualFormatMode = false
            hasTextSelection = false
        } label: {
            Text("Done")
                .font(.system(size: 13, weight: .semibold))
                .foregroundStyle(RemColor.accent)
                .padding(.horizontal, 10)
                .padding(.vertical, 5)
                .background(RemColor.accentTint)
                .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
        }
        .buttonStyle(.plain)
    }

    private func formatIconButton(systemName: String, label: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Image(systemName: systemName)
                .font(.system(size: 15, weight: .semibold))
                .foregroundStyle(RemColor.text)
                .frame(width: 36, height: 34)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(label)
    }

    // MARK: - Actions

    private func applyFormat(_ action: MarkdownFormatAction) {
        let hadSelection = hasTextSelection
        if activeEditorField == .front {
            frontController.apply(action, binding: $front)
        } else {
            backController.apply(action, binding: $back)
        }
        if hadSelection {
            manualFormatMode = false
            hasTextSelection = false
        }
    }

    private func dismissKeyboard() {
        frontController.blur()
        backController.blur()
        isTagFieldFocused = false
        activeEditorField = nil
        manualFormatMode = false
        hasTextSelection = false
    }

    private func commitTag() {
        let cleaned = newTagText
            .trimmingCharacters(in: CharacterSet(charactersIn: "# \n\t"))
            .lowercased()
        guard !cleaned.isEmpty else {
            newTagText = ""
            return
        }
        if !tags.contains(cleaned) {
            tags.append(cleaned)
        }
        newTagText = ""
    }

    private func attachPhoto(_ item: PhotosPickerItem) async {
        defer { selectedPhotoItem = nil }
        do {
            guard let data = try await item.loadTransferable(type: Data.self) else { return }
            let mime = item.supportedContentTypes.first?.preferredMIMEType ?? "image/jpeg"
            let hash = try await appState.store.putAsset(mime: mime, bytes: data)
            if !attachedAssetHashes.contains(hash) {
                attachedAssetHashes.append(hash)
            }
            let markdown = "\n![](asset:\(hash))\n"
            if activeEditorField == .front {
                front += markdown
            } else {
                back += markdown
            }
            appState.showToast("Photo attached")
        } catch {
            appState.showToast("Could not attach photo: \(error.localizedDescription)")
        }
    }

    private func removeAttachment(hash: String) {
        attachedAssetHashes.removeAll { $0 == hash }
        front = MarkdownFormatter.removeAssetReference(hash: hash, from: front)
        back = MarkdownFormatter.removeAssetReference(hash: hash, from: back)
    }

    private func save() async {
        commitTag()
        isSaving = true
        do {
            if let existing = editingCard {
                try await appState.store.updateCard(
                    id: existing.id,
                    front: front,
                    back: back,
                    tags: tags,
                    suspended: nil
                )
                await appState.refreshAll()
                onSaved?()
                appState.showToast("Card updated")
            } else {
                _ = try await appState.store.createCard(
                    deckId: selectedDeckId,
                    front: front,
                    back: back,
                    tags: tags
                )
                await appState.refreshAll()
                onSaved?()
                let isCloud = appState.syncStatus?.signedIn == true && !appState.syncAccessRestricted
                appState.showToast(isCloud ? "Card saved · syncing" : "Card saved")
            }
            Task { await appState.syncNow(silent: true) }
            dismiss()
        } catch {
            appState.showToast(error.localizedDescription)
        }
        isSaving = false
    }

    private func deleteCard() async {
        guard let existing = editingCard else { return }
        do {
            try await appState.store.deleteCard(id: existing.id)
            await appState.refreshAll()
            onSaved?()
            appState.showToast("Card deleted")
            Task { await appState.syncNow(silent: true) }
            dismiss()
        } catch {
            appState.showToast(error.localizedDescription)
        }
    }
}

/// Real photo thumbnail tile shown inside `QuickCaptureSheet` instead of raw `asset:<hash>` text.
private struct CaptureAssetThumbnail: View {
    let hash: String
    let store: RemStore
    let onRemove: () -> Void

    @State private var loaded: CachedAssetImage?

    var body: some View {
        ZStack(alignment: .topTrailing) {
            Group {
                if let loaded {
                    Image(uiImage: loaded.image)
                        .resizable()
                        .scaledToFill()
                } else {
                    ZStack {
                        RemColor.bgInset
                        Image(systemName: "photo")
                            .font(.system(size: 18))
                            .foregroundStyle(RemColor.faint)
                    }
                }
            }
            .frame(width: 66, height: 66)
            .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
            .overlay(
                RoundedRectangle(cornerRadius: 10, style: .continuous)
                    .strokeBorder(RemColor.borderStrong, lineWidth: 1)
            )

            Button(action: onRemove) {
                Image(systemName: "xmark")
                    .font(.system(size: 9, weight: .bold))
                    .foregroundStyle(Color.white)
                    .frame(width: 19, height: 19)
                    .background(Color.black.opacity(0.65))
                    .clipShape(Circle())
            }
            .offset(x: 4, y: -4)
            .accessibilityLabel("Remove photo")
        }
        .padding(.top, 4)
        .padding(.trailing, 4)
        .task(id: hash) {
            loaded = await AssetImageCache.shared.load(hash: hash, store: store)
        }
    }
}
