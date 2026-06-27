import ReactMarkdown, { defaultUrlTransform } from 'react-markdown'
import remarkGfm from 'remark-gfm'
import rehypeHighlight from 'rehype-highlight'
import 'highlight.js/styles/github-dark.css'
import { useAssetUrl } from './assetUrl'
import { parseAssetSrc, isAssetSrc } from './imageSrc'

/** An <img> whose `asset:<hash>` src resolves to an object URL and reflects its
 *  stored placement; plain srcs pass through unchanged. */
function MarkdownImg({ src, alt }: { src?: string; alt?: string }) {
  const parsed = parseAssetSrc(src)
  const resolved = useAssetUrl(parsed?.hash)
  const finalSrc = parsed ? (resolved ?? undefined) : src
  return <img className="card-img" src={finalSrc} alt={alt ?? ''} data-align={parsed?.align} />
}

/** Allow `asset:<hash>` URLs through in addition to the standard safe protocols. */
function urlTransform(url: string): string {
  return isAssetSrc(url) ? url : defaultUrlTransform(url)
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
