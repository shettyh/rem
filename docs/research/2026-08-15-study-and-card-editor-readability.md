# Study mode and card-editor readability

_Date: 2026-08-15_

## Executive summary

rem does **not** need a new display font or a blanket increase in card text size. The recent study-layout work already made the most important choices well:

- card content uses the platform system sans stack;
- the unrevealed question is `26–30px` at `1.4` line height;
- the revealed answer is `21px` at `1.55` line height;
- answer measure is capped at `68ch`;
- the card editor uses the same sans family at `21px / 1.5`.

Those values are already more generous than the U.S. Web Design System's `16px` body-text floor and GOV.UK's `19px` default paragraph benchmark. They also align with USWDS guidance to use at least `1.5` line height for longer text and target roughly 66 characters per line.

The next pass should therefore improve the **edges of the reading system**, not replace its foundation:

1. fix dark-theme primary-button text contrast;
2. raise essential card-region labels from `10–11px` to `12px`;
3. make code blocks, links, tables, and other rich Markdown as readable as plain paragraphs;
4. ensure a very long question cannot be clipped by vertical centering;
5. add a user-controlled study text-size preference instead of searching for one universal “right” size;
6. keep the card editor's typography aligned with study mode and make its narrow-window toolbar robust.

## What the guidance actually says

There is no single standards-defined “correct font size.” Font metrics, viewing distance, platform scaling, user vision, line length, and spacing all affect the effective reading size.

High-trust design-system benchmarks provide a useful range:

- [USWDS Typography](https://designsystem.digital.gov/components/typography/) recommends at least an effective `16px` for most body copy, while allowing smaller type sparingly for specialized UI.
- [GOV.UK Paragraphs](https://design-system.service.gov.uk/styles/paragraphs/) uses `19px` for default paragraphs and `24px` for a lead paragraph.
- [USWDS Typography](https://designsystem.digital.gov/components/typography/) recommends `45–90` characters per line, with `66` as a good target for long text, and at least `1.5` line height for longer text.
- [WCAG 2.2 Visual Presentation](https://www.w3.org/WAI/WCAG22/Understanding/visual-presentation.html) uses no more than 80 characters, non-justified text, and at least `1.5` line spacing as an AAA presentation target.

Accessibility requirements matter more than choosing between 19, 20, or 21 pixels:

- [WCAG 2.2 Resize Text](https://www.w3.org/WAI/WCAG22/Understanding/resize-text.html) requires text to resize to 200% without loss of content or functionality.
- [WCAG 2.2 Reflow](https://www.w3.org/WAI/WCAG22/Understanding/reflow.html) requires content to work at a width equivalent to 320 CSS pixels without two-dimensional scrolling, except where a two-dimensional layout is essential.
- [WCAG 2.2 Text Spacing](https://www.w3.org/WAI/WCAG22/Understanding/text-spacing.html) requires content to remain functional when users override line height to `1.5`, paragraph spacing to `2em`, letter spacing to `0.12em`, and word spacing to `0.16em`. It does **not** require every interface to use those values by default.
- [WCAG 2.2 Contrast (Minimum)](https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum.html) requires `4.5:1` for normal text and `3:1` for large text.

On typeface choice, [USWDS Typography](https://designsystem.digital.gov/components/typography/) says serif faces can help true extended long-form reading, while straightforward neutral faces suit interfaces; it explicitly notes that this is not a hard rule. Flashcards are short, mixed-format, interaction-heavy content rather than essays. rem's platform system sans is therefore the safer default, especially across macOS, Windows, and Linux. Mono should remain limited to code, intervals, and key hints.

## Current implementation audit

### Study mode

Relevant styles are in [`src/ui/styles.css`](../../src/ui/styles.css), under `Review` and `Grade buttons`.

| Element | Current | Assessment |
| --- | --- | --- |
| Unrevealed question | system sans, `clamp(26px, 3vw, 30px)`, `1.4`, max stage `720px` | Strong default for a short retrieval prompt. |
| Revealed question | `20px / 1.5`, muted | Good de-emphasis after recall; light contrast is about `5.41:1`, dark about `7.99:1`. |
| Answer | `21px / 1.55`, `max-width: 68ch` | Already in the recommended range. `66ch` would align exactly with the USWDS target, but `68ch` is not a material problem. |
| Question/Answer labels | `11px`, uppercase | Too small for essential orientation labels; raise to `12px`. |
| Grade controls | label `13px`, interval `12px`, key `10px` | Usable but visually small at a distance. A modest `14 / 13 / 11px` scale would improve scanning without enlarging the dock much. |
| Code blocks | `14px / 1.5` globally | Too large a drop from `21px` prose for code-heavy learning cards. Use at least `16px` in review/editor card content. |
| Answer measure | `68ch` | Good. Do not make it full-width. |
| Alignment | left, not justified | Correct. |
| Font family | platform system sans | Keep it. Do not switch all cards to Instrument Serif. |

### Contrast

The primary light-theme button is sound: white on `#5a47e8` is about `6.00:1`.

The dark theme currently sets `--accent: #8174ff` and `--on-accent: #ffffff`. White on that accent is about **`3.57:1`**, which fails the `4.5:1` threshold for the 13.5–14px text used by **Show answer** and **Save card**. The same token combination affects both study and card creation.

The simplest correction is to use a dark foreground on the light lavender accent in dark mode: `#161413` on `#8174ff` is about **`5.14:1`**. Because `--on-accent` is also used by danger buttons, verify all semantic button pairs before changing the shared token. Dark text on the current dark-theme danger red `#e5484d` is about **`4.69:1`**, so the shared dark foreground works for both current fills.

The primary and muted card text colors themselves pass normal-text contrast in both themes:

| Pair | Approximate contrast |
| --- | ---: |
| light primary `#1c1917` on `#fff` | `17.49:1` |
| light muted/faint `#6f6965` on `#fff` | `5.41:1` |
| dark primary `#fafaf9` on `#161413` | `17.58:1` |
| dark muted `#b0aaa5` on `#161413` | `7.99:1` |
| dark faint `#a8a29e` on `#161413` | `7.28:1` |

### Rich Markdown

Plain paragraphs are treated well, but the rendering system supports more than paragraphs:

- fenced code is only `14px`;
- links have no rem-specific style, so browser defaults control their light/dark appearance;
- GFM tables have no study-specific layout or overflow treatment;
- blockquotes rely on browser defaults;
- heading treatment is scoped to revealed answers, not consistently shared by questions and the editor.

For a product advertising rich Markdown cards, these are part of core reading quality. A typography pass should test a representative card containing paragraphs, lists, nested lists, inline code, a code block, a link, headings, a blockquote, an image, and a wide table.

### Long-question behavior

The unrevealed state uses:

```css
.review:not(.is-revealed) .review-scroll {
  display: grid;
  place-items: center;
}
```

Centering is attractive for short prompts, but an item taller than its scroll viewport can overflow above the scroll origin when centered. The existing browser test protects long **answers**, not long **questions**. Use safe alignment (`safe center`) where supported, or switch oversized prompts to start alignment, and add a regression test proving the first and last lines of a long front are reachable.

## Recommended study typography

Keep the current family and almost all current content sizes:

```text
Question, hidden-answer state:  28px default, fluid 26–30px; line-height 1.4–1.45
Question, revealed state:       20px; line-height 1.5; muted but >= 4.5:1
Answer:                         21px default; line-height 1.55; max 66–68ch
Card-region labels:             12px / 600–650 / modest uppercase tracking
Code blocks:                    16px minimum; line-height 1.5–1.6
Grade label / interval / key:   14px / 13px / 11px
```

Additional rules:

- Keep normal weight around 400 for running card text; avoid light weights.
- Keep left alignment and ragged-right lines.
- Remove or reduce negative letter spacing on multi-line study content. The current `-0.015em` question tracking is decorative, not a readability requirement.
- Preserve paragraph separation around `0.8–1em`; current answer paragraph spacing of `0.9em` is good.
- Do not expand the answer beyond roughly `68ch`, even on a wide window.
- Do not use mono or serif for all card content. Mono is for code; serif can remain an optional future user preference, not the default.

## The important feature: adjustable study text

A fixed `21px` default is good, but no default is right for every user. rem currently exposes no in-app card-text size control. A native webview also cannot assume the visible browser zoom UI available on a normal website.

Add a small persisted **Card text size** preference with three clear presets rather than a complicated typography panel:

| Preset | Answer/editor body | Unrevealed question |
| --- | ---: | ---: |
| Compact | `18px` | `24–27px` |
| Standard (default) | `21px` | `26–30px` |
| Large | `24px` | `30–34px` |

The control could live in Settings and optionally expose `A− / A+` in the review header. It should change card prose, list text, headings, and code proportionally while leaving compact application chrome alone. Use `rem` or semantic CSS custom properties so 200% scaling and text-spacing overrides remain testable.

This is higher-value than debating 20px versus 21px.

## Card editor: what should and should not change

Relevant code:

- [`src/features/cards/CardEditorPage.tsx`](../../src/features/cards/CardEditorPage.tsx)
- [`src/features/cards/RichMarkdownEditor.tsx`](../../src/features/cards/RichMarkdownEditor.tsx)
- [`src/features/cards/EditorToolbar.tsx`](../../src/features/cards/EditorToolbar.tsx)
- the `Card editor` section in [`src/ui/styles.css`](../../src/ui/styles.css)

### Keep

- Keep `21px / 1.5` system-sans editing text. It is already comfortable and closely matches the study answer.
- Keep Front and Back in one grouped surface with a structural divider.
- Keep one shared formatting toolbar and the current `720px` outer editor width.
- Keep labels outside the editable ProseMirror content so users cannot delete them.

### Improve now

1. **Raise Front, Back, and Tags labels from `10px` to `12px`.** These are semantic field labels, not optional footnotes.
2. **Fix the shared dark primary-button contrast token.** This repairs Save card and Show answer together.
3. **Raise code blocks to at least `16px` in card content.** Editing and study rendering should agree.
4. **Style all supported Markdown consistently.** Links need explicit theme-safe colors and focus treatment; blockquotes and tables need readable defaults.
5. **Make the toolbar safe at the minimum 760px app width.** With the 220px narrow sidebar, the editor has roughly 460px of inner space. The normal toolbar only just fits; image-alignment controls add enough width to overflow. Prefer a horizontally scrollable single row or a carefully tested wrap.
6. **Autofocus Front for a new card.** This removes an unnecessary click and makes the primary task immediately obvious. Do not autofocus when editing an existing card if it would disturb selection or screen-reader context.

### Improve later, only if observed in use

- Add a true study-preview toggle using the same `MarkdownView` and study typography. This is better than trying to make the editor imitate both the large unrevealed question and the smaller revealed document at once.
- Add brief prompt/answer guidance only if card-quality testing shows users need it. Do not fill the quiet editor with permanent instructional copy by default.
- Consider editor focus mode only if the sidebar is measurably distracting or the minimum-width toolbar remains cramped. Hiding navigation is a larger behavior change than this readability pass requires.

## Proposed implementation order

### Pass 1 — small, high-confidence CSS and tests

- dark-theme `--on-accent` contrast fix;
- `12px` study/editor field labels;
- `14 / 13 / 11px` grade-control typography;
- `16px` scoped card code blocks;
- safe long-question scrolling;
- explicit link, blockquote, and table styles;
- narrow-window toolbar overflow handling.

Verify in light and dark themes at:

- default window (`1040×720` from `tauri.conf.json`);
- minimum window (`760×540`);
- 200% text/zoom simulation;
- short and long question/answer fixtures;
- a rich-Markdown fixture.

### Pass 2 — user-adjustable size

- add Compact / Standard / Large semantic card-size tokens;
- persist the preference;
- apply it to review and editor card content;
- add `A− / A+` review affordances only if the Settings-only control is too remote.

### Pass 3 — optional editor preview

Add only after the shared typography and Markdown rendering are stable.

## Test gaps

The current browser test in [`src/features/review/reveal.browser.test.tsx`](../../src/features/review/reveal.browser.test.tsx) checks that answer font size is no more than `24px` and line height is at least `1.5`. It does not protect:

- a useful minimum answer size;
- text measure;
- long-front reachability;
- rich Markdown readability;
- dark primary-button contrast;
- minimum-window editor toolbar behavior;
- 200% resizing/reflow.

The editor browser test intentionally checks equal Front/Back font size and line height. Preserve that unless a dedicated preview mode is introduced; typography hierarchy inside the editing surface should not be changed accidentally.

## Decision

The recommended direction is:

> **Keep system sans and the current 21px answer/editor default. Improve labels, contrast, rich content, overflow, and user scaling.**

This gives rem a more readable study mode without turning the interface into oversized display typography or introducing a decorative font that performs worse for mixed prose, code, controls, and cross-platform rendering.
