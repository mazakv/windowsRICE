# ADR-0005: Live design application and the visual Design Studio (Phases 7 and 8)

- Status: **Proposed** (implemented; awaiting owner review)
- Amends ADR-0003 (bar surface) and ADR-0004 (Studio structure).

## Decisions

### 1. The bar draws its own translucency; no system backdrop

| Option | Result |
|---|---|
| System Acrylic or Mica backdrop (Phases 4–6) | Windows draws the backdrop of a never-activated window as a flat fallback tint. Translucent themes all became the same grey (measured: Glass #34363A over both black and white). |
| Activating the bar so Acrylic works | Would steal focus from games and other applications. Rejected. |
| Capturing the desktop and blurring it ourselves | Records other applications' content and costs GPU time. Rejected. |
| **No system backdrop; per-pixel alpha through DirectComposition** | What is behind the bar shows through at the chosen opacity, without blur. The colour is the theme's own, and the preview matches. **Chosen.** |

`[bar.style] material` (theme, solid, translucent) is mapped by `windowsrice_ui::style::bar_surface`, used by the bar and the Studio. The legibility policy (backplates when the worst case is below AA) is unchanged. The popup and the Studio, which do activate, keep their system backdrops.

### 2. Apply is verified against the running bar

After saving, Apply compares what the running bar uses (theme id, font family and size, surface, text and accent colours) with what the draft resolves to. It reports the verified result, or a warning naming what differs. A saved file alone is not reported as success.

### 3. Every gallery card is a pure edit of the draft

`windowsrice_ui::studio::catalog` defines styles, palettes, gradients, backgrounds, typography, icons, module looks, layouts and effects as functions over the draft (theme selection plus `[bar]`). A card is shown as selected exactly when applying it would change nothing, so selection marks cannot drift from the settings. Each card touches only its subject; the single exception (the "Minimal information" layout turns modules off) says so on the card.

### 4. Card pictures are the real thing

Card previews call `build_bar` on a copy of the draft with the card applied, resolved by the same functions as the running bar, and draw it with the draw-list transform. Font cards use a per-family font role (`FontRole::Family`) and the DirectWrite system collection, so samples are set in the real typeface. A sample in Korean is drawn only for families that contain Hangul.

### 5. Scrolling with virtualization

The section area scrolls. Elements outside the visible area are left out of the accessibility tree and are not drawn, so large galleries (about 190 fonts) stay cheap: the median scene build is 2.4 ms. Gallery grids publish their card rectangles, so the arrow keys move through every card, scrolling it into view, even when it is not yet in the tree.

### 6. Visual settings are optional configuration fields

All Phase 8 appearance settings are optional `[bar.style]` fields that are not written when they hold their default. Schema v3 files from earlier phases stay valid and unchanged. Preset files move to format version 2, and version 1 files load.

## Consequences

- **Benefits:**
  - the bar shows each theme's real colour;
  - previews and cards are faithful to the bar;
  - every visual option is something people can see before choosing;
  - Apply proves its result.
- **Costs:**
  - no blur behind the bar;
  - more drawing code in `build_bar` (shapes, gradients, effects, icon tints);
  - the Studio module grew by a catalog and a gallery module.
- **Open:** Narrator, real DPI changes and multiple monitors are not verified with the new Studio.
