import Image from '@tiptap/extension-image'
import { NodeSelection } from '@tiptap/pm/state'
import { parseAssetSrc } from './imageSrc'

/** TipTap Image with a DOM node-view that resolves `asset:<hash>` srcs to object
 *  URLs and reflects the stored placement (`#left|#center|#right`) onto the img. */
export function createImageExtension(resolveAsset?: (hash: string) => Promise<string | null>) {
  return Image.extend({
    addNodeView() {
      return ({ node, view, getPos }) => {
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
        const selectImage = (event: MouseEvent) => {
          const position = getPos()
          if (typeof position !== 'number') return
          event.preventDefault()
          view.dispatch(view.state.tr.setSelection(NodeSelection.create(view.state.doc, position)))
          view.focus()
        }
        dom.addEventListener('mousedown', selectImage)
        return {
          dom,
          selectNode: () => dom.classList.add('ProseMirror-selectednode'),
          deselectNode: () => dom.classList.remove('ProseMirror-selectednode'),
          destroy: () => dom.removeEventListener('mousedown', selectImage),
        }
      }
    },
  })
}
