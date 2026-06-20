# WYSIWYG-markdown editor — design

_Date: 2026-06-20 · Roadmap item: Near-term #1_

## Goal

Replace the two-pane card editor (CodeMirror source + live preview) with a single
inline **WYSIWYG-markdown** editor (Typora/Obsidian style): the user types, markdown
syntax transforms live, and **markdown stays the source of truth**. Built on **TipTap**.

Non-goals: changing how cards render during review, any data-model change, or a
broader visual redesign (that is Near-term #2).

## Scope & blast radius

Only the **editing** surface changes. `MarkdownView.tsx` (react-markdown +
`rehype-highlight`) still renders cards in `ReviewPage` and `DeckDetailPage`.

- **New** `src/features/cards/RichMarkdownEditor.tsx` — controlled TipTap editor.
  Props mirror today's `MarkdownEditor`:
  `value: string` (markdown), `onChange: (md: string) => void`, `placeholder?: string`,
  plus `ariaLabel?: string` for the field.
- **Simplify** `CardEditorPage.tsx` `CardField`: remove the `.editor-grid` two-column
  layout, the `.preview` div, and the `MarkdownView` import. A field becomes a label +
  `RichMarkdownEditor` (single column).
- **Delete** `src/features/cards/MarkdownEditor.tsx` (CodeMirror).
- **Keep** `src/features/cards/MarkdownView.tsx` unchanged.

`CardEditorPage.save()` is unchanged — it still persists `front`/`back` markdown strings.

## Markdown round-trip (data flow)

The stored value is markdown; the ProseMirror document is only the editing
representation.

- **Load:** the editor initializes from `value`. Edit-mode loads the card
  asynchronously (`storage.getCard`), so a guarded effect re-syncs:

  ```ts
  useEffect(() => {
    if (!editor) return
    if (value !== editor.storage.markdown.getMarkdown()) {
      editor.commands.setContent(value, false) // false = do not emit an update
    }
  }, [value, editor])
  ```

  The guard prevents cursor-jump and feedback loops: while the user types, the
  parent's `value` already equals `getMarkdown()`, so the effect is a no-op.
- **Edit:** on the editor `update` event → `onChange(editor.storage.markdown.getMarkdown())`.

## Extensions / feature set ("practical set")

`StarterKit` configured with `codeBlock: false` and `heading: { levels: [1, 2, 3] }`,
plus:

- `CodeBlockLowlight({ lowlight })` — fenced code blocks with syntax highlighting and
  language identifier preserved in markdown.
- `Markdown` (from `tiptap-markdown`) — serialize/parse; configure
  `transformPastedText: true` so pasted markdown is parsed.
- `Placeholder` — empty-field hint.
- `Link` — inline links.

Interaction: **markdown input rules** (type `**bold**`, `- `, `` ``` ``, `# `, etc. and
it transforms live) plus a minimal **bubble menu** on text selection
(bold / italic / inline code / link).

Resulting feature set: bold, italic, inline code, fenced code blocks w/ highlighting +
language, bullet & numbered lists, H1–H3, links.

## Code-highlight consistency

`lowlight` is registered with the **common** language set
(`createLowlight(common)`) and emits `hljs` spans — the same classes the review side
already styles via `highlight.js/styles/github-dark.css` (loaded by `MarkdownView`).
A code card therefore looks identical while editing and while reviewing. The
github-dark stylesheet import is shared/available on both surfaces.

## Dependencies

**Add:** `@tiptap/react`, `@tiptap/pm`, `@tiptap/starter-kit`,
`@tiptap/extension-code-block-lowlight`, `@tiptap/extension-placeholder`,
`@tiptap/extension-link`, `tiptap-markdown`, `lowlight`. (`highlight.js` already
present.)

**Remove** (orphaned by deleting `MarkdownEditor`): `@uiw/react-codemirror`,
`@codemirror/lang-markdown`.

**Version compatibility:** `tiptap-markdown` has historically tracked TipTap v2.
During implementation, pin the TipTap packages + `tiptap-markdown` to a mutually
compatible set and confirm `npm run build` / `npm run typecheck` are green before
building further. If a compatible v3 set is not available, pin the whole TipTap suite
to v2.

**Styling:** TipTap is headless, so the editor CSS is written here — a clean, minimal
`.rich-editor` style reusing existing CSS tokens (`--surface`, `--border`, `--radius`,
etc.). Deeper visual polish is deferred to Near-term #2.

## Testing strategy

ProseMirror/TipTap is unreliable to drive in jsdom (it needs real DOM
ranges/selection), so live input-rule typing is not cleanly assertable in Vitest.
TDD what is reliable; do not write brittle tests for the rest.

- **Round-trip test** (`RichMarkdownEditor.test.tsx`): mount with markdown that
  includes a fenced code block and a list; assert it serializes back to equivalent
  markdown via `onChange` / `getMarkdown()`. Mount with empty string and assert the
  placeholder path does not throw.
- **Integration:** existing `reviewCycle.test.ts` and `MarkdownView` rendering stay
  green (untouched surface). Run the full suite to confirm no regression.
- **Live WYSIWYG interaction** (input rules, bubble menu): verified manually via
  `npm run dev` — explicitly out of automated scope for now. A future Playwright pass
  could cover it.

## Risks / open items

- **Version compat** between `tiptap-markdown` and the TipTap major version — verified
  by a green build/typecheck gate before further work (see Dependencies).
- **Round-trip fidelity** for exotic markdown (deeply nested lists, etc.) may differ
  slightly; acceptable because markdown remains the stored source.
- **Not a git repo:** this repository is not under git, so the spec/commit step is
  skipped unless `git init` is requested.

## Success criteria

1. Creating and editing a card uses a single inline editor; no preview pane.
2. Existing cards (markdown strings) load, edit, and save without data change.
3. A code card renders with identical syntax highlighting while editing and reviewing.
4. `npm run build`, `npm run typecheck`, and `npm test` are green.
5. CodeMirror dependencies are removed; no orphaned imports remain.
