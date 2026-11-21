import PhotosUI
import SwiftUI

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

    @FocusState private var focusedField: CaptureField?

    enum CaptureField {
        case front
        case back
        case tag
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

    var body: some View {
        NavigationStack {
            VStack(spacing: 0) {
                ScrollView {
                    VStack(alignment: .leading, spacing: 12) {
                        HStack {
                            if isEditing, let deck = selectedDeck {
                                HStack(spacing: 7) {
                                    Circle()
                                        .fill(Color(hexString: deck.color))
                                        .frame(width: 8, height: 8)
                                    Text(deck.name)
                                        .font(.system(size: 13, weight: .semibold))
                                        .foregroundStyle(RemColor.text)
                                }
                                .padding(.horizontal, 11)
                                .padding(.vertical, 7)
                                .background(RemColor.bgInset)
                                .clipShape(RoundedRectangle(cornerRadius: 9, style: .continuous))
                                .overlay(
                                    RoundedRectangle(cornerRadius: 9, style: .continuous)
                                        .strokeBorder(RemColor.border, lineWidth: 1)
                                )
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
                                    HStack(spacing: 7) {
                                        Circle()
                                            .fill(Color(hexString: selectedDeck?.color ?? "#5a47e8"))
                                            .frame(width: 8, height: 8)
                                        Text(selectedDeck?.name ?? "Select deck")
                                            .font(.system(size: 13, weight: .semibold))
                                            .foregroundStyle(RemColor.text)
                                            .lineLimit(1)
                                        Image(systemName: "chevron.down")
                                            .font(.system(size: 10, weight: .semibold))
                                            .foregroundStyle(RemColor.muted)
                                    }
                                    .padding(.horizontal, 11)
                                    .padding(.vertical, 7)
                                    .background(RemColor.bgInset)
                                    .clipShape(RoundedRectangle(cornerRadius: 9, style: .continuous))
                                    .overlay(
                                        RoundedRectangle(cornerRadius: 9, style: .continuous)
                                            .strokeBorder(RemColor.border, lineWidth: 1)
                                    )
                                }
                            }

                            Spacer()

                            Picker("Mode", selection: $isPreview) {
                                Text("Write").tag(false)
                                Text("Preview").tag(true)
                            }
                            .pickerStyle(.segmented)
                            .frame(width: 146)
                        }

                        if isPreview {
                            previewCard
                        } else {
                            editorFields
                        }
                    }
                    .padding(16)
                }

                if !isPreview {
                    markdownToolbar
                }
            }
            .background(RemColor.bg)
            .navigationTitle(isEditing ? "Edit card" : "New card")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                        .foregroundStyle(RemColor.muted)
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") {
                        Task { await save() }
                    }
                    .fontWeight(.semibold)
                    .foregroundStyle(canSave ? RemColor.accent : RemColor.faint)
                    .disabled(!canSave)
                }
            }
            .onAppear {
                if let card = editingCard {
                    selectedDeckId = card.deckId
                    front = card.front
                    back = card.back
                    tags = card.tags
                    attachedAssetHashes = card.assetHashes
                } else {
                    selectedDeckId = initialDeckId ?? appState.decks.first?.id ?? ""
                    focusedField = .front
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
                Text("This permanently deletes the card and its scheduling history.")
            }
        }
    }

    private var editorFields: some View {
        VStack(alignment: .leading, spacing: 12) {
            VStack(alignment: .leading, spacing: 6) {
                Text("QUESTION")
                    .font(.system(size: 10.5, weight: .bold))
                    .tracking(0.8)
                    .foregroundStyle(RemColor.faint)

                TextField("What do you want to remember?", text: $front, axis: .vertical)
                    .font(.system(size: 15))
                    .lineLimit(3...8)
                    .focused($focusedField, equals: .front)
            }
            .padding(13)
            .background(RemColor.surface)
            .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
            .overlay(
                RoundedRectangle(cornerRadius: 12, style: .continuous)
                    .strokeBorder(RemColor.border, lineWidth: 1)
            )

            VStack(alignment: .leading, spacing: 6) {
                Text("ANSWER")
                    .font(.system(size: 10.5, weight: .bold))
                    .tracking(0.8)
                    .foregroundStyle(RemColor.faint)

                TextField("Write a concise recall answer (Markdown supported)…", text: $back, axis: .vertical)
                    .font(.system(size: 15))
                    .lineLimit(5...14)
                    .focused($focusedField, equals: .back)
            }
            .padding(13)
            .background(RemColor.surface)
            .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
            .overlay(
                RoundedRectangle(cornerRadius: 12, style: .continuous)
                    .strokeBorder(RemColor.border, lineWidth: 1)
            )

            if !attachedAssetHashes.isEmpty {
                VStack(spacing: 6) {
                    ForEach(attachedAssetHashes, id: \.self) { hash in
                        HStack(spacing: 8) {
                            Image(systemName: "photo")
                                .font(.system(size: 12))
                                .foregroundStyle(RemColor.muted)
                            Text("asset:\(hash.prefix(12))…")
                                .font(.system(size: 12, design: .monospaced))
                                .foregroundStyle(RemColor.text)
                            Spacer()
                            Text("Attached")
                                .font(.system(size: 11, weight: .semibold))
                                .foregroundStyle(RemColor.good)
                        }
                        .padding(.horizontal, 12)
                        .padding(.vertical, 8)
                        .background(RemColor.bgInset)
                        .clipShape(RoundedRectangle(cornerRadius: 9, style: .continuous))
                    }
                }
            }

            VStack(alignment: .leading, spacing: 8) {
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
                                        .font(.system(size: 9, weight: .bold))
                                }
                            }
                            .foregroundStyle(RemColor.muted)
                            .padding(.horizontal, 10)
                            .padding(.vertical, 5)
                            .background(RemColor.bgInset)
                            .clipShape(Capsule())
                            .overlay(Capsule().strokeBorder(RemColor.border, lineWidth: 1))
                        }

                        HStack(spacing: 4) {
                            Text("#")
                                .foregroundStyle(RemColor.faint)
                            TextField("add tag", text: $newTagText)
                                .font(.system(size: 12.5))
                                .textInputAutocapitalization(.never)
                                .autocorrectionDisabled()
                                .focused($focusedField, equals: .tag)
                                .onSubmit { commitTag() }
                                .frame(minWidth: 72)
                        }
                        .padding(.horizontal, 10)
                        .padding(.vertical, 5)
                        .background(RemColor.surface)
                        .clipShape(Capsule())
                        .overlay(
                            Capsule().strokeBorder(RemColor.borderStrong, style: StrokeStyle(lineWidth: 1, dash: [3]))
                        )
                    }
                }
            }

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
                .padding(.top, 6)
            }
        }
    }

    private var previewCard: some View {
        RemCardSurface(padding: 18) {
            VStack(alignment: .leading, spacing: 12) {
                Text("QUESTION")
                    .font(.system(size: 10.5, weight: .bold))
                    .tracking(0.8)
                    .foregroundStyle(RemColor.faint)
                Text(localizedMarkdown(front.isEmpty ? "_Empty question_" : front))
                    .font(.system(size: 17, weight: .medium))
                    .foregroundStyle(RemColor.text)

                Divider().overlay(RemColor.border).padding(.vertical, 4)

                Text("ANSWER")
                    .font(.system(size: 10.5, weight: .bold))
                    .tracking(0.8)
                    .foregroundStyle(RemColor.faint)
                Text(localizedMarkdown(back.isEmpty ? "_No answer yet_" : back))
                    .font(.system(size: 15.5))
                    .foregroundStyle(RemColor.text)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private var markdownToolbar: some View {
        VStack(spacing: 0) {
            Divider().overlay(RemColor.border)
            HStack(spacing: 6) {
                toolbarButton("B", bold: true) { insertSnippet("**bold**") }
                toolbarButton("I", italic: true) { insertSnippet("*italic*") }
                toolbarButton("`code`", mono: true) { insertSnippet("`code`") }
                toolbarButton("```", mono: true) { insertSnippet("\n```\n\n```") }

                PhotosPicker(selection: $selectedPhotoItem, matching: .images) {
                    HStack(spacing: 4) {
                        Image(systemName: "photo")
                        Text("Photo")
                    }
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(RemColor.muted)
                    .padding(.horizontal, 9)
                    .padding(.vertical, 6)
                    .background(RemColor.surface)
                    .clipShape(RoundedRectangle(cornerRadius: 7, style: .continuous))
                    .overlay(
                        RoundedRectangle(cornerRadius: 7, style: .continuous)
                            .strokeBorder(RemColor.border, lineWidth: 1)
                    )
                }

                Spacer()

                Text("Normal card")
                    .font(.system(size: 11))
                    .foregroundStyle(RemColor.faint)
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 9)
            .background(RemColor.chrome)
        }
    }

    private func toolbarButton(
        _ title: String,
        bold: Bool = false,
        italic: Bool = false,
        mono: Bool = false,
        action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            Text(title)
                .font(mono
                      ? .system(size: 11.5, weight: .semibold, design: .monospaced)
                      : .system(size: 12.5, weight: bold ? .bold : .semibold))
                .italic(italic)
                .foregroundStyle(RemColor.muted)
                .padding(.horizontal, 9)
                .padding(.vertical, 6)
                .background(RemColor.surface)
                .clipShape(RoundedRectangle(cornerRadius: 7, style: .continuous))
                .overlay(
                    RoundedRectangle(cornerRadius: 7, style: .continuous)
                        .strokeBorder(RemColor.border, lineWidth: 1)
                )
        }
    }

    private func insertSnippet(_ snippet: String) {
        if focusedField == .front {
            front += (front.isEmpty || front.hasSuffix(" ") || front.hasSuffix("\n") ? "" : " ") + snippet
        } else {
            back += (back.isEmpty || back.hasSuffix(" ") || back.hasSuffix("\n") ? "" : " ") + snippet
        }
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
            if focusedField == .front {
                front += markdown
            } else {
                back += markdown
            }
            appState.showToast("Photo attached")
        } catch {
            appState.showToast("Could not attach photo: \(error.localizedDescription)")
        }
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
                let isCloud = appState.syncStatus?.signedIn == true
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

    private func localizedMarkdown(_ raw: String) -> AttributedString {
        (try? AttributedString(markdown: raw, options: .init(interpretedSyntax: .inlineOnlyPreservingWhitespace)))
            ?? AttributedString(raw)
    }
}
