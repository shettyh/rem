// Render the rem app-icon master PNG from source.html using the same Chromium
// that Playwright already installs for tests, so the real Instrument Serif web
// font is used. Usage: node tools/icon/render.mjs [outPath]
import { chromium } from 'playwright'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const here = path.dirname(fileURLToPath(import.meta.url))
const src = 'file://' + path.join(here, 'source.html')
const out = process.argv[2] || path.join(here, 'master.png')

const browser = await chromium.launch()
const page = await browser.newPage({ viewport: { width: 1024, height: 1024 }, deviceScaleFactor: 2 })
await page.goto(src, { waitUntil: 'networkidle' })
const loadedInstrumentSerif = await page.evaluate(async () => {
  try {
    await document.fonts.load("410px 'Instrument Serif'")
    await document.fonts.ready
    return document.fonts.check("410px 'Instrument Serif'")
  } catch {
    return false
  }
})
if (!loadedInstrumentSerif) {
  console.warn('WARNING: Instrument Serif did not load — falling back to a system serif.')
}
await page.locator('#tile').screenshot({ path: out, omitBackground: true })
await browser.close()
console.log('wrote', out, '(Instrument Serif:', loadedInstrumentSerif ? 'yes' : 'fallback', ')')
