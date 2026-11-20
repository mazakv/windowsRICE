# ADR-0002: UI rendering technology for windowsRICE desktop surfaces

- Status: **Accepted** (owner approval, Phase 3.5 acceptance, 2026-11-19)
- Decision: **Direct2D + DirectWrite + DirectComposition on native Win32 windows, with a UI Automation provider designed in from the start.**
- Evidence: Phase 3.5 spike in `experiments/ui-spike` (benchmark run `results/bench/20261118-224822`), and the Phase 4 foundation measurements in `docs/benchmarks/2026-11-19-panel/`.

## 1. Context

windowsRICE needs always-on desktop surfaces: a top panel, a dock, a launcher, widgets and later Theme Studio. They must be nearly free when idle, render Korean and English text correctly at fractional scaling, honour the design tokens (Segoe UI Variable with Malgun Gothic, weights 400/500/600), use DWM materials such as Acrylic, stay smooth at 160 Hz and be fully usable with a keyboard and a screen reader. Phase 3.5 compared Slint 1.19 (FemtoVG and software renderers) with a hand-built Direct2D pipeline using identical 34 px top-bar prototypes.

## 2. Spike evidence (Phase 3.5)

The test machine was Windows 11 Pro 25H2, build 26200.9974, with a Ryzen 9 5900X, a Radeon RX 9070 XT (driver 32.0.32015.7029) and an MSI MAG323UPF at 3840×2160, 160 Hz and 150% scaling. Windhawk 1.7.3 and the AMD driver inject DLLs into the measured processes. Each configuration had 3 idle repetitions (5-minute warm-up, 10-minute window), 10 startups and a 60-second interaction run. Values are medians with min–max.

| | Slint FemtoVG | Slint software | Direct2D |
|---|---|---|---|
| First frame after process creation | 437 ms (430–458); 2.2 s cold | unavailable (no frame callback) | 119 ms (118–134) |
| Idle CPU | below the resolution of the method | same | same |
| Working set / private | 218 / 259 MiB | 22 / 4 MiB | 47 / 38 MiB |
| Dedicated / shared GPU memory | 124 / 109 MiB | 0 / 0 (DWM-held, unattributed) | 19 / 5.5 MiB |
| 160 Hz frame pacing | 6.10 ms median, 1 late frame of 897 | unavailable | 6.24 ms median, 4 late frames of 981 |
| Hangul at 125% and 150% | **corrupted** ("문서" read as "문시", "파일" as "피일") | correct | correct |
| Exact weight 500 | no | no | yes (variable-font axis) |
| Acrylic | not achieved | not achieved | works |
| UI Automation | 16 elements, named buttons | same | **one blank pane** |

Licensing: Slint 1.19 is `GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0`. The royalty-free licence requires attribution (AboutSlint widget or the public download-page badge; Phase 0 wrongly called this optional) and excludes applications that expose Slint's APIs, which is relevant to community themes. The Direct2D path uses only `windows-rs` (MIT/Apache-2.0) and operating-system components.

## 3. Decision and reasons

**Direct2D, DirectWrite and DirectComposition are the production rendering stack.**

- **Slint FemtoVG is rejected** for production. Hangul is visibly corrupted at 125% and 150% on the actual target display, which violates a critical requirement. It also uses 5–7× the memory of Direct2D per surface, starts 3.7× slower, and Acrylic was not achieved with it.
- **Slint's software renderer is not selected as a fallback.** It renders Hangul correctly and is small, but Acrylic was not achieved and it cannot report frames, so smoothness at 160 Hz could not be verified. Its CPU raster cost for large surfaces at 4K is unmeasured, and it carries the same licensing conditions.
- **Direct2D met every measured requirement except accessibility.** That gap is engineering work under the project's control, so the decision is conditional on building accessibility into the architecture rather than adding it later. Phase 4 implemented that architecture (§4.5).

A different framework is reconsidered only if a concrete technical blocker is found and documented.

## 4. Architecture (Phase 4)

| Layer | Crate or module | Responsibility |
|---|---|---|
| Theme data | `windowsrice-theme` (Phase 3) | Tokens, validation, activation, fallback; `ThemeService::reapply` follows Windows light/dark changes without writing |
| UI model | `windowsrice-ui` (no `unsafe`, no Windows APIs) | Layout, element tree, focus order, keyboard navigation, draw list, surface legibility policy, placement, strings |
| Rendering | `windowsrice-render` | D3D11/DXGI/D2D/DComp device and surface, DirectWrite text system, painting, device-loss recovery |
| Windows state (read-only) | `windowsrice-windows::live` | CPU, memory, GPU counters, audio volume, connectivity, foreground window, fullscreen state, monitors, appearance settings |
| Application | `apps/panel` | Win32 window and message loop, DPI and display changes, input, UI Automation provider, sampler thread, diagnostics |

### 4.1 One scene, two consumers

The UI model builds a `Scene` in a single layout pass. It holds the accessibility element tree (role, name, help, bounds, focusable, enabled, selected and expanded state, stable ids) and the draw list. Every draw command names the element that owns it. The renderer paints the draw list, and the UI Automation provider reads the tree. Because both come from the same rectangles, what is drawn and what assistive technology sees cannot drift apart, and tests assert this (every draw command lies inside its owner's bounds, and every focusable element draws something).

### 4.2 Rendering pipeline

1. A D3D11 device with BGRA support on the hardware adapter, falling back to WARP if no hardware device can be created.
2. A DXGI swap chain for composition: flip-sequential, two buffers, premultiplied alpha.
3. A D2D device context targeting the back buffer, with the context DPI set explicitly. The target bitmap's DPI does not change the context DPI; Phase 4 found this on the real display, where content was first drawn at 1× in a 1.5× window.
4. A DirectComposition target and visual on a `WS_EX_NOREDIRECTIONBITMAP` window, with an effect group whose opacity DWM animates for the show fade (ease-out cubic over the theme's `duration_fast_ms`, disabled by motion level Off or by Windows' "Animation effects" setting).
5. DWM: immersive dark mode by theme mode, rounded corners, and the theme's system backdrop (Acrylic is `DWMSBT_TRANSIENTWINDOW`, Mica is `DWMSBT_MAINWINDOW`) with the frame extended into the client area.

Frames are event-driven. A frame is drawn only when the rebuilt draw list or tree differs from the previous one. Nothing is laid out or drawn while the panel is hidden.

### 4.3 DirectWrite typography

- **Font model.** Fonts are looked up in the typographic system collection (`IDWriteFactory6`, `DWRITE_FONT_FAMILY_MODEL_TYPOGRAPHIC`). In the classic collection, "Segoe UI Variable" exists only as Text, Display and Small faces.
- **Fallback.** A custom fallback maps Hangul Jamo, compatibility Jamo, syllables, the extended Jamo blocks, CJK punctuation and full-width forms to the theme's fallback families (Malgun Gothic), and then defers to the system fallback. A real-machine test asserts that Hangul resolves to Malgun Gothic and Latin text to Segoe UI Variable.
- **Weights.** The weight axis is set to the exact token value (`IDWriteTextFormat3::SetFontAxisValues`). Without it, DirectWrite snaps 500 to Semibold. A test asserts that 400, 500 and 600 produce three distinct widths. Malgun Gothic has no 500 face, so Hangul at 500 uses its Regular face.
- **Antialiasing.** Text is grayscale-antialiased, because ClearType needs an opaque target.

### 4.4 DPI, monitors and placement

The process is Per-Monitor-V2 aware. The panel is placed inside the monitor's work area, so it never covers the taskbar, using the theme's bar height, margin and floating flag, rounded to whole pixels. Layout is in DIPs and every element rectangle is snapped to the physical pixel grid. The panel handles `WM_DPICHANGED`, `WM_DISPLAYCHANGE` and `WM_SETTINGCHANGE` by re-enumerating monitors, recomputing placement and resizing the swap chain. A missing monitor falls back to the primary one. `--scale` forces a UI scale for testing; this is simulated DPI, not a Windows DPI change.

### 4.5 Accessibility architecture

- **Providers.** A fragment-root provider is hosted on the panel HWND (`WM_GETOBJECT`, `UiaRootObjectId`). Element providers are created on demand for stable element ids. Both are `ServerSideProvider | UseComThreading` and live on the window's STA thread.
- **Properties.** Exposed properties are ControlType (ToolBar, Group, Button, Text, MenuItem, ListItem), Name, AutomationId, HelpText, IsKeyboardFocusable, HasKeyboardFocus, IsEnabled, IsOffscreen, FrameworkId (`windowsRICE`), BoundingRectangle in physical screen pixels, RuntimeId and hit testing.
- **Patterns.** Invoke, ExpandCollapse and SelectionItem are each offered only when the element supports them. Actions are posted to the window and run through the same path as keyboard and mouse input.
- **Events.** Focus-changed, property-changed (Name, IsEnabled, ExpandCollapseState, IsSelected) and structure-changed events are derived from a diff between the old and new trees, and are raised only when clients are listening.
- **Panel structure.** The root is a ToolBar with three named groups: Application (the menu Button with Invoke and ExpandCollapse, and the active-window Text), Clock, and System status (network, volume, CPU, GPU, memory, windowsRICE status). Unavailable readings are disabled, are not focusable, and are named as unavailable.
- **Menu.** The menu is a native Win32 popup menu, which Windows exposes through its own accessibility support.
- **Keyboard.** Ctrl+Alt+B focuses the panel (`RegisterHotKey`; a conflict is reported, never overridden). Tab and Shift+Tab, Left and Right, and Home and End move focus. Enter, Space and Down Arrow open the menu. Escape returns to the previous window. The focus ring is 2 DIPs wide in the theme's `focus_ring` colour.

### 4.6 Glass readability

The legibility policy (`windowsrice-ui::legibility`) is applied at render time and never changes a setting:

- **Solid surface.** The surface is drawn opaque when Windows "Transparency effects" is off, reduced transparency is requested, high contrast is on (Windows system colours are used), the theme's backdrop is None, or DWM refuses the backdrop.
- **Opacity.** Otherwise the surface uses the theme's `surface_opacity`, or a runtime override clamped to 0.30–1.00.
- **Backplates.** If the pessimistic worst case (pure black or white behind the surface) puts primary or secondary text below 4.5:1, solid surface-coloured backplates are drawn behind every text element in Auto mode.
- **Never mode.** With backplates set to Never, the plan reports that the surface does not meet AA, and the panel's status element carries a warning.

Measured on screen at 150% over the live Acrylic backdrop, the Glass backplate pixel is exactly the opaque surface colour (#191D25 dark, #F2F4F7 light), so text contrast is the opaque palette contrast (15.2:1 and 15.8:1) whatever the backdrop is. Without backplates, the translucent surface measured 10.9:1 (dark) and 13.8:1 (light) over that day's backdrop. The worst case is 2.12:1 and 2.41:1, and backdrops change, so translucent Glass without backplates is **not** described as accessible.

### 4.7 Resource management and gaming

- **Device-dependent objects** are rebuilt as a unit when a device is lost (D2D recreate-target, or DXGI device removed, reset, hung or internal error). The DirectWrite state survives.
- **While hidden**, the swap chain memory is trimmed (`IDXGIDevice3::Trim`), and the sampler thread is blocked on a condition variable.
- **The sampler** reads performance counters and COM state on a worker thread, so slow queries never block input.
- **Fullscreen yield.** The panel hides completely while the foreground window on its monitor covers the monitor (excluding maximised captioned windows) or Windows reports a fullscreen or presentation app. It reappears when that ends. This path is the basis for Gaming Mode.
- **No overlays, no injection.** The panel uses only out-of-context WinEvent hooks, filtered to the foreground process. It reads no other process's memory and modifies no other window.

## 5. Phase 4 measurements

The full Phase 4 measurement was stopped on request after 5 of 9 idle runs. The completed runs are in `docs/benchmarks/2026-11-19-panel/summary-partial.md` and summarised in `docs/phase4-rendering-foundation.md` §2: a median of 108 ms to the first frame, 1.0 ms input-to-frame, about 45 MiB private memory, 21 MiB dedicated GPU memory and idle CPU of 0.16–0.34% of one core. These figures are partial, not a complete three-repetition result.

## 6. Consequences and limitations

- **Hand-built UI.** The project owns its layout, widgets, focus model and UI Automation provider. More `unsafe` FFI must be reviewed. In return it gets small footprints, correct text, native materials and no licensing conditions.
- **Native menu style.** The menu uses Windows' standard (light) menu style rather than the theme, and its items were not reachable through UI Automation in unattended runs, where the panel cannot take the foreground. A themed, provider-backed menu is future work.
- **Untested here.** Real Windows DPI transitions and multiple monitors need the owner to change settings or attach a display. Screen-reader output (Narrator) requires a person listening. Both are documented as manual checks.
- **Not implemented yet.** Text input and IME (TSF) for the launcher are not part of Phase 4.
- **Unexplained retention.** Each simulated device-loss recovery retained about 0.19 MiB of private memory (30 recoveries: +5.6 MiB, with handles flat). This needs investigation with the D3D debug layer, which is not installed on the test machine.
- **No work-area reservation.** The panel does not reserve screen space (no AppBar registration), so it overlaps the top of maximised windows. Reserving space changes other applications' work area and needs owner approval.
