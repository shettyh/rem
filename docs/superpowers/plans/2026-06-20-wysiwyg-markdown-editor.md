# WYSIWYG-markdown Editor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the two-pane CodeMirror+preview card editor with a single inline TipTap WYSIWYG-markdown editor, keeping markdown as the stored source of truth.

**Architecture:** A shared TipTap extension set (StarterKit + CodeBlockLowlight + tiptap-markdown + Placeholder) is exposed by one module so its markdown round-trip can be unit-tested headlessly. A thin controlled React component (`RichMarkdownEditor`) wraps it with `value`/`onChange` props and a selection bubble menu. `CardEditorPage` swaps its preview-pane field for this component. The review surface (`MarkdownView`) is untouched.

**Tech Stack:** React 19, TypeScript, Vite, Vitest, TipTap v3 (`@tiptap/react`), `tiptap-markdown` 0.9, `lowlight` 3 (over the existing `highlight.js` 11), Dexie (unchanged).

## Global Constraints

- **Markdown is the source of truth.** `Card.front`/`Card.back` stay markdown strings; no data-model or DB change.
- **TipTap v3 only.** Use `tiptap-markdown@^0.9.0` (peers on `@tiptap/core ^3`). Do not mix v2 packages.
- **Editing surface only.** Do not modify `MarkdownView.tsx`, `ReviewPage`, `DeckDetailPage`, scheduler, or storage.
- **Code-highlight parity.** Editor code blocks must use `highlight.js`'s `github-dark` theme — the same one the review side already loads — so a code card looks identical in both places.
- **Verification gate.** `npm run typecheck`, `npm run build`, and `npm test` must be green at the end of every task that changes code.
- **Git:** This repo is not under git. Task 0 initializes it (optional). If you skip Task 0, skip every `git commit` step too.

---

### Task 0: Initialize git (optional, recommended)

**Files:** none (creates `.git/`)

- [ ] **Step 1: Check whether git is already initialized**

Run: `git rev-parse --is-inside-work-tree 2>/dev/null || echo "no git"`
Expected: `no git` (if it prints `true`, skip this task).

- [ ] **Step 2: Initialize and make the baseline commit**

```bash
git init
git add -A
git commit -m "chore: baseline before WYSIWYG editor work"
```

Expected: a commit is created. If you skip this task, omit all later commit steps.

---

### Task 1: Add TipTap dependencies

**Files:**
- Modify: `package.json` (dependencies)

**Interfaces:**
- Produces: the packages `@tiptap/react`, `@tiptap/pm`, `@tiptap/starter-kit`, `@tiptap/extension-code-block-lowlight`, `@tiptap/extensions`, `tiptap-markdown`, `lowlight` available to later tasks. (CodeMirror packages stay installed until Task 5 so the build keeps passing.)

- [ ] **Step 1: Install the new dependencies**

Run:
```bash
npm install @tiptap/react @tiptap/pm @tiptap/starter-kit @tiptap/extension-code-block-lowlight @tiptap/extensions tiptap-markdown@^0.9.0 lowlight
```
Expected: install succeeds; `package.json` gains the seven packages at `@tiptap/*` v3.x, `tiptap-markdown` ^0.9, `lowlight` ^3.

- [ ] **Step 2: Confirm the baseline still typechecks**

Run: `npm run typecheck`
Expected: PASS (no source uses the new packages yet; nothing broke).

- [ ] **Step 3: Commit**

```bash
git add package.json package-lock.json
git commit -m "build: add TipTap v3 + tiptap-markdown + lowlight"
```

---

### Task 2: Shared editor extension set + markdown round-trip tests (TDD)

**Files:**
- Create: `src/features/cards/editorExtensions.ts`
- Test: `src/features/cards/editorExtensions.test.ts`

**Interfaces:**
- Produces: `createEditorExtensions(placeholder?: string): Extensions` — the TipTap extension array consumed by `RichMarkdownEditor` in Task 3. Configures StarterKit (codeBlock off, headings 1–3), CodeBlockLowlight (lowlight `common` languages), the `Markdown` serializer/parser, and `Placeholder`.

- [ ] **Step 1: Write the failing test**

Create `src/features/cards/editorExtensions.test.ts`:
```ts
import { describe, it, expect } from 'vitest'
import { Editor } from '@tiptap/core'
import { createEditorExtensions } from './editorExtensions'

/** Parse markdown into the editor, then serialize it straight back out. */
function roundTrip(markdown: string): string {
  const editor = new Editor({
    extensions: createEditorExtensions(),
    content: markdown,
  })
  const out = editor.storage.markdown.getMarkdown()
  editor.destroy()
  return out
}

describe('editor markdown round-trip', () => {
  it('preserves bold text', () => {
    expect(roundTrip('hello **world**')).toContain('**world**')
  })

  it('preserves a fenced code block with its language', () => {
    const out = roundTrip('```js\nconst x = 1\n```')
    expect(out).toContain('```js')
    expect(out).toContain('const x = 1')
  })

  it('preserves bullet list items', () => {
    const out = roundTrip('- one\n- two')
    expect(out).toMatch(/[-*]\s+one/)
    expect(out).toContain('two')
  })
})
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `npx vitest run src/features/cards/editorExtensions.test.ts`
Expected: FAIL — cannot resolve `./editorExtensions` / `createEditorExtensions is not a function`.

- [ ] **Step 3: Write the minimal implementation**

Create `src/features/cards/editorExtensions.ts`:
```ts
import type { Extensions } from '@tiptap/core'
import StarterKit from '@tiptap/starter-kit'
import CodeBlockLowlight from '@tiptap/extension-code-block-lowlight'
import { Placeholder } from '@tiptap/extensions'
import { Markdown } from 'tiptap-markdown'
import { common, createLowlight } from 'lowlight'

const lowlight = createLowlight(common)

/**
 * Shared TipTap extension set: the "practical" markdown feature set —
 * bold, italic, inline code, headings (1–3), bullet/numbered lists, links,
 * and syntax-highlighted fenced code blocks — with markdown as the
 * serialized source of truth (via tiptap-markdown).
 */
export function createEditorExtensions(placeholder?: string): Extensions {
  return [
    StarterKit.configure({
      codeBlock: false, // replaced by CodeBlockLowlight for syntax highlighting
      heading: { levels: [1, 2, 3] },
    }),
    CodeBlockLowlight.configure({ lowlight }),
    Markdown.configure({ transformPastedText: true }),
    Placeholder.configure({ placeholder: placeholder ?? '' }),
  ]
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `npx vitest run src/features/cards/editorExtensions.test.ts`
Expected: PASS (3 tests). If the bullet assertion fails because tiptap-markdown emits a different marker, adjust the regex to the actual marker it produced — do not change the implementation.

- [ ] **Step 5: Typecheck and commit**

Run: `npm run typecheck`
Expected: PASS.
```bash
git add src/features/cards/editorExtensions.ts src/features/cards/editorExtensions.test.ts
git commit -m "feat: shared TipTap markdown extension set with round-trip tests"
```

---

### Task 3: RichMarkdownEditor component + smoke test + styles

**Files:**
- Create: `src/features/cards/RichMarkdownEditor.tsx`
- Test: `src/features/cards/RichMarkdownEditor.test.tsx`
- Modify: `src/ui/styles.css` (add editor + bubble-menu styles)

**Interfaces:**
- Consumes: `createEditorExtensions(placeholder?)` from Task 2.
- Produces: `RichMarkdownEditor({ value, onChange, placeholder?, ariaLabel? })` — a controlled markdown editor used by `CardEditorPage` in Task 4. `value` is markdown in; `onChange(markdown)` fires on user edits only (not on programmatic load).

- [ ] **Step 1: Write the failing smoke test**

Create `src/features/cards/RichMarkdownEditor.test.tsx`:
```tsx
import { describe, it, expect } from 'vitest'
import { render, screen } from '@testing-library/react'
import { RichMarkdownEditor } from './RichMarkdownEditor'

describe('RichMarkdownEditor', () => {
  it('renders provided markdown content', async () => {
    render(<RichMarkdownEditor value="hello **world**" onChange={() => {}} ariaLabel="Front" />)
    expect(await screen.findByText('world')).toBeInTheDocument()
  })

  it('mounts with an empty value without crashing', () => {
    const { container } = render(<RichMarkdownEditor value="" onChange={() => {}} />)
    expect(container.querySelector('.rich-editor')).toBeTruthy()
  })
})
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `npx vitest run src/features/cards/RichMarkdownEditor.test.tsx`
Expected: FAIL — cannot resolve `./RichMarkdownEditor`.

- [ ] **Step 3: Write the component**

Create `src/features/cards/RichMarkdownEditor.tsx`:
```tsx
import { useEffect } from 'react'
import { useEditor, EditorContent } from '@tiptap/react'
import { BubbleMenu } from '@tiptap/react/menus'
import { createEditorExtensions } from './editorExtensions'
import 'highlight.js/styles/github-dark.css'

/** A single inline WYSIWYG-markdown editor. Markdown is the value in and out. */
export function RichMarkdownEditor({
  value,
  onChange,
  placeholder,
  ariaLabel,
}: {
  value: string
  onChange: (markdown: string) => void
  placeholder?: string
  ariaLabel?: string
}) {
  const editor = useEditor({
    extensions: createEditorExtensions(placeholder),
    content: value,
    immediatelyRender: false,
    editorProps: {
      attributes: {
        class: 'rich-editor-content',
        ...(ariaLabel ? { 'aria-label': ariaLabel } : {}),
      },
    },
    onUpdate: ({ editor }) => onChange(editor.storage.markdown.getMarkdown()),
  })

  // Re-sync when the external value changes (e.g. async card load on Edit).
  // Guard prevents cursor-jump / feedback loops while typing.
  useEffect(() => {
    if (!editor) return
    if (value !== editor.storage.markdown.getMarkdown()) {
      editor.commands.setContent(value, { emitUpdate: false })
    }
  }, [value, editor])

  return (
    <div className="rich-editor">
      {editor && (
        <BubbleMenu editor={editor} className="bubble-menu">
          <button type="button" onMouseDown={(e) => e.preventDefault()}
            onClick={() => editor.chain().focus().toggleBold().run()}
            className={editor.isActive('bold') ? 'active' : ''}>B</button>
          <button type="button" onMouseDown={(e) => e.preventDefault()}
            onClick={() => editor.chain().focus().toggleItalic().run()}
            className={editor.isActive('italic') ? 'active' : ''}>i</button>
          <button type="button" onMouseDown={(e) => e.preventDefault()}
            onClick={() => editor.chain().focus().toggleCode().run()}
            className={editor.isActive('code') ? 'active' : ''}>{'</>'}</button>
        </BubbleMenu>
      )}
      <EditorContent editor={editor} />
    </div>
  )
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `npx vitest run src/features/cards/RichMarkdownEditor.test.tsx`
Expected: PASS (2 tests).

- [ ] **Step 5: Add editor styles**

In `src/ui/styles.css`, append after the existing `/* Card editor */` section:
```css
/* Rich (WYSIWYG) editor */
.rich-editor {
  border: 1px solid var(--border);
  border-radius: var(--radius);
  background: var(--surface);
  font-size: 14px;
}

.rich-editor:focus-within {
  outline: 2px solid var(--accent);
  outline-offset: -1px;
}

.rich-editor-content {
  min-height: 120px;
  padding: 10px 12px;
  outline: none;
}

.rich-editor-content > :first-child {
  margin-top: 0;
}

.rich-editor-content > :last-child {
  margin-bottom: 0;
}

.rich-editor-content pre {
  background: #1e1e2e;
  color: #f8f8f2;
  padding: 12px 14px;
  border-radius: 8px;
  overflow-x: auto;
  font-size: 13px;
}

.rich-editor-content code {
  font-family: ui-monospace, SFMono-Regular, 'SF Mono', Menlo, monospace;
  font-size: 0.9em;
}

.rich-editor-content :not(pre) > code {
  background: #f4f4f5;
  padding: 1px 5px;
  border-radius: 5px;
}

.rich-editor-content p.is-editor-empty:first-child::before {
  content: attr(data-placeholder);
  color: var(--muted);
  float: left;
  height: 0;
  pointer-events: none;
}

.bubble-menu {
  display: flex;
  gap: 4px;
  padding: 4px;
  background: var(--text);
  border-radius: 8px;
}

.bubble-menu button {
  background: transparent;
  border: none;
  color: #fff;
  cursor: pointer;
  padding: 4px 8px;
  border-radius: 4px;
  font-size: 13px;
  line-height: 1;
}

.bubble-menu button:hover,
.bubble-menu button.active {
  background: rgba(255, 255, 255, 0.18);
}
```

- [ ] **Step 6: Typecheck and commit**

Run: `npm run typecheck`
Expected: PASS.
```bash
git add src/features/cards/RichMarkdownEditor.tsx src/features/cards/RichMarkdownEditor.test.tsx src/ui/styles.css
git commit -m "feat: RichMarkdownEditor component with bubble menu and styles"
```

---

### Task 4: Wire RichMarkdownEditor into CardEditorPage

**Files:**
- Modify: `src/features/cards/CardEditorPage.tsx`

**Interfaces:**
- Consumes: `RichMarkdownEditor` from Task 3.

- [ ] **Step 1: Replace the imports**

In `src/features/cards/CardEditorPage.tsx`, change the two editor imports:
```tsx
import { MarkdownEditor } from './MarkdownEditor'
import { MarkdownView } from './MarkdownView'
```
to:
```tsx
import { RichMarkdownEditor } from './RichMarkdownEditor'
```

- [ ] **Step 2: Replace the `CardField` body**

Replace the entire `CardField` function (currently the two-column editor-grid + preview) with:
```tsx
function CardField({
  label,
  value,
  onChange,
}: {
  label: string
  value: string
  onChange: (value: string) => void
}) {
  return (
    <div className="stack">
      <span className="field-label">{label}</span>
      <RichMarkdownEditor
        value={value}
        onChange={onChange}
        placeholder={`${label} (markdown)…`}
        ariaLabel={label}
      />
    </div>
  )
}
```

- [ ] **Step 3: Typecheck and run the full suite**

Run: `npm run typecheck && npm test`
Expected: PASS — all tests green, no unused-import errors (`MarkdownEditor`/`MarkdownView` no longer referenced in this file).

- [ ] **Step 4: Manual verification in the dev server**

Run: `npm run dev`, then in the browser:
1. Open a deck → **New card**. Type `**bold**`, `` `code` ``, `- a list`, and a fenced ` ```js ` block — confirm each renders inline as you type.
2. Save the card; reopen it via **Edit** — confirm the saved markdown loads back into the editor intact.
3. Confirm the code block's highlighting looks the same in the editor as on the review screen.
Stop the dev server when done. (This step is manual because TipTap input rules are not reliably testable in jsdom.)

- [ ] **Step 5: Commit**

```bash
git add src/features/cards/CardEditorPage.tsx
git commit -m "feat: use RichMarkdownEditor in CardEditorPage"
```

---

### Task 5: Remove CodeMirror editor and orphaned styles

**Files:**
- Delete: `src/features/cards/MarkdownEditor.tsx`
- Modify: `package.json` (remove CodeMirror deps)
- Modify: `src/ui/styles.css` (remove now-orphaned `.cm-wrap`, `.editor-grid`, `.preview` rules)

**Interfaces:**
- Consumes: nothing. This is cleanup of code orphaned by Tasks 3–4.

- [ ] **Step 1: Confirm `MarkdownEditor` is unused, then delete it**

Run: `grep -rn "MarkdownEditor\b" src` (expect: no matches except the file itself / its own definition).
Then delete the file:
```bash
git rm src/features/cards/MarkdownEditor.tsx
```
(If git was skipped: `rm src/features/cards/MarkdownEditor.tsx`.)

- [ ] **Step 2: Remove the orphaned CSS**

In `src/ui/styles.css`, delete these now-unused rules (orphaned by removing the two-pane editor):
- `.editor-grid { ... }`
- `.cm-wrap { ... }`, `.cm-wrap .cm-editor { ... }`, `.cm-wrap .cm-focused { ... }`
- `.preview { ... }`
- the `@media (max-width: 640px) { .editor-grid { ... } }` block

Leave `.card-snippet`, `.field-label`, and all other rules intact.

- [ ] **Step 3: Remove the CodeMirror dependencies**

Run:
```bash
npm uninstall @uiw/react-codemirror @codemirror/lang-markdown
```
Expected: both removed from `package.json` dependencies.

- [ ] **Step 4: Full verification gate**

Run: `npm run typecheck && npm test && npm run build`
Expected: all green. `npm run build` confirms no dangling CodeMirror imports.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "chore: remove CodeMirror editor and orphaned styles"
```

---

## Self-Review

**Spec coverage** (against `2026-06-20-wysiwyg-markdown-editor-design.md`):
- New `RichMarkdownEditor` controlled component → Task 3. ✓
- Simplify `CardEditorPage` CardField (drop grid/preview/MarkdownView) → Task 4. ✓
- Delete `MarkdownEditor.tsx`; keep `MarkdownView.tsx` → Task 5 (delete) / untouched. ✓
- Markdown round-trip via `getMarkdown()` / `setContent(value, {emitUpdate:false})` → Tasks 2 & 3. ✓
- Practical feature set (StarterKit codeBlock off, headings 1–3, CodeBlockLowlight, Markdown, Placeholder) → Task 2. ✓
- Input rules + bubble menu → Task 3. ✓
- Code-highlight consistency (lowlight `common` + shared github-dark.css) → Tasks 2 & 3. ✓
- Dependencies added; CodeMirror removed → Tasks 1 & 5. ✓
- Testing strategy (headless round-trip unit tests + React smoke test + manual live check) → Tasks 2, 3, 4. ✓
- Success criteria 1–5 → covered by Tasks 3–5 and the final gate. ✓

**Placeholder scan:** No TBD/TODO; every code step shows full code; every test step shows the command and expected result.

**Type consistency:** `createEditorExtensions(placeholder?: string): Extensions` is defined in Task 2 and consumed identically in Task 3. `RichMarkdownEditor` prop shape (`value`, `onChange`, `placeholder?`, `ariaLabel?`) is defined in Task 3 and used identically in Task 4. `editor.storage.markdown.getMarkdown()` and `setContent(value, { emitUpdate: false })` used consistently across Tasks 2–3.
