# Phase 6: windowsRICE Design Studio

Phase 6 adds the Design Studio: a resizable native window inside the running windowsRICE Bar where the bar's look, modules, themes, typography and layout are edited visually. The design decisions are in `docs/adr/ADR-0004-design-studio.md`. Evidence labels follow `docs/development.md`: unit, simulated, real (this Windows 11 Pro machine), and manual (not done until a person does it).

Test machine: Windows 11 Pro 25H2 build 26200.9974, Ryzen 9 5900X, Radeon RX 9070 XT, one 3840×2160 monitor at 160 Hz and 150%. Every automated run used a project-local configuration folder (`--config`). `%APPDATA%\windowsRICE` was never read or written by tests.

## 1. Opening the Studio

- **From the bar:** open the menu (click the windowsRICE button, or Ctrl+Alt+B then Enter) and choose **Open Design Studio**.
- **From a command line:** `target\release\windowsrice-panel.exe --studio`. This starts the bar with the Studio open. If the bar is already running, the new process asks the running bar to open the Studio and exits, so there is never a second bar.
- **Closing:** the window's close button or Alt+F4 closes the Studio, and the bar keeps running. If there are unapplied changes, the Studio asks first: **Yes** applies them, **No** discards them, and **Cancel** keeps editing.

## 2. What was built

The Studio is drawn by the same Direct2D/DirectWrite renderer as the bar and exposed through the same UI Automation provider. It is a standard resizable window (minimum 1180×760 DIPs) with a dark or light title bar. Its layout has five parts:
- a header with Undo, Redo, Revert, Apply and a status line;
- a sidebar with eight sections;
- a live preview card;
- the section's editing area;
- a property panel for the current selection.

**Live preview.** The preview runs the real bar layout (`windowsrice_ui::bar::build_bar`) on the draft at the width of the actual monitor, with the draft's theme resolved through the same `with_bar_style` function the bar uses. It shows two views:
- the whole bar scaled down;
- the left end, middle and right end at actual size.

It can be shown over a dark, a light or a colourful backdrop. The preview draws the latest real readings and never invents values: a reading that is unavailable is shown the way the bar shows it. Nothing reaches the desktop until **Apply**.

**Sections.**

| Section | What it edits |
|---|---|
| Overview | Summary of the current design and shortcuts to the other sections |
| Bar design | Mode (Windows setting, dark or light); background, accent and text colours (swatches or typed `#RRGGBB`); opacity or the theme's opacity; text backplates (when needed, always, never); bottom border; separators |
| Modules | Drag-and-drop editor with Left, Center and Right columns and a palette. Per module: show or hide, section, order (Earlier and Later), label, custom text, icon, refresh interval, maximum width, 12-hour clock, remove |
| Themes | All installed themes with swatches. Use a theme; duplicate any theme into an editable custom theme; edit its name, mode, background, accent, text, backdrop and opacity; save after validation; delete with confirmation |
| Typography | Font family (installed families only, marked when they lack Hangul), size 11–18, weight 400 or 500, labels on or off for all modules |
| Layout | Height, spacing, horizontal padding, vertical alignment |
| Presets | Save the draft as a preset; load, duplicate, rename, delete (with confirmation); import and export through the Windows file dialogs |
| Advanced | Screen-space reservation, auto-hide and its delay, the fullscreen policy, per-monitor on or off (stored with the stable monitor id), the Studio's own appearance, reset everything to the defaults (with confirmation) |

**Editing safety.**
- **Undo and redo:** each draft change records an undo step (up to 200), available as buttons and Ctrl+Z and Ctrl+Y.
- **Revert:** returns the draft to what the bar currently uses.
- **Reset:** sets the draft to the defaults after a confirmation, and still needs Apply.
- **Unsaved changes:** closing with unapplied changes asks first.
- **Readability:** a colour combination whose opaque text contrast is below 4.5:1 disables Apply and says why. When backplates are turned off and translucency could drop contrast below AA over a very light or dark background, the Studio warns and says what to change.

**Apply.**
1. The whole configuration (bar and theme selection) is validated first.
2. The bar settings are saved through `ThemeService::update_config` (validate, then write atomically; the original of an older-schema file is preserved first).
3. The theme selection is saved through `ThemeService::select`.
4. If the theme cannot be applied, the bar settings are written back to their previous values, and the status line reports both results.
5. On success, the running bars are restyled, the scheduler is reconfigured, bars are added or removed for the monitor settings, and every bar is laid out again. There is no restart.

When the bar was started with `--theme` or `--mode`, Apply changes only the running session and says so.

**Files.**
- **Configuration:** `config.toml` stays the one source of truth for the bar.
- **Custom themes:** saved in the user themes folder (`themes\` next to the configuration), validated with the theme engine's own rules before writing, and written through a temporary file and rename.
- **Presets:** saved in `presets\` next to the configuration, in a versioned format (`format = "windowsrice-preset"`, `version = 1`). A preset holds the theme selection and the whole `[bar]` section; files over 128 KiB, of another format or a newer version, and invalid ones are refused with a reason. Saving, renaming and importing never overwrite an existing preset silently: saving and renaming refuse, and importing asks.
- **Studio settings:** the last section, the Studio's appearance and its window position are kept in `studio.toml` next to the configuration.

**Accessibility and input.**
- **UI Automation:** the window is a Window element. Navigation items are ListItems with SelectionItem, buttons have Invoke, check boxes and switches have Toggle, radio buttons have SelectionItem, and text fields are Edit elements with the Value pattern. Module cards are ListItems named with their section and position (for example "CPU usage, Right section, position 7 of 12, shown").
- **Keyboard:** Tab, Shift+Tab and the arrow keys move focus; Space and Enter activate; Enter on a text field starts typing, Enter commits and Escape cancels. Every drag-and-drop move has a keyboard alternative (Earlier, Later and the section choice in the property panel).
- **Text entry:** characters arrive through `WM_CHAR`, so Korean typed with the Windows IME is accepted when each syllable is committed.
- **Languages:** Korean and English follow the bar's language. Long messages wrap.
- **DPI:** the Studio follows the DPI of its window and moves to the rectangle Windows suggests on `WM_DPICHANGED`.

## 3. Results

### Unit tests (unit)

- **`windowsrice-ui` Studio tests:** 5 tests. They cover every section at three window sizes (including the minimum) in both languages: the accessibility tree lint is clean, every interactive element has a working action, and exactly one navigation item is selected. They also cover module cards and drop slots, draft edits, drag moves and monitor toggles.
- **`windowsrice-core`:** preset format tests (round trip, oversize, wrong format, newer version, invalid bar) and the optional `[bar.style]` and module fields.

### Real application (real)

`scripts/studio-verify.ps1`: **42 checks, 0 failed** on the final release build (`docs/phase6/verify/checks.json`). The script drives the running application from another process:
- UI Automation for navigation, buttons, toggles, selection and typed values;
- mouse messages posted to the Studio window for drag and drop;
- the Windows message boxes and file dialogs through their own windows.

What it checks:
- **Opening:** `--studio` opens the Studio, and a second `--studio` launch opens it in the running bar instead of starting another bar.
- **Accessibility tree:** the root is a Window, there are eight named navigation items, and every element is named. The preview has a text summary of the modules it shows.
- **Sections:** every section opens and was captured (`docs/phase6/screens/`).
- **Colours:**
  - swatches and typed colours work;
  - an invalid colour is explained;
  - an unreadable text colour (1.0:1) disables Apply with an explanation;
  - undo and redo restore the draft.
- **Apply:** Apply wrote `background`, `accent` and `border_bottom` to `config.toml`, and the real bar on screen changed to the new background colour (pixel read from a screen capture).
- **Modules:**
  - a mouse drag moved CPU from the right section to the left;
  - Earlier and the section choice moved it with the keyboard alternatives;
  - a custom text module was added, given Korean text through the Value pattern, and removed.
- **Themes:** a built-in theme was duplicated into a custom theme file, renamed and recoloured, and saved after validation. Deleting it asked first and removed the file after Yes.
- **Presets:**
  - a preset was saved, loaded into the draft, and exported through the Save dialog;
  - a malformed file was refused on import with the parser's reason;
  - a valid file was imported into the presets folder.
- **Reset and close:**
  - reset asked first, changed only the draft, and did not touch `config.toml`;
  - closing with unapplied changes asked; Cancel kept the Studio open, and No closed it with the bar still running;
  - `studio.toml` recorded the last section.
- **Exit:** the bar exited cleanly with no errors and no accessibility lint problems.

**Visual (real).** Screenshots in English with dark appearance and in Korean with light appearance are in `docs/phase6/screens/`. Hangul renders with Malgun Gothic, long warnings wrap, and the actual-size preview crops match the bar.

**Bar regression (real).** `scripts/bar-a11y.ps1` and `scripts/phase45-verify.ps1` were re-run on the final binary (results in `docs/phase5-top-bar.md` §6).

### Level 3

`scripts/check.ps1 full` on the final code passed:
- fmt;
- check of all targets;
- Clippy with `-D warnings`;
- **223 tests passed, 0 failed**;
- the release build.

The Debug build of the workspace also succeeded. Release-mode tests were not run.

## 4. Not verified

These need a person or hardware this machine does not have, and are not claimed:
- **Narrator** reading the Studio. A background script cannot give the window the system foreground.
- **Typing with the Korean IME into a Studio field by hand.** Korean text was entered through the UI Automation Value pattern, which tests the same commit path but not the IME candidate window.
- **A real DPI change while the Studio is open**, and dragging it to a monitor with another DPI.
- **High-contrast mode** (turning it on changes a system setting).
- **Multiple monitors** (one monitor here), including per-monitor on/off with two real monitors.

Manual procedure:
1. Start the bar (`target\release\windowsrice-panel.exe`) and open the Studio from the menu.
2. Start Narrator (Ctrl+Win+Enter). Use Tab and the arrow keys through the sections; each item should be announced with its name and state.
3. In Modules, select a card and use Earlier, Later and the section choice. Narrator should announce the card's new position.
4. In Modules, add Custom text, focus its text field, press Enter and type Korean with the IME. Press Enter to commit. The preview should show the text.
5. Change Settings › Display › Scale while the Studio is open. The Studio should stay sharp and keep its layout.
6. Apply a change and check that the bar on every monitor updates.

## 5. Limitations

- **Text fields are simple.** Typing appends and Backspace deletes from the end. There is no caret movement, selection or clipboard paste. Text can also be set through UI Automation.
- **Font list.** The Typography section offers installed families from a fixed list of 14 common Latin and Korean families, not every installed font.
- **Theme editing** derives a complete palette from three colours (background, accent, text) for one mode at a time. Other palette entries keep derived values, and the theme validator checks every contrast pair before saving.
- **Glass preview.** The preview shows the draft over a chosen backdrop, but the bar itself shows the Acrylic fallback tint (see `docs/phase5-top-bar.md`), so translucency is not visible on screen.
- **GPU memory.** The Studio has its own Direct3D device while it is open.
- **Colour choice** is by swatches or typed hex values. There is no colour-picker dialog.

## 6. Status

The Studio described here was redesigned in Phase 8 (`docs/phase8-visual-design-studio.md`); Phase 7 found and fixed why its changes looked as if they did not reach the bar (`docs/phase7-live-design.md`)., with the items in §4 NOT VERIFIED. The work is uncommitted, in the same working tree as Phases 4 and 5.
