import type { ReactNode } from 'react'
import ReactMarkdown, { defaultUrlTransform } from 'react-markdown'
import remarkGfm from 'remark-gfm'
import remarkMath from 'remark-math'
import rehypeHighlight from 'rehype-highlight'
import rehypeKatex from 'rehype-katex'
import 'highlight.js/styles/github-dark.css'
import 'katex/dist/katex.min.css'
import { useAsset } from './assetUrl'
import { parseAssetSrc, isAssetSrc } from './imageSrc'
import { CLOZE_ANSWER_HREF, CLOZE_BLANK_HREF, renderClozeMarkdown } from './cloze'
import { normalizeMathDelimiters } from './mathPreprocess'

function AudioAssetPlayer({ url, label }: { url: string; label?: ReactNode }) {
  return (
    <span className="card-audio-player">
      <span className="card-audio-label">{label || 'Audio'}</span>
      <audio key={url} className="card-audio-el" controls preload="metadata" src={url} />
    </span>
  )
}

/** An <img> whose `asset:<hash>` src resolves to an object URL and reflects its
 *  stored placement; audio assets render as an inline player; plain srcs pass through. */
function MarkdownImg({ src, alt }: { src?: string; alt?: string }) {
  const parsed = parseAssetSrc(src)
  const resolved = useAsset(parsed?.hash)
  if (parsed && resolved?.mime.startsWith('audio/')) {
    return <AudioAssetPlayer url={resolved.url} label={alt || 'Audio'} />
  }
  const finalSrc = parsed ? (resolved?.url ?? undefined) : src
  return <img className="card-img" src={finalSrc} alt={alt ?? ''} data-align={parsed?.align} />
}

/** Custom <a> handling cloze pills (`#rem-cloze-*`) and inline audio assets (`asset:<hash>`). */
function MarkdownLink({ href, children }: { href?: string; children?: ReactNode }) {
  const parsedAsset = parseAssetSrc(href)
  const resolved = useAsset(parsedAsset?.hash)

  if (href === CLOZE_BLANK_HREF) {
    return <span className="cloze-blank">[{children}]</span>
  }
  if (href === CLOZE_ANSWER_HREF) {
    return <span className="cloze-reveal">{children}</span>
  }
  if (parsedAsset) {
    if (resolved?.mime.startsWith('audio/')) {
      return <AudioAssetPlayer url={resolved.url} label={children} />
    }
    if (!resolved) {
      return <span className="card-audio-missing">{children}</span>
    }
    return (
      <a href={resolved.url} download>
        {children}
      </a>
    )
  }
  return (
    <a href={href} target="_blank" rel="noreferrer">
      {children}
    </a>
  )
}

/** Allow `asset:<hash>` URLs and internal `#rem-cloze-*` markers through url sanitizing. */
function urlTransform(url: string): string {
  if (url === CLOZE_BLANK_HREF || url === CLOZE_ANSWER_HREF || isAssetSrc(url)) {
    return url
  }
  return defaultUrlTransform(url)
}

/** Renders markdown source (text, LaTeX math, cloze deletions, code with highlighting, lists, images, audio). */
export function MarkdownView({
  source,
  clozeIndex,
  clozeRevealed = false,
}: {
  source: string
  clozeIndex?: number | null
  clozeRevealed?: boolean
}) {
  const mathNormalized = normalizeMathDelimiters(source)
  const prepared =
    clozeIndex && clozeIndex >= 1
      ? renderClozeMarkdown(mathNormalized, clozeIndex, clozeRevealed)
      : mathNormalized

  return (
    <div className="markdown">
      <ReactMarkdown
        remarkPlugins={[remarkGfm, [remarkMath, { singleDollarTextMath: true }]]}
        rehypePlugins={[
          [rehypeKatex, { strict: false, throwOnError: false, trust: false, maxExpand: 1000, maxSize: 100 }],
          rehypeHighlight,
        ]}
        components={{ img: MarkdownImg, a: MarkdownLink }}
        urlTransform={urlTransform}
      >
        {prepared}
      </ReactMarkdown>
    </div>
  )
}
