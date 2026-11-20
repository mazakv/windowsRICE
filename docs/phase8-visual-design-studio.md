# Phase 8: visual Design Studio

Phase 8 rebuilds the Design Studio around things people can see and click. Every design category has a gallery of cards that show the real result, the live preview is the real bar layout, and Apply is verified on the running bar (Phase 7). Decisions are in `docs/adr/ADR-0005-visual-design-studio.md`. Evidence labels follow `docs/development.md`.

Test machine: Windows 11 Pro 25H2 build 26200.9974, one 3840×2160 monitor at 150%. All automated runs used an isolated configuration and a fixed window title. Bar captures were taken over a pinned test backdrop, Studio captures contain only the Studio window, and no desktop content was recorded.

## 1. Opening the Studio

Use the bar's menu and choose **Open Design Studio**, or run `target\release\windowsrice-panel.exe --studio`. If the bar is already running, the second launch asks it to open the Studio and exits.

## 2. What was built

**Layout.** The window has five parts:
- a header with Undo, Redo, Revert, Apply and a status line (unapplied changes, blocking problems, and the verified result of Apply);
- a sidebar with twelve sections;
- a large live preview;
- a scrolling gallery area;
- a context property panel.

**Preview.** The preview is the real bar layout of the draft at the monitor's width. Six views are available:
- **All:** the whole bar plus three actual-size close-ups;
- **Whole:** the whole bar in two larger halves;
- **Left, Centre, Right:** one end at actual size or larger;
- **Module:** the selected module enlarged.

The preview can be shown over a dark, light or colourful backdrop.

**Cards.** Every card is a whole-card target: a list item with the selection pattern, hover, selected and keyboard-focus states, and a "Not available" state with the reason. Each card's picture is real:
- a miniature built by `build_bar` on a copy of the draft with the card applied;
- or the real gradient;
- or text set in the real font family.

The name sits directly below the picture.

| Section | Galleries and controls |
|---|---|
| Home | Current style, theme and font; cards for every section; readability warnings |
| Design styles | 15 complete looks: Minimal, Fluent, Glass, Dark Modern, Light Modern, Midnight, Monochrome, Colorful Accent, Soft Contrast, Compact, Premium, Futuristic, Editorial, Soft Rounded and High Contrast. They change theme, colours, background, type, icons and effects, never the module list |
| Themes | Essentials (Light, Dark, Glass Light, Glass Dark) and all 17 built-in themes plus custom ones, each as a miniature bar. Choosing applies immediately; duplicate, edit (name, mode, colours, backdrop, opacity), save after validation, delete with confirmation |
| Colours | 14 palettes with swatch strips and miniature bars. 13 colour roles plus each gradient colour, as cards showing their current colour. The editor has a large current swatch, 24 swatches, a hex field, red, green and blue steps, the Windows colour dialog, "theme colour" and recent colours |
| Background | 9 backgrounds (Solid, Translucent, Glass, Gradient, Dark translucent, Light translucent, Custom colour, High opacity; Frosted is shown as not available), 7 gradients, gradient direction, opacity slider, backplate policy, border |
| Typography | 8 typography presets shown as real samples, then every installed font: a sample in the font, Korean only when the font has Hangul, the family name below, Korean support and weight count, and search. The property panel offers size, clock size, the weights the family really has (300–600), labels and the 12-hour clock |
| Icons | 6 icon styles (Fluent, Classic, Accent, Colourful, Monochrome, Large), icon size, custom icon colour |
| Modules | 10 module appearances (icon only, value only, icon and value, icon, label and value, compact, expanded, pill, flat, separated, minimal), then the drag-and-drop module editor with keyboard alternatives |
| Layout | 9 arrangements (Balanced, Left focus, Centered, Right focus, Symmetrical, Compact, Wide spacing, Split regions, Minimal information), sliders for height, spacing, padding and group spacing, vertical alignment |
| Effects | 11 effects (bottom border, separators, shadow, highlight, strong hover, selection colour, focus outline (always on), rounded tiles, flat tiles, subtle depth, accent indicator), with sliders for border, separator, radius, tile opacity, shadow, highlight and hover |
| Presets | Save, load, duplicate, rename, delete, import and export complete designs (format version 2; version 1 files load) |
| Advanced | Screen space, auto-hide, fullscreen, monitors, Studio appearance, reset |

**Fonts.** Installed families are read from DirectWrite's typographic system collection when the Studio opens: 200 families, of which 188 are offered for text (symbol fonts are left out), in 8–15 ms. Duplicate names are merged, and the weight list and Hangul support (가 and 한) are read from the faces. Samples use `FontRole::Family`, so they are drawn in the real family. A font without Hangul shows only Latin text and says that Korean uses Malgun Gothic; the bar's Hangul fallback is unchanged.

**Editing.**
- **Text fields:** caret movement (Left, Right, Home, End, with Shift to select), Delete, Backspace, Ctrl+A, Ctrl+C, Ctrl+X, Ctrl+V (clipboard), Enter commits, Escape cancels.
- **Sliders:** drag or click the track, or use the arrow keys, Home and End. They are exposed as UI Automation Slider elements with a value.
- **Galleries:** move with the arrow keys and choose with Space or Enter. The view scrolls with the mouse wheel and Page Up and Page Down; off-screen cards are left out of the accessibility tree and are not drawn.

**Configuration.**
- **New settings:** the new appearance settings are optional fields of `[bar.style]` (material, gradient, ten more colour roles, module shape, opacity and radius, content, density, icon set, tint and size, accent line, shadow, highlight, hover, group spacing, clock size; font weight now 300–600).
- **Compatibility:** fields that hold their default are not written, so schema v3 files from earlier phases stay valid and unchanged.
- **Validation:** every field is validated, and colour choices that drop any text colour below 4.5:1 on any surface it sits on block Apply with an explanation.

**Themes.** Nine built-in themes were added (Midnight, Soft Gray, Blue Accent, Monochrome, Ocean, Neon, Pastel, Warm, High Contrast), generated from seed colours by `scripts/gen-themes.ps1`. They are validated by the theme engine's full contrast rules, giving 17 built-in themes.

## 3. Results

### Unit and integration (unit)

- **`windowsrice-ui` Studio tests:** 11.
  - Every section at three window sizes, both languages and three scroll positions: the accessibility tree lint is clean and every interactive element has an action.
  - Galleries show real cards with the right actions and selection.
  - Font cards use the real family and never draw a Korean sample for a font without Hangul; search filters.
  - Scrolling leaves off-screen cards out of the tree.
  - Sliders snap and clamp.
  - Colour roles and theme cards; text editing with caret, selection and limits; drag and drop; draft edits.
- **Catalog tests:** 5. Every card is valid and selected after applying it; styles do not touch modules; effects toggle; layouts place modules; palette text meets 4.5:1 on both of its surfaces.
- **Font enumeration on this system (real):** 200 families, Malgun Gothic with Hangul, Segoe UI Variable without.
- **Configuration:** Phase 8 fields round-trip and are validated, defaults are not written, and version 1 presets load.

### Real application (real)

- **`scripts/design-e2e.ps1`: 26 checks, 0 failed.** This is the end-to-end Apply test (style, palette, font, background, module appearance, layout, icons, effects, preset, restart); see `docs/phase7-live-design.md` §4.
- **`scripts/studio-verify.ps1`: 51 checks, 0 failed.**
  - all 12 sections;
  - colours, invalid and unreadable colours, undo and redo, Apply verified with the exact pixel on the real bar;
  - drag and drop and its keyboard alternatives;
  - the theme lifecycle;
  - presets with the Windows dialogs;
  - reset and unsaved close;
  - second launch;
  - keyboard: grid arrows, Space, slider arrows, caret editing.
- **`scripts/bar-a11y.ps1`: 59 checks, 0 failed.** The popup now lists 17 themes.
- **`scripts/phase45-verify.ps1`: 15 checks, 0 failed.** Text on Glass is at least 15:1 on backplates over black, white and colourful backdrops.
- **`scripts/studio-shots.ps1`:** every section in English (dark) and Korean (light), in `docs/phase8/screens/`.
- **Performance (real, release):**
  - Studio scene build: median 2.4 ms and maximum 9.3 ms in English; median 5.7–6.2 ms and maximum 16 ms in Korean (Hangul fallback shaping), over about 110 rebuilds each;
  - font discovery: 8–10 ms once per opening;
  - no font enumeration per frame and no device re-creation for edits.

### Level 3

fmt, check, Clippy with `-D warnings`, **237 tests passed, 0 failed**, Release and Debug builds (§6).

## 4. Limitations

- **No blur.** Translucent backgrounds show what is behind the bar without blur. Windows does not blur a window that never activates, and the bar does not capture other windows. "Frosted" is therefore shown as not available.
- **Icon families.** Only icon fonts shipped with Windows are offered: Segoe Fluent Icons and Segoe MDL2 Assets. Duotone, geometric and similar icon families are not installed and not offered.
- **Typography.** Letter spacing, line spacing and tabular figures are not offered: the bar is one line and these were not implemented. Weights are limited to 300–600 and to the weights the family has.
- **Slider accessibility.** Sliders expose a Value, not the RangeValue pattern. A screen reader reads "88%", and the value can be set by typing a number.
- **Gradients** apply to the bar surface. Module tiles use one colour.
- **The theme editor** still edits three colours per mode (background, accent, text). Every other colour role can be overridden per design in Colours.
- **Clipboard shortcuts** (Ctrl+C, X, V, A) are unit-tested on the editing model. In the real window they could not be driven by posted messages (the Ctrl state cannot be posted), so they are not verified in the running app.

## 5. Not verified

- Narrator reading the galleries and cards.
- A real DPI change while the Studio is open.
- High-contrast mode.
- Multiple monitors.
- Typing Korean with the IME by hand.
- Clipboard shortcuts in the running window.

Procedure: start the bar, open the Studio, start Narrator (Ctrl+Win+Enter), then move through the sections with Tab and through a gallery with the arrow keys. Each card should be announced with its name, description and selection state. Press Space to choose a card, then Ctrl+S to apply, and check the bar.

## 6. Status

Level 3 on the final code (2026-11-19, `scripts/check.ps1 full`): fmt, check of all targets, Clippy with `-D warnings`, **237 tests passed, 0 failed**, release build; the Debug build of the workspace also succeeded. Release-mode tests were not run.

**Phase 8 status: COMPLETE**, with the items in §5 NOT VERIFIED.
