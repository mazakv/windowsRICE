# Phase 7: design changes that really reach the bar

Phase 7 investigated the report that changing designs in the Design Studio did not visibly change the running bar, found the causes, fixed them, and added a test that checks the real bar instead of the configuration file. Evidence labels follow `docs/development.md`.

Test machine: Windows 11 Pro 25H2 build 26200.9974, one 3840×2160 monitor at 150%. Every automated run used a project-local configuration (`--config`) and a fixed window title. The owner's real configuration was only read, never written.

## 1. Reproduction

**What the owner had done.** The owner's own configuration and its backup were read (read-only). They showed one Apply from the Phase 6 Studio: the theme changed from windowsRICE Glass to windowsRICE Ambient, with no colour overrides.

**Probe (real).** `scripts/apply-probe.ps1` then reproduced the situation with an isolated configuration on the Phase 6 build. For each of 22 settings, it changed the setting through UI Automation, pressed Apply, and captured the real bar before and after.
- **Result:** the configuration and the bar changed for every setting (`target/apply-probe/`).
- **Caveat:** live values (CPU, network, clock) also differ between captures, so the before/after images were compared by eye as well.
- **What the probe proved:** the save and reload path worked.

The visible problem was elsewhere.

## 2. Root cause

Four causes together made design changes look as if they did nothing.

1. **Windows replaced translucent theme colours with a flat grey (main cause).** The bar never activates (`WS_EX_NOACTIVATE`), and Windows draws the system Acrylic or Mica backdrop of such windows as a flat fallback tint. The bar drew the theme surface at the theme's opacity *over that tint*. A translucent theme's colour was therefore largely replaced by the same grey.
   - **Measured:** Glass asks for a #191D25 surface at 55% opacity. On screen the bar was #34363A over a black backdrop and the same #34363A over a white one.
   - **Effect:** Glass, Ambient, Default and HYPR all looked like slightly different dark greys.
2. **Themes differ mostly in their accent colour, which an idle bar does not show.** The accent appeared only in the menu mark and the focus ring, so even opaque dark themes were hard to tell apart.
3. **Clicking a theme card in the Phase 6 Studio only inspected it.** The preview and the draft changed only after a second click on "Use this theme". Colour overrides from earlier edits also stayed in force and could hide the theme completely.
4. **The preview and the bar used different transparency.** The Studio preview blended the translucent surface over a sample backdrop, but the real bar showed the Windows fallback grey. The preview therefore promised differences the bar could not show.

The probe also showed that changing the **accent** colour left the idle bar unchanged, which is cause 2 again.

## 3. Fix

- **Real translucency, drawn by the bar.** Bars no longer request a system backdrop (`DWMSBT_NONE`). Translucency is drawn by the bar itself with per-pixel alpha through DirectComposition, so what is behind the bar really shows through, without blur.
  - **Verified (real):** with a test window behind the bar, Glass at 55% measured #0E1014 over black and #818387 over white, the arithmetic values for 55% Glass, and showed the colourful backdrop through over a gradient.
  - **Materials:** the new `[bar.style] material` setting chooses `theme`, `solid` or `translucent`, through one function (`windowsrice_ui::style::bar_surface`) shared by the bar and the Studio.
- **Readability kept.** The legibility policy is unchanged: when translucency could drop text below AA, text gets opaque backplates.
  - **Verified (real):** text on Glass measured at least 15.17:1 over black, white and colourful backdrops (`scripts/phase45-verify.ps1`).
  - **Gaps:** the gaps between modules can be lower (3.41:1 over white), but no text is drawn there.
- **Accent visible on the bar.** The accent now appears where people can see it, through an accent line, accent or colourful icons and tiles (Phase 8 effects and icon styles). Theme cards in the Studio now show each theme as a miniature real bar.
- **Theme cards apply immediately.** Choosing a theme card puts the theme and mode into the draft at once and clears colour overrides, so the theme's own colours show. Undo restores the overrides. Fonts, layout and effects stay.
- **One preview logic.** The Studio's previews (the main preview and every gallery card) resolve their draft with the same functions as the bar: `resolve_bar_theme`, `with_bar_style`, `bar_surface`, `plan`, `Decor::from_style` and `build_bar`. The preview backdrop is just what is behind the bar, which is now also how the real bar composites.
- **Apply confirms the running bar.** After saving, Apply now:
  1. restyles and lays out the running bars;
  2. compares what the bar *uses* (theme id, font family and size, surface, text and accent colours) with what the draft resolves to;
  3. reports either "Verified on N bar(s): theme · font size", or a warning naming what differs.

  The comparison is also written to the diagnostics log (`studio apply verified: … differs=[]`).

## 4. Results (real)

`scripts/design-e2e.ps1`: **26 checks, 0 failed** (`docs/phase8/e2e/checks.json`). Every bar capture was taken over a test backdrop pinned behind the bar (checked before each capture) with a fixed title, so no desktop content was recorded.

| Step | Checked on the real bar | Result |
|---|---|---|
| Style "Light Modern" | Surface pixel | #EBEDF1 (light); Apply verified on 1 bar |
| Palette "Ocean Blue" | Surface pixel | exactly #0E2A3F |
| Font "Cascadia Code" | Bar text pixels changed; verified font name | changed; "Cascadia Code 13" |
| Background "Translucent" | Surface pixel over the backdrop | #0E2A3F → #0B2232 (backdrop shows through) |
| Module appearance "Icon + label + value" | Right side of the bar | changed |
| Layout "Right focus" | Bar's own UI Automation tree | clock moved from region.center to region.right |
| Icons "Colourful" + effect "Accent indicator" | Bottom edge pixel | #4FB3FF (Ocean Blue accent) |
| Save preset | Preset file | complete design saved |
| Restart | Bar pixels, UI Automation, Studio selection marks | design kept, clock still right, cards still selected, config unchanged |

Seven consecutive Applies in one session each logged `differs=[]` (no difference between the draft and what the bar uses). No restart was needed for any of them.

## 5. Not verified

- Translucency over other applications' windows with blur: not provided (no blur is drawn).
- Multiple monitors (one monitor here): each bar applies the same style; not verified with two.
- Narrator: not verified (see `docs/phase8-visual-design-studio.md`).

## 6. Status

**Phase 7 status: COMPLETE.** The cause is documented, the fix is in place, and Apply is verified against the running bar. Multiple monitors and Narrator remain NOT VERIFIED.
