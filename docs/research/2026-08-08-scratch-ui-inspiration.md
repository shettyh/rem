# UI simplification: lessons from Scratch

_Date: 2026-08-08_  
_Reference: [erictli/scratch](https://github.com/erictli/scratch) at commit [`9126a5a`](https://github.com/erictli/scratch/commit/9126a5adf78abbabd111302ca0b659ca9278d43a)_

## Executive summary

rem already has a clear visual identity, but too many parts of that identity are active at once: warm layered backgrounds, purple fills and tints, five deck colors, four review-grade colors, three typefaces, large radii, borders, and shadows. The result is more “designed” than calm.

Scratch's useful lesson is not to copy its exact appearance. It is to make the persistent application chrome nearly monochrome, use typography and spacing before containers, and reserve emphasis for the user's content and current action. For rem, the right direction is **near-monochrome chrome with sparse purple**, while retaining semantic color only where it materially helps studying.

## What Scratch does well

### 1. A very small persistent color system

Scratch's light theme is essentially white, a warm off-white secondary surface, dark text, muted text, and translucent neutral fills/borders. Its accent is the same color as its foreground text. The dark theme follows the same structure rather than introducing a new saturated accent palette. See [the pinned light and dark tokens](https://github.com/erictli/scratch/blob/9126a5adf78abbabd111302ca0b659ca9278d43a/src/App.css#L8-L78).

This means selection, hover, borders, and controls do not all compete as separate color events. Color used inside content—syntax highlighting, images, diagrams—feels stronger because the chrome is quiet.

### 2. Flat hierarchy for persistent UI

The sidebar is one secondary background with one dividing hairline; its header and rows use spacing, type weight, and muted neutral fills rather than cards or branded fills ([sidebar source](https://github.com/erictli/scratch/blob/9126a5adf78abbabd111302ca0b659ca9278d43a/src/components/layout/Sidebar.tsx#L313-L440)). Selected list items use the same neutral muted fill as hover, with text/opacity carrying the remaining state difference ([list-item source](https://github.com/erictli/scratch/blob/9126a5adf78abbabd111302ca0b659ca9278d43a/src/components/ui/index.tsx#L142-L202)).

### 3. Compact, predictable controls

Scratch defines a narrow 24–40 px button-height scale. Default and secondary controls are neutral fills, ghost controls are mostly text until hover, and the primary action is a foreground-colored fill ([button source](https://github.com/erictli/scratch/blob/9126a5adf78abbabd111302ca0b659ca9278d43a/src/components/ui/Button.tsx#L10-L30)). This keeps toolbar actions from becoming a second content layer.

### 4. Focus is created by removing chrome

Scratch can collapse the sidebar and its focus mode fades both sidebar and editor toolbar rather than creating a more decorated “focus” surface ([app layout](https://github.com/erictli/scratch/blob/9126a5adf78abbabd111302ca0b659ca9278d43a/src/App.tsx#L471-L490), [editor toolbar behavior](https://github.com/erictli/scratch/blob/9126a5adf78abbabd111302ca0b659ca9278d43a/src/components/editor/Editor.tsx#L2235-L2268)). Its README explicitly describes this as distraction-free writing ([README](https://github.com/erictli/scratch/blob/9126a5adf78abbabd111302ca0b659ca9278d43a/README.md#L19-L29)).

### 5. Native-feeling typography in the chrome

Scratch uses the platform system sans stack for its application UI and keeps serif/mono as supporting or content choices ([font tokens](https://github.com/erictli/scratch/blob/9126a5adf78abbabd111302ca0b659ca9278d43a/src/App.css#L81-L98)). That makes small labels and controls feel crisper and less stylized.

## Where rem currently feels busy

### Persistent color has too many jobs

In [`src/ui/tokens.css`](../../src/ui/tokens.css), purple currently drives primary actions, active navigation, due counts, selected rows, inline code, progress, chips, badges, and one review grade. At the same time, deck colors and the red/orange/green/purple grade system remain visible. The user sees several independent color grammars on the same screen.

The Today screen is the clearest example: the greeting, split review card, gradient action, purple count, two chips, colored deck rails, deck metadata, and add form all ask for attention. The primary action is clear, but almost everything around it is also styled as an object.

### Too many simultaneous hierarchy devices

Across [`src/ui/styles.css`](../../src/ui/styles.css), static surfaces commonly combine a background, border, radius, and shadow. KPI cards, deck cards, card rows, editor fields, settings panels, review cards, and toolbars each become a separate container. A minimal interface normally chooses one separator at a time:

1. whitespace first;
2. then a subtle background difference;
3. then a hairline when a boundary is necessary;
4. shadow only for something floating above the page.

### Three expressive type voices compete

Space Grotesk, Instrument Serif, and Space Mono all appear throughout the application chrome. The serif is used for greetings, deck names, KPI values, list content, review content, and editor fields; mono is used for most labels, chips, dates, counts, and hints. This creates personality, but it also makes routine UI metadata feel editorial.

A calmer split would be:

- **system sans:** all chrome, navigation, controls, labels, settings, stats;
- **serif (optional):** flashcard front/back and review content only;
- **mono:** code and keyboard shortcuts only.

### Semantic and decorative color are mixed

Deck colors are identity, grade colors are actions, purple is brand/primary/selection, and status chips introduce more tints. Because these appear together, red/orange/green no longer read purely as feedback and purple no longer reads purely as the primary action.

## Recommended direction for rem

### Color rule: one saturated moment per viewport

Keep rem's purple, but demote it from general-purpose UI color to sparse emphasis. On a normal screen it should appear in at most one dominant place: the primary study action, progress, or a small brand marker—not all three plus navigation and counts.

Suggested role split:

| Role | Light | Dark | Use |
|---|---|---|---|
| canvas | `#ffffff` or `#fcfcfb` | `#161413` | main content |
| secondary canvas | `#fafaf9` | `#0e0c0b` | sidebar/inset region |
| muted fill | `rgba(28,25,23,.055)` | `rgba(250,249,249,.055)` | hover/selection/chips |
| border | `rgba(28,25,23,.09)` | `rgba(250,249,249,.08)` | hairlines only |
| text | `#1c1917` | `#fafaf9` | primary content/action fill |
| muted text | `#78716c` | `#a8a29e` | secondary content |
| rem purple | retain current family | retain current family | study CTA, progress, focus only |
| danger | red | red | destructive action/error/Again only |

This borrows Scratch's neutral foundation without erasing rem's identity.

### Component rules

1. **Active navigation:** neutral muted fill; remove lavender selection. Keep a tiny purple brand dot only if needed.
2. **Deck identity:** keep deck color as an 8 px dot. Remove full-width colored card rails and avoid tinting surfaces with deck color.
3. **Primary actions:** one solid action per region. Secondary actions become ghost or neutral outline controls.
4. **Static surfaces:** no shadow. Use shadows only for menus, dialogs, sheets, and drag overlays.
5. **Radii:** use roughly 6 px for controls and 10–12 px for true grouped surfaces. Avoid 16–20 px on every card.
6. **Status:** prefer plain muted text. Use pills only when users need to scan or act on the state.
7. **Grade controls:** use neutral buttons, make **Good** the visual default, and reserve red for **Again**. Hard/Easy can be distinguished by label and interval rather than orange/purple rails. If four colors are retained, restrict them to a tiny marker or interval text.
8. **Typography:** move chrome to the system sans stack. Keep the serif for card content if that distinction is valuable; stop using mono for ordinary labels and counts.

## Screen-by-screen changes

### Today — highest impact

- Replace the split purple gradient review band with one quiet summary row: large due count, “2 new · 0 review,” and one compact **Start review** button.
- Reduce the greeting from 68 px to about 40–44 px, or make “Today” the content heading and keep the greeting as secondary copy.
- Turn deck cards into a simple grouped list, or at minimum remove shadows, top color rails, and FSRS chips. FSRS is an implementation detail and does not need persistent visual priority.
- Show the new-deck form on explicit `+` action rather than permanently if reducing page density is desirable.

### Deck detail

- Replace the three bordered KPI cards with one stat strip separated by spacing or vertical hairlines.
- Put all cards inside one list surface with row dividers instead of giving every row its own rounded border.
- Render `new`, `due`, and scheduled intervals as quiet text; reserve colored status for leech/suspended states that require attention.

### Review

- Keep the focused, sidebar-free composition; it is already one of rem's cleanest screens.
- Remove the card shadow, reduce the radius, and soften or remove the outline.
- Replace the full-width saturated **Show answer** slab with a compact action or a neutral foreground-colored button.
- Neutralize the grade row as described above. The answer content should be the strongest visual element, not the controls.
- Keep the progress bar purple only if the action itself is neutral; avoid two large purple elements at once.

### Card editor

- Keep front/back separation, because it represents the card model, but group both fields into one editor surface separated by a hairline rather than two large cards.
- Flatten the formatting toolbar into the page header or a borderless sticky row.
- Keep serif styling inside editable card content, not in the surrounding labels and controls.
- Move tags and destructive actions into a quiet metadata/footer area.

### Stats

- Flatten the four KPI cards into one top summary row.
- Keep one bordered chart region, but remove unnecessary panel chrome from the lower distributions.
- Use deck colors only in deck breakdown markers. Use semantic grade colors only in the grade chart, not elsewhere on the screen.

### Settings and sidebar

- Settings can keep grouped panels, but remove their shadows and rely on one outer boundary plus row dividers.
- Simplify the brand from icon + word + dot + `RECALL` to one or two elements.
- Turn the labeled theme control into an icon button with tooltip.
- Adopt Scratch's collapsible sidebar/focus-mode idea for editing and other narrow or concentration-heavy screens. rem already removes it during review, which validates the pattern.

## What not to copy from Scratch

- Do not remove all semantic feedback. Reviewing has meaningful failure/destructive states that a note editor does not.
- Do not copy Scratch's yellow text selection or exact palette merely for visual similarity.
- Do not add Scratch's extensive appearance customization. A strong default system is more important for rem right now.
- Do not make flashcard front/back visually indistinguishable in the editor; that separation is domain structure, not decoration.

## Suggested implementation order

### Pass 1: quiet the system with CSS only

Change [`src/ui/tokens.css`](../../src/ui/tokens.css) and [`src/ui/styles.css`](../../src/ui/styles.css):

- neutral canvas/sidebar/surface roles;
- neutral active/hover states;
- sparse accent use;
- no shadows on static surfaces;
- smaller radius scale;
- system sans for chrome and mono only for code/keyboard hints;
- remove the Today gradient and reduce colored rails/tints.

This should deliver most of the improvement without changing behavior or component structure.

### Pass 2: flatten the busiest screens

Make small structural edits to:

- [`src/features/decks/DeckListPage.tsx`](../../src/features/decks/DeckListPage.tsx);
- [`src/features/cards/DeckDetailPage.tsx`](../../src/features/cards/DeckDetailPage.tsx);
- [`src/features/review/GradeButtons.tsx`](../../src/features/review/GradeButtons.tsx);
- the card editor layout.

Prefer grouped rows and dividers over independent cards.

### Pass 3: focus behavior

Add collapsible sidebar/focus mode outside review only after the visual simplification is validated. It is useful, but it should not be used to hide a busy base design.

## Visual acceptance criteria

- No ordinary screen has more than one dominant saturated element.
- No static content surface uses a shadow.
- No component combines fill + border + large radius + shadow unless it floats above the page.
- Application chrome uses one sans family; mono is limited to code and keyboard shortcuts.
- Deck color appears as a small identity marker, not a surface treatment.
- Red is reserved for destructive/error/Again states; other routine status is neutral.
- Light and dark screenshots have the same hierarchy, not merely inverted colors.
