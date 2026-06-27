import ReactMarkdown, { defaultUrlTransform } from 'react-markdown'
import remarkGfm from 'remark-gfm'
import rehypeHighlight from 'rehype-highlight'
import 'highlight.js/styles/github-dark.css'
import { useAssetUrl } from './assetUrl'

const ASSET_SRC = /^asset:([0-9a-f]{64})$/

/** An <img> whose `asset:<hash>` src resolves to an object URL; plain srcs pass through. */
function MarkdownImg({ src, alt }: { src?: string; alt?: string }) {
  const match = src ? ASSET_SRC.exec(src) : null
  const resolved = useAssetUrl(match?.[1])
  const finalSrc = match ? (resolved ?? undefined) : src
  return <img src={finalSrc} alt={alt ?? ''} />
}

/** Allow `asset:<hash>` URLs through in addition to the standard safe protocols. */
function urlTransform(url: string): string {
  return ASSET_SRC.test(url) ? url : defaultUrlTransform(url)
}

/** Renders markdown source (text, code with highlighting, lists, images, etc.). */
export function MarkdownView({ source }: { source: string }) {
  return (
    <div className="markdown">
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        rehypePlugins={[rehypeHighlight]}
        components={{ img: MarkdownImg }}
        urlTransform={urlTransform}
      >
        {source}
      </ReactMarkdown>
    </div>
  )
}
