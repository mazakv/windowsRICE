# RICE Design Code

RICE Design Code is the text format for sharing a complete windowsRICE design. It is JSON, versioned, and **strictly declarative**: it describes colours, typography, modules and layout, and nothing in it is ever run. A design package (`.ricepack`) carries the same code together with images and videos.

Use it in the Design Studio:
- **My designs › Import design code:** paste code or open a `.json` or `.ricepack` file, choose **Analyze**, fix any reported problems (the line editor edits the line of the selected problem), then **Preview in Studio**, **Save to My designs** or Apply.
- **My designs:** **Copy design code** and **Export .ricepack…** share your current design.

## Top level

```json
{
  "format": "rice-design",
  "version": 1,
  "name": "Neon evening",
  "author": "optional",
  "description": "optional",
  "theme": { "active": "windowsrice-neon", "mode": "dark" },
  "top_bar": { },
  "bottom_bar": { },
  "assets": [ ]
}
```

| Field | Required | Meaning |
|---|---|---|
| `format` | yes | Always `"rice-design"`. |
| `version` | yes | `1`. A newer version is refused with an explanation. |
| `name` | yes | Name shown in My designs (up to 400 characters, as every text value). |
| `author`, `description` | no | Text. |
| `theme` | yes | `active`: a theme id such as `windowsrice-default`; `mode`: `auto`, `dark` or `light`. An uninstalled theme is a warning (windowsRICE Default is used). |
| `top_bar` | yes | Exactly the `[bar]` section of `config.toml` (below). |
| `bottom_bar` | no | The bottom bar, same fields as `top_bar` (Phase 14). |
| `assets` | no | Images and videos of a `.ricepack` (at most 8). |

Unknown fields are errors everywhere: only the documented fields are accepted.

## Bar (`top_bar`, `bottom_bar`)

| Field | Values |
|---|---|
| `height` | Height in DIPs (24–64); omit for the theme's height. |
| `spacing` | Gap between modules, 0–24. |
| `reserve_work_area`, `auto_hide` | `true` / `false`. |
| `auto_hide_delay_ms` | 200–5000. |
| `fullscreen` | `"hide"` or `"keep"`. |
| `monitors` | `"all"` or `"primary"`. |
| `animations` | `true` / `false` (short fades). |
| `style` | Appearance (below). |
| `modules` | The modules in display order (below). |

### `style`

Every field is optional; a missing field uses the theme. Colours are `#RRGGBB`.

- **Colours:** `background`, `accent`, `text`, `text_secondary`, `clock_color`, `icon_color`, `value_color`, `border_color`, `separator_color`, `hover_color`, `selected_color`, `focus_color`, `module_background`.
- **Surface:** `material` (`"theme"`, `"solid"`, `"translucent"`), `opacity` (0.3–1.0), `gradient` (`{ "direction": "horizontal" | "vertical" | "diagonal", "colors": ["#..", "#.."] }`, 2–4 colours), `background_asset` (an asset id, see below).
- **Typography:** `font_family`, `font_size` (11–18), `font_weight` (300–600), `clock_size` (11–22).
- **Modules:** `module_shape` (`"none"`, `"rounded"`, `"pill"`, `"square"`), `module_opacity` (10–100), `module_radius` (0–16), `content` (`"icons"`, `"values"`, `"full"`), `density` (`"compact"`, `"normal"`, `"expanded"`).
- **Icons:** `icon_set`, `icon_tint` (`"text"`, `"secondary"`, `"accent"`, `"colorful"`, `"custom"`), `icon_size` (−2–6).
- **Effects:** `border_bottom`, `border_width` (1–4), `separators`, `separator_width` (1–3), `accent_line`, `shadow` and `highlight` (0–100), `hover_intensity` (10–100), `region_gap` (0–200), `padding_x` (0–48), `vertical_align`.
- **`animations`:** colour animations (at most 4, one per target):

```json
{
  "target": "text_gradient",
  "keyframes": [
    { "at": 0.0, "color": "#FF5A5A" },
    { "at": 0.5, "color": "#4DABF7" }
  ],
  "duration_ms": 8000,
  "interpolation": "linear",
  "direction": "normal",
  "wrap": true,
  "enabled": true
}
```

`target` is `text`, `text_gradient` (rainbow text), `accent`, `icons`, `clock` or `background`. `at` is 0.0–1.0 in increasing order; at most 16 keyframes. `duration_ms` is 1000–120000: shorter cycles are refused because fast colour changes can trigger photosensitive reactions. `interpolation` is `linear`, `smooth` or `step`; `direction` is `normal`, `reverse` or `alternate`; `wrap` blends the last keyframe back into the first for a seamless loop. Animations stop when the bar is hidden and follow the Windows "Animation effects" setting.

Readability is enforced when drawing: a text colour without 4.5:1 contrast (3:1 for icons) on its background is shown as the nearest readable colour, and the analyzer and the Studio report it.

### `modules`

```json
{ "id": "cpu", "kind": "cpu", "region": "right", "enabled": true, "interval_ms": 2000 }
```

`kind` is one of `menu`, `clock`, `cpu`, `gpu`, `vram`, `ram`, `volume`, `network`, `upload`, `download`, `active_window`, `battery`, `notifications`, `custom_text` and `custom_icon`. `region` is `left`, `center` or `right`. Ids are unique; each kind appears once except `custom_text` and `custom_icon`. Optional per-module fields: `text`, `icon` (a Segoe Fluent Icons code point such as `"E734"`), `label`, `show_label`, `max_width`, `content`, `color`, `background`, `font_size`, `density`, `value_format`, `speed_unit`, `show_date`, `hour12`.

## Assets and `.ricepack`

```json
"assets": [
  { "id": "sky", "kind": "image", "file": "assets/sky.png" },
  { "id": "waves", "kind": "video", "file": "assets/waves.mp4",
    "trim_start_ms": 1000, "trim_end_ms": 9000, "poster_ms": 2500, "loop": true, "speed": 1.0 }
]
```

and `"top_bar": { "style": { "background_asset": "sky" } }`.

A `.ricepack` is a ZIP archive with `design.json`, an optional `preview.png` and the asset files under `assets/`. windowsRICE writes packages with **stored** (uncompressed) entries and accepts only those: PNG, JPEG and MP4 are already compressed, and an archive can then never expand beyond its own size.

When a package is opened:
- every entry name is checked: no absolute paths, drive letters, backslashes, `..`, folders other than `assets/`, symbolic links or duplicates;
- every entry's CRC-32 is verified;
- each file is identified by its **content** (PNG and JPEG signatures, the MP4 `ftyp` box), not its extension; anything else, including executables and scripts renamed to `.png`, is refused;
- limits are enforced: 64 MB per package, 16 files, 16 MB per image (up to 7680×4320), 48 MB per video (up to 3840×2160 and 120 s);
- assets are copied into the design library (`library\` next to `config.toml`) under generated, content-derived names; names from the package are never used as paths.

SVG and other formats are not supported, so they need no sanitising.

## What the analyzer checks

Syntax (with line and column), format and version, unknown and missing fields, value types, duplicate ids, references to assets, colours, value ranges, installed fonts and themes, animation limits, the number of modules turned on, readability, text length and size limits. It refuses script and command content (`<script`, `javascript:`, `powershell`, `cmd.exe`, `eval(` and similar), web and file addresses, absolute or parent-directory paths and executable file types anywhere in the code. Each finding has a severity, the location (path, line and column), an explanation and a suggested correction.

The results are stated plainly: *Syntax passed*, *Schema passed*, *No executable content accepted*, *Asset checks passed* (or *not checked* for pasted code), *Antivirus scan* (*not checked*, *passed*, *failed* or *unavailable*) and *Review required*. For an opened package, **Defender scan** runs Microsoft Defender's command-line scanner on the file when it is installed (read-only; nothing is quarantined by windowsRICE). A static check cannot prove that a file is free of malware, and windowsRICE never labels a design as such: review the preview before you apply it.

## Example

```json
{
  "format": "rice-design",
  "version": 1,
  "name": "Sunset",
  "theme": { "active": "windowsrice-warm", "mode": "dark" },
  "top_bar": {
    "height": 36,
    "spacing": 4,
    "style": {
      "material": "solid",
      "gradient": { "direction": "horizontal", "colors": ["#8C2F39", "#6A2453", "#3E1F57"] },
      "text": "#FFFFFF",
      "clock_color": "#FFE3C2",
      "module_shape": "pill",
      "module_background": "#000000",
      "module_opacity": 18,
      "animations": [
        {
          "target": "clock",
          "keyframes": [
            { "at": 0.0, "color": "#FFE3C2" },
            { "at": 0.5, "color": "#FFC2D6" }
          ],
          "duration_ms": 6000,
          "interpolation": "smooth",
          "direction": "alternate",
          "wrap": true,
          "enabled": true
        }
      ]
    },
    "modules": [
      { "id": "menu", "kind": "menu", "region": "left", "enabled": true },
      { "id": "active-window", "kind": "active_window", "region": "left", "enabled": true },
      { "id": "clock", "kind": "clock", "region": "center", "enabled": true },
      { "id": "volume", "kind": "volume", "region": "right", "enabled": true },
      { "id": "notifications", "kind": "notifications", "region": "right", "enabled": true }
    ]
  }
}
```
