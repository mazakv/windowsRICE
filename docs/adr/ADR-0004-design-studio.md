# ADR-0004: Design Studio architecture (Phase 6)

- Status: **Proposed** (implemented in Phase 6; awaiting owner review)
- Builds on ADR-0002 (rendering and UI Automation) and ADR-0003 (bar, configuration schema v3).

## Decisions

### 1. A window inside the bar process, on the existing stack

| Option | Result |
|---|---|
| **Studio window in the bar process, drawn by the existing renderer and UI model** | One process, one configuration owner, no new dependencies. Apply updates the running bars directly. The UI Automation provider, keyboard model and text stack are reused. **Chosen.** |
| Separate settings executable | Two processes would write `config.toml`, so it would need locking and a reload protocol, and its look would differ from the bar. |
| WinUI 3, WebView2 or another UI framework | Rejected in ADR-0002 for start-up cost, memory and dependency size; nothing in the Studio needs it. |

The Studio is created on demand: from the popup's **Open Design Studio** button, with `--studio`, or by a second `--studio` launch, which posts `WM_APP + 4` to the running controller and exits. It is destroyed when closed. While it is closed it costs nothing. While it is open it has its own Direct3D device (one more swap chain).

### 2. The preview is the real bar layout on a draft

The Studio edits a `Draft` (theme selection and the whole `BarConfig`). The preview calls the same `build_bar` used by the running bar, with the draft's theme resolved through the same catalog and `with_bar_style`, at the real monitor width. The result is drawn scaled for the whole bar and at actual size for the two ends and the middle, using clip and transform commands added to the draw list. So what the preview shows is what Apply produces; there is no second preview layout to drift out of sync.

### 3. Style overrides in configuration, not forked themes

Colour, opacity, backplate, border, separator, font and spacing choices are stored as optional `[bar.style]` overrides on top of the active theme (ADR-0003 §6a). Changing the theme keeps the user's adjustments, and an empty style means "exactly the theme". Full custom themes remain available through the Themes section, which writes ordinary theme files that the theme engine validates.

### 4. Draft, undo and Apply with rollback

Every Studio edit changes only the draft and records an undo step. Apply works in this order:
1. it validates the whole configuration;
2. it saves the bar section through `ThemeService::update_config` (atomic write, original of an older-schema file preserved first);
3. it selects the theme through `ThemeService::select`;
4. if the theme step fails, it restores the previous bar section, so `config.toml` never contains a half-applied change.

The running bars are then restyled and laid out in place.

### 5. Readability is a precondition, not a warning

If the draft's opaque text contrast is below 4.5:1, Apply is disabled and the reason is shown. Translucency risks that the legibility policy cannot fix (backplates turned off) are shown as warnings with the opacity that would fix them. This keeps the Phase 4 rule that readability is never sacrificed silently.

### 6. Versioned presets as separate files

A preset is a TOML file with `format = "windowsrice-preset"`, `version = 1`, a name, the theme selection and a complete `[bar]` table. It is validated with the same rules as `config.toml`.
- **Limits:** files over 128 KiB, of another format, or of a newer version are refused with a reason.
- **Storage:** presets live in `presets\` next to the configuration, named by a slug of the preset name (with a hash suffix for non-ASCII names).
- **Overwriting:** saving and renaming refuse to replace an existing preset, and importing asks before replacing one.
- **Dialogs:** import and export use the Windows common item dialogs, so no file paths are typed by hand.

### 7. Accessibility through the shared provider

Studio elements use the existing roles plus `Edit` with the UI Automation Value pattern (added in Phase 6). Setting a value through UI Automation stores the text and posts an action to the window, as Invoke and Toggle already do, so every change goes through the same code path as the keyboard and mouse.

Each drag-and-drop operation has a keyboard alternative in the property panel. A unit test asserts, for every section, three window sizes and both languages, that every interactive element has an action and that the tree passes the layout lint.

### 8. Destructive confirmations with native message boxes

Deleting a theme or preset, replacing a preset on import, resetting everything, and closing with unapplied changes use `MessageBoxW` owned by the Studio. They are shown outside the controller's borrow (as a deferred action), so the modal loop can still deliver messages to the bars.

## Consequences

- **Benefits:** one configuration owner, a preview that cannot diverge from the bar, no new dependencies, and the same accessibility model everywhere.
- **Costs:** the controller grows by the Studio module (`apps/panel/src/studio_win.rs`). The Studio's text fields are deliberately minimal: append and Backspace, with no caret, selection or paste.
- **Open:** Narrator, real DPI changes and multiple monitors with the Studio open are not verified (`docs/phase6-design-studio.md` §4).
