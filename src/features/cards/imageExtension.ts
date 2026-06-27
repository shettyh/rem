import Image from '@tiptap/extension-image'
import { parseAssetSrc } from './imageSrc'

/** TipTap Image with a DOM node-view that resolves `asset:<hash>` srcs to object
 *  URLs and reflects the stored placement (`#left|#center|#right`) onto the img. */
export function createImageExtension(resolveAsset?: (hash: string) => Promise<string | null>) {
  return Image.extend({
    addNodeView() {
      return ({ node }) => {
        const dom = document.createElement('img')
        dom.className = 'card-img'
        const src: string = node.attrs.src ?? ''
        if (node.attrs.alt) dom.alt = node.attrs.alt as string
        const parsed = parseAssetSrc(src)
        if (parsed) {
          dom.dataset.align = parsed.align
          if (resolveAsset) {
            void resolveAsset(parsed.hash).then((url) => {
              if (url) dom.src = url
            })
          }
        } else {
          dom.src = src
        }
        return { dom }
      }
    },
  })
}
