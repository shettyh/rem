import Image from '@tiptap/extension-image'

const ASSET_SRC = /^asset:([0-9a-f]{64})$/

/** TipTap Image with a DOM node-view that resolves `asset:<hash>` srcs to object URLs. */
export function createImageExtension(resolveAsset?: (hash: string) => Promise<string | null>) {
  return Image.extend({
    addNodeView() {
      return ({ node }) => {
        const dom = document.createElement('img')
        const src: string = node.attrs.src ?? ''
        if (node.attrs.alt) dom.alt = node.attrs.alt as string
        const match = ASSET_SRC.exec(src)
        if (match && resolveAsset) {
          void resolveAsset(match[1]).then((url) => {
            if (url) dom.src = url
          })
        } else {
          dom.src = src
        }
        return { dom }
      }
    },
  })
}
