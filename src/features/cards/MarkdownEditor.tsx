import CodeMirror from '@uiw/react-codemirror'
import { markdown } from '@codemirror/lang-markdown'
import { EditorView } from '@codemirror/view'

const extensions = [markdown(), EditorView.lineWrapping]

/** A focused CodeMirror editor for markdown source. */
export function MarkdownEditor({
  value,
  onChange,
  placeholder,
}: {
  value: string
  onChange: (value: string) => void
  placeholder?: string
}) {
  return (
    <CodeMirror
      value={value}
      onChange={onChange}
      extensions={extensions}
      placeholder={placeholder}
      basicSetup={{ lineNumbers: false, foldGutter: false, highlightActiveLine: false }}
      className="cm-wrap"
    />
  )
}
