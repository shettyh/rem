import type { Extensions } from '@tiptap/core'
import StarterKit from '@tiptap/starter-kit'
import CodeBlockLowlight from '@tiptap/extension-code-block-lowlight'
import { Placeholder } from '@tiptap/extensions'
import { Markdown } from 'tiptap-markdown'
import { common, createLowlight } from 'lowlight'
import { createImageExtension } from './imageExtension'

const baseLowlight = createLowlight(common)
// Avoid random syntax auto-detection on untagged code blocks while keeping explicit languages highlighted.
const lowlight = {
  ...baseLowlight,
  highlight: baseLowlight.highlight.bind(baseLowlight),
  listLanguages: baseLowlight.listLanguages.bind(baseLowlight),
  registered: baseLowlight.registered.bind(baseLowlight),
  highlightAuto: (value: string) => ({
    type: 'root' as const,
    children: [{ type: 'text' as const, value }],
  }),
}

/**
 * Shared TipTap extension set: the "practical" markdown feature set —
 * bold, italic, inline code, headings (1–3), bullet/numbered lists, links,
 * and syntax-highlighted fenced code blocks — with markdown as the
 * serialized source of truth (via tiptap-markdown).
 */
export function createEditorExtensions(
  placeholder?: string,
  resolveAsset?: (hash: string) => Promise<string | null>,
): Extensions {
  return [
    StarterKit.configure({
      codeBlock: false, // replaced by CodeBlockLowlight for syntax highlighting
      heading: { levels: [1, 2, 3] },
      link: {
        protocols: ['asset'],
        openOnClick: false,
      },
    }),
    CodeBlockLowlight.configure({ lowlight }),
    createImageExtension(resolveAsset),
    Markdown.configure({ transformPastedText: true }),
    Placeholder.configure({ placeholder: placeholder ?? '' }),
  ]
}
