# Design: Real-browser UI testing capability

_Date: 2026-06-20_

## Context

`rem` (a local-first spaced-repetition app) currently tests only through **vitest +
jsdom** (22 tests, all green: scheduler, storage, review cycle, one editor component
test). jsdom does not do real layout or CSS rendering, so it structurally cannot catch
visual/layout problems in the UI.

The user wants two things, in order:

1. A committed, repeatable ability to **test UI components in a real browser**.
2. To use that ability to surface the concrete UI issues they're seeing, which then
   feed the next sub-project (the UI redesign pass, roadmap #2).

This spec covers **only sub-project 1: the testing capability + an issue-finding pass**.
The redesign and export/import are separate specs.

## Goal / success criteria

- A repeatable, committed way to render `rem`'s components **and** full pages in **real
  Chromium** (real CSS/layout), with **screenshot capture**.
- `npm test` remains a single command and stays green; the existing 22 unit tests are
  untouched and keep running.
- Deliverable feeding the redesign: baseline screenshots of all primary screens plus an
  enumerated list of concrete UI issues found.

## Approach

**Chosen: Vitest Browser Mode** (`@vitest/browser-playwright` + `vitest-browser-react`),
over Playwright E2E or Playwright Component Testing.

Rationale: it slots into the existing Vitest stack — one runner, same `npm test`, same
Testing-Library idioms — and keeps the existing unit tests in a node/jsdom project while
adding a browser project that renders in real Chromium and can take screenshots. Because
IndexedDB works in a real browser, the **actual app** (`RouterProvider` + real
`DexieStorage`) can be rendered and screenshotted, covering most of what an E2E runner
would, without a second toolchain.

Rejected:
- **Playwright E2E** against the dev server — most faithful, but introduces a second test
  runner + config and is slower/more brittle for component-level checks.
- **Playwright Component Testing** (`experimental-ct-react`) — experimental, weaker React
  19 support, second runner.

## Architecture

### Dependencies (dev)
- `@vitest/browser-playwright` — Playwright provider for Vitest browser mode.
- `vitest-browser-react` — `render()` for React components in browser mode.
- `playwright` — browser engine; requires a one-time `npx playwright install chromium`
  (bundled Chromium download).

### Config (`vite.config.ts`)
Split `test` into two projects so nothing existing moves or breaks:

```ts
test: {
  projects: [
    {
      // existing behavior, unchanged
      extends: true,
      test: {
        name: 'unit',
        environment: 'jsdom',
        setupFiles: './src/test/setup.ts',
        include: ['src/**/*.test.{ts,tsx}'],
        exclude: ['src/**/*.browser.test.tsx', ...defaultExclude],
      },
    },
    {
      extends: true,
      test: {
        name: 'browser',
        include: ['src/**/*.browser.test.tsx'],
        setupFiles: './src/test/browser-setup.ts',
        browser: {
          enabled: true,
          provider: playwright(),
          headless: true,
          instances: [{ browser: 'chromium', viewport: { width: 1280, height: 800 } }],
        },
      },
    },
  ],
}
```

- `unit` keeps the current 22 tests (jsdom, `fake-indexeddb`).
- `browser` runs real Chromium and only picks up `*.browser.test.tsx`.
- `npm test` (`vitest run`) runs both projects.

### Screenshot mechanism
- `src/test/browser-setup.ts` imports `../ui/styles.css` so real CSS applies in the
  browser project.
- A small helper captures a screenshot of a rendered element/page and writes a PNG to
  `test-artifacts/` (gitignored). The PNGs are how the implementer visually inspects the
  UI (read the image files).
- Capture uses the Playwright provider screenshot path available in Vitest browser mode
  (element/page `.screenshot()`).

### Issue-finding pass
A `screens.browser.test.tsx` renders each primary screen against real `DexieStorage`
seeded with sample data, screenshots it, and the implementer enumerates problems. Screens:

1. Deck list — with decks (due + zero badges) and the empty state.
2. Deck detail — with cards, the "Study (n)" / "Nothing due" states, and the empty state.
3. Card editor — new and edit, with the TipTap rich editor and a code-block sample.
4. Review — front-only and revealed (front+back) with the grade bar; plus
   "nothing due" and "review complete" states.

Output: `docs/superpowers/specs/2026-06-20-ui-issues.md` listing each concrete issue with
the screenshot it came from. That document is the input spec for the redesign sub-project.

### File layout (new)
```
src/test/browser-setup.ts            # imports styles.css for browser project
src/test/screenshot.ts               # render+screenshot helper
src/features/**/<x>.browser.test.tsx # real-browser render/screenshot tests
test-artifacts/                      # PNG output (gitignored)
docs/superpowers/specs/2026-06-20-ui-issues.md  # enumerated issues (deliverable)
```

## Out of scope
- Any actual UI redesign or visual fixes (next sub-project).
- Visual-regression baselining/diffing (`toMatchScreenshot`) — not needed yet; screenshots
  are for human inspection, not automated pixel diffing.
- Export/import (separate sub-project).
- Migrating the existing jsdom editor test to the browser project — it passes; leave it.

## Verification
- `npm test` runs `unit` + `browser` projects, all green.
- At least one real-browser render+screenshot test passes and writes a readable PNG.
- `test-artifacts/` contains baseline screenshots of every primary screen/state above.
- `docs/.../2026-06-20-ui-issues.md` exists and enumerates the concrete issues found.
