---
name: phpthunder
description: Native PHP IDE in the Thunder Purple theme; calm storm-gray islands, one lilac strike of accent.
colors:
  thunder-lilac: "#B98BE8"
  thunder-lilac-bright: "#CBA5F0"
  thunder-lilac-deep: "#A373D6"
  island: "#1D1D26"
  frame: "#2F2F40"
  popup: "#2C2C3B"
  caret-line: "#242433"
  border: "#2E2D3D"
  border-strong: "#4E4C63"
  selected: "#34334A"
  selected-strong: "#5C3A7A"
  hover-wash: "rgba(255, 255, 255, 0.06)"
  pressed-wash: "rgba(255, 255, 255, 0.11)"
  text: "#D0D0D9"
  text-bright: "#FFFFFF"
  text-soft: "#C9C9D1"
  muted: "#8785A6"
  gutter: "#575566"
  ink-on-accent: "#14161C"
  input-well: "#151821"
  input-stroke: "#262B36"
  placeholder: "#5C6475"
  selection: "#2B3A5C"
  success: "#76B366"
  warning: "#E0AF68"
  danger: "#DD3962"
  syn-keyword: "#E6967A"
  syn-function: "#C7A65D"
  syn-field: "#93A6F5"
  syn-string: "#62A362"
  syn-number: "#37AEF3"
  syn-comment: "#676773"
  syn-doc-tag: "#8C6154"
typography:
  title:
    fontFamily: "Geist, system-ui, sans-serif"
    fontSize: "14px"
    fontWeight: 600
  body:
    fontFamily: "Geist, system-ui, sans-serif"
    fontSize: "13px"
    fontWeight: 400
  label:
    fontFamily: "Geist, system-ui, sans-serif"
    fontSize: "12px"
    fontWeight: 500
  code:
    fontFamily: "Geist Mono, ui-monospace, monospace"
    fontSize: "13.5px"
    fontWeight: 400
    lineHeight: "22px"
rounded:
  xs: "4px"
  sm: "5px"
  md: "6px"
  lg: "8px"
  island: "10px"
spacing:
  island-gap: "5px"
  xs: "4px"
  sm: "8px"
  md: "10px"
  lg: "16px"
components:
  button-primary:
    backgroundColor: "{colors.thunder-lilac}"
    textColor: "{colors.ink-on-accent}"
    rounded: "{rounded.md}"
    padding: "4px 10px"
  button-primary-hover:
    backgroundColor: "{colors.thunder-lilac-bright}"
    textColor: "{colors.ink-on-accent}"
  button-primary-pressed:
    backgroundColor: "{colors.thunder-lilac-deep}"
    textColor: "{colors.ink-on-accent}"
  button-ghost:
    textColor: "{colors.muted}"
    rounded: "{rounded.xs}"
    padding: "2px 6px"
  button-ghost-hover:
    backgroundColor: "{colors.hover-wash}"
    textColor: "{colors.text-bright}"
  row:
    textColor: "{colors.text-soft}"
    rounded: "{rounded.xs}"
    padding: "0 8px"
    height: "26px"
  row-selected:
    backgroundColor: "{colors.selected}"
    textColor: "{colors.text-bright}"
  rail-button:
    textColor: "{colors.muted}"
    rounded: "{rounded.lg}"
  rail-button-active:
    backgroundColor: "{colors.selected}"
    textColor: "{colors.thunder-lilac}"
  input-boxed:
    backgroundColor: "{colors.input-well}"
    textColor: "{colors.text-bright}"
    rounded: "{rounded.sm}"
    padding: "4px 8px"
  island:
    backgroundColor: "{colors.island}"
    rounded: "{rounded.island}"
  popup:
    backgroundColor: "{colors.popup}"
    textColor: "{colors.text}"
    rounded: "{rounded.lg}"
  pill:
    backgroundColor: "{colors.input-stroke}"
    textColor: "{colors.muted}"
    rounded: "{rounded.xs}"
    padding: "2px 6px"
---

# Design System: phpthunder

## Overview

**Creative North Star: "Storm Glass"**

The workbench is a calm purple-gray sky seen through glass. Rounded dark islands (project tree, editor, bottom panel) float on a slightly lighter violet frame that holds the title bar, tool rail and status bar. Nothing glows at rest. The single lilac accent is the lightning: it strikes only where attention must land right now (the active tool, a fuzzy-match letter, a focused field, the one primary action) and is gone everywhere else.

The mood is calm, dense, precise. This is a tool used for whole days, so chrome contrast stays low and information density stays high: 26px rows, 13px UI text, 22px code lines. Color carries meaning, not decoration; the warm syntax hues (coral keywords, ochre functions, periwinkle fields) are the brightest things on screen because the code is the work.

Everything is native and drawn by iced in Rust: no web chrome, no gradients, no blur. Interactive surfaces are quiet until touched, answering hover with a faint white wash and press with a deeper one.

**Key Characteristics:**
- Dark tonal islands (10px corners) separated by a 5px gap of violet frame.
- One accent, Thunder Lilac, used surgically.
- Geist for interface, Geist Mono for code, both bundled.
- Flat panes; only floating surfaces cast shadows.
- Translucent interaction washes that work on every surface.

## Colors

A cool storm-violet neutral range with one lilac accent and a warm, readable syntax palette.

### Primary
- **Thunder Lilac** (`thunder-lilac`): active rail icon, fuzzy-match letters, find marks, enabled toggles, focused input stroke, and the fill of the primary button (dark ink on top). Brightens to `thunder-lilac-bright` on hover, deepens to `thunder-lilac-deep` on press.

### Neutral
- **Island Night** (`island`): the editor, tree and bottom panel surfaces; also the theme background.
- **Storm Frame** (`frame`): the window frame around islands: title bar, tool rail, status bar.
- **Cloud Popup** (`popup`): floating surfaces (completion, hover, palette, tooltips, toasts, dialogs), outlined in `border-strong`.
- **Caret Band** (`caret-line`): band behind the caret's line.
- **Seam** (`border`): 1px separators between panes.
- **Dusk Selection** (`selected`): selected rows, active rail button, hovered completion rows. `selected-strong` (a plum) marks the selected completion/palette row and pressed selections.
- **Text ladder**: `text-bright` for emphasis (active tab, selected rows, titles, input values), `text` default, `text-soft` secondary (tree files, locations), `muted` (a desaturated lavender) for hints, counts, inactive tabs and icons, `gutter` for line numbers.
- **Selection Blue** (`selection`): text selection in editor and inputs; deliberately blue, not lilac, so it never reads as an accent.

### Semantic
- `success` green, `warning` amber, `danger` rose: test results, Git marks, Problems, destructive confirmations.

### Syntax
Coral keywords (`syn-keyword`), ochre functions (`syn-function`), periwinkle fields and variables (`syn-field`), green strings, sky-blue numbers, gray comments, rust-brown doc tags. Types and punctuation stay in the default text tone. Completion kind badges reuse the same hues as fills with `island` ink.

### Named Rules
**The Lightning Rule.** Thunder Lilac appears only where attention must land now. If two lilac things compete on one screen, one of them is wrong.

**The Wash Rule.** Hover and press are translucent white washes (`hover-wash`, `pressed-wash`), never new solid colors, so one token reads on frame, island and popup alike.

## Typography

**Body Font:** Geist (bundled 400/500/600)
**Mono Font:** Geist Mono (bundled 400/500/600)

**Character:** A neutral, slightly geometric grotesque paired with its own mono sibling; the pair feels like one voice, so code and chrome never clash.

### Hierarchy
- **Title** (600, 14px): dialog titles, section headers, the wordmark.
- **Body** (400, 13px): default UI text, rows, panel content.
- **Label** (500, 12px): button labels, file names in results, small metadata.
- **Code** (Geist Mono 400, 13.5px, 22px line): editor, gutter, code previews, shortcut labels. Every glyph advances 0.6em, so columns map to x by arithmetic. Commas and semicolons render semibold.

### Named Rules
**The Two-Family Rule.** Only Geist and Geist Mono. Hierarchy comes from weight (400/500/600) and the text-color ladder, not from size jumps; nothing in the workbench exceeds 14px.

## Layout

Fixed desktop workbench, resizable panes. A 32px title bar (inset 76px on macOS for traffic lights, 6px elsewhere) and a status bar wrap a tool rail plus islands. Islands sit 5px apart and 5px from the window edge, letting the frame show as a seam. Rows are 26px (completion rows 28px); the gutter is 56px. Popups have fixed widths (completion list 460px with a 300px doc pane, hover 460px).

Spacing is a tight 4/8/10/16 rhythm: 4px inside icon buttons, 8px horizontal row padding, 10px for header bars and pills, 16px for dialog bodies.

## Elevation & Depth

Hybrid. Panes are flat and separated by tone alone: island over frame, caret band over island. Shadows exist only on surfaces that float above the work: popups, the palette, dialogs, tooltips and toasts share one heavy, soft drop shadow (black at 45%, 12px down, 32px blur) plus a 1px `border-strong` outline.

### Named Rules
**The Float-Only Shadow Rule.** If it does not float over other content, it does not cast a shadow.

## Shapes

Soft but small corners, graded by scale: 4px for rows, badges, pills, ghost buttons and toggles; 5px for boxed inputs; 6px for the primary button; 8px for popups and rail buttons; 10px for islands. Strokes are 1px, used for floating-surface outlines, focused inputs and toggles that are on. No pill-shaped buttons, no sharp corners.

## Components

### Buttons
Quiet until touched.
- **Primary:** Thunder Lilac fill, dark ink label, 6px corners. Hover brightens, press deepens, disabled drops to 40% alpha. Reserved for the one call to action on a surface ("Open Folder", "Find").
- **Ghost:** transparent, `muted` label; hover adds the wash and brightens text to `text-bright`; disabled dims to `gutter`. Close buttons, find arrows, replace actions.
- **Toggle (regex/case/word):** off is a ghost; on is a 16% lilac tint, lilac label and a 45% lilac 1px stroke.

### Rows (tree, lists, results)
Transparent 26px rows with 4px corners; wash on hover, `selected` fill and bright text when selected, plum `selected-strong` while a selected row is pressed.

### Tool Rail
Icon buttons with 8px corners; `muted` icons at rest, active tool gets `selected` fill and a lilac icon.

### Inputs
- **Boxed** (find bar, dialogs): near-black well, 1px `input-stroke`, 5px corners; focus turns the stroke lilac.
- **Bare** (palette query): transparent, borderless, bright value.

### Popups
Cloud Popup surface, 8px corners, `border-strong` outline, float shadow. Completion rows carry a 4px-rounded kind badge in the syntax hue of the kind.

### Scrollbars
Thin translucent gray scrollers (18% alpha, 35% on hover), 4px rounded, no track.

## Do's and Don'ts

### Do:
- **Do** take every color from the theme palette in `ui/theme.rs`; add a named constant before using a new shade twice.
- **Do** use `hover-wash` / `pressed-wash` for interaction feedback on any surface.
- **Do** keep UI text at 12–14px and let weight and the text ladder carry hierarchy.
- **Do** separate panes with tone and the 5px frame gap, not lines.
- **Do** use the in-house inline SVG icons, drawn in `muted` and lilac only when active.

### Don't:
- **Don't** use Thunder Lilac for decoration, large fills or more than one competing element per screen.
- **Don't** put shadows on panes, rows or anything that does not float.
- **Don't** introduce fonts beyond Geist and Geist Mono.
- **Don't** use web-style effects (gradients, blur, glow); the GUI is native iced.
- **Don't** make selection lilac; selection stays Selection Blue.
