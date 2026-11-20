# ADR-0003: windowsRICE Bar architecture (Phase 5)

- Status: **Proposed** (implemented in Phase 5; awaiting owner review)
- Builds on ADR-0002 (Direct2D, DirectWrite, DirectComposition, UI Automation).

## Decisions

### 1. Work-area reservation through the application desktop toolbar API

| Option | Result |
|---|---|
| **`SHAppBarMessage` (ABM_NEW / QUERYPOS / SETPOS / REMOVE)** | Documented API. The shell shrinks the monitor's work area and maximised windows start below the bar. It negotiates with the taskbar and other toolbars, and notifies about fullscreen applications. **Chosen.** |
| Overlapping topmost window (Phase 4) | Covers the top of maximised windows, including their caption buttons. |
| Changing the work area with `SystemParametersInfo(SPI_SETWORKAREA)` | A global setting change, conflicts with the shell, and is not released if the process ends abnormally. Rejected. |

The reservation lasts only as long as the bar window: `ABM_REMOVE` releases it on exit, when reservation or auto-hide is turned off, and when a monitor disappears. Nothing is written to the registry or to taskbar settings. Verified on this machine: the work area changed from top 0 to top 51 (and 57 after a height change), returned to 0 when reservation was turned off, and returned to 0 after exit.

### 2. Own auto-hide rather than shell auto-hide registration

`ABM_SETAUTOHIDEBAREX` allows one auto-hide bar per edge per monitor and fails if another one, such as an auto-hidden taskbar, already holds the edge. The bar instead collapses its own window to a 2 px strip at the monitor's top edge after the configured delay. Pointing at the strip expands it. While auto-hidden, no work area is reserved, nothing is laid out or drawn, and the scheduler pauses when every bar is collapsed.

### 3. One worker thread per data source

| Option | Result |
|---|---|
| One sampler thread for everything (Phase 4) | One slow query (GPU performance counters, Network List Manager) delays every other reading. |
| A thread pool with timers | Adds a dependency, and still needs per-source non-overlap. |
| **One thread per source (8 threads)** | Each source has its own interval, the shortest among the enabled modules that use it. A source runs one collection at a time, is independent of slow neighbours, is parked on a condition variable when unused or hidden, and records its own errors. **Chosen.** |

Audio volume and default-device changes (`IAudioEndpointVolumeCallback`, `IMMNotificationClient`) and connectivity changes (`INetworkListManagerEvents`) wake their workers through events. Their interval is only a safety re-read. Readings that two modules share (GPU utilisation and VRAM from one PDH query; upload and download from one interface-counter read) are collected once.

### 4. Configuration schema v3

The new `[bar]` section holds modules, height, spacing, reservation, auto-hide, the fullscreen policy, monitor placement and per-monitor overrides, the focus hotkey and popup preferences.
- **Migration from v2** adds `[bar]` with defaults and changes nothing else. A v1 file migrates through v2.
- **Before the first save** over a migrated or invalid file, the original is preserved as `config.toml.v2-<time>.bak` and never overwritten.
- **Invalid files** (unknown keys, out-of-range values, duplicate module ids, a missing or disabled menu module) are refused with every problem listed. Defaults are then used and the file is left untouched.
- **Saving:** settings are saved only after an explicit change in the popup, through `ThemeService::update_config`, which validates first.

### 5. Settings popup on the same UI model

The popup is a second native window drawn by the same renderer from a `Scene` built in `windowsrice-ui::popup`, exposed by the same UI Automation provider.
- **Roles:** Window, Group, RadioButton with SelectionItem, CheckBox with Toggle, Button with Invoke, and Text.
- **Ids:** element ids derive from stable keys, so focus stays on a control after it moves.
- **Keyboard:** Up, Down, Tab and Shift+Tab move focus; Space and Enter toggle or invoke; Escape closes.

The Phase 4 native Win32 menu, which was unthemed and whose items were unreachable through UI Automation in unattended runs, is gone.

### 6. Per-monitor bar instances keyed by device name

The controller reconciles bars with `EnumDisplayMonitors` at startup, on `WM_DISPLAYCHANGE` and on `WM_SETTINGCHANGE` (debounced by 300 ms):
- it removes the bars of monitors that disappeared (releasing their reservation, renderer and providers);
- it creates bars for new or newly enabled monitors;
- it re-positions the rest.

Each bar has its own renderer, DPI, accessibility host and reservation.

**Monitor identity (amended in Phase 6).** The Windows device name (`\\.\DISPLAY2`) can change when the display topology changes, so per-monitor settings now also record a stable identity. That identity is the monitor's device interface path from `DisplayConfigGetDeviceInfo` (`DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME`, field `monitorDevicePath`, for example `\\?\DISPLAY#MSI3CB5#...`). It identifies the physical monitor and its connection, and stays the same when Windows renumbers devices.
- **Matching:** an override with an `id` matches by `id` only. An override without one (written before Phase 6) still matches by device name, so existing files keep working unchanged.
- **Writing:** the Design Studio writes both `device` and `id` for new overrides.
- **Edge case:** the same monitor model on a different port gets a different path, which is the expected behaviour for "this screen".
- **Not verified:** renumbering with real multiple monitors (one monitor here).

**GPU memory.** One Direct3D device per bar keeps device-loss recovery local to one monitor. It costs about 21 MiB of dedicated GPU memory per bar at 3840×57 px. Sharing one device would save memory but couple recovery across monitors; it stays a candidate for later.

### 6a. Bar style (added in Phase 6)

`[bar.style]` holds optional overrides applied on top of the active theme by one function (`windowsrice_ui::style::with_bar_style`). The bar, the popup and the Design Studio preview all use that same function. The overrides are:
- background, accent and text colours (`#RRGGBB`);
- surface opacity (0.30–1.0);
- the text-backplate policy (`auto`, `always`, `never`);
- a bottom border and module separators;
- font family, size (11–18) and weight (400 or 500);
- horizontal padding and vertical alignment.

Each module can also hide its label (`show_label`), take a maximum width (`max_width`, 40–800 DIPs) and, for the clock, use a 12-hour format (`hour12`). Every field is optional, so schema v3 files without them stay valid and look the same. Colour overrides that drop opaque text contrast below 4.5:1 are refused by the Studio before saving.

### 7. Notification indicator scope

Desktop (unpackaged) applications have no documented API for the number of Windows notifications. The module therefore shows the user-notification state (`SHQueryUserNotificationState`): accepting notifications, quiet time, or suppressed by a fullscreen or presentation application. It does not show a count, and it does not claim to replace Windows' notification centre. Whether Windows 11 "Do not disturb" maps to quiet time was not verified.

### 8. Phase 4 panel kept as a library layout

`windowsrice-ui::panel` (the fixed Phase 4 layout) and its 18 tests are unchanged. The application now uses `windowsrice-ui::bar`. Reading types shared with the bar remain in `panel`.

## Consequences

- **Benefits:** maximised windows no longer go under the bar, readings stay independent, and the popup is themed and accessible.
- **Costs:** the bar uses 8 worker threads (most of them sleeping), and the controller is the largest module in the code base.
- **Main open issue:** behaviour with real multiple monitors, hot-plugging and mixed DPI has not been verified, because this machine has one monitor.
- **Translucency (superseded by ADR-0005):** Windows draws the Acrylic backdrop of a never-activated window with its solid fallback tint, so Glass themes looked tinted-solid on the bar. Since Phase 7 the bar uses no system backdrop and draws translucency itself (no blur).
