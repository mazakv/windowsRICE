# Phase 5: full-width windowsRICE Bar

Phase 5 turns the Phase 4 panel prototype into the windowsRICE Bar. It has:
- one full-width native bar per configured monitor, touching the top edge;
- a reserved work area;
- 14 module kinds plus the menu, with independent refresh intervals;
- configuration schema v3 with migration;
- a themed, accessible settings popup;
- auto-hide and fullscreen behaviour.

Design decisions are in `docs/adr/ADR-0003-top-bar.md`. Evidence labels follow `docs/development.md`: unit, simulated, real (this Windows 11 Pro machine), and manual (not done until a person does it).

Test machine: Windows 11 Pro 25H2 build 26200.9974, Ryzen 9 5900X, Radeon RX 9070 XT, one 3840×2160 monitor at 160 Hz and 150%. Every automated run used a project-local configuration file (`--config`); `%APPDATA%\windowsRICE\config.toml` was never read or written by tests.

## 1. What was built

| Area | Where | Notes |
|---|---|---|
| Configuration schema v3 | `crates/windowsrice-core/src/bar.rs`, `config.rs` | `[bar]` section, validation, v2→v3 and v1→v3 migration |
| Saving settings | `ThemeService::update_config` | validates first, preserves the original before the first save |
| Module model | `crates/windowsrice-ui/src/module.rs` | readings (pending / ok / unavailable / absent), per-module view, stable ids |
| Bar layout | `crates/windowsrice-ui/src/bar.rs` | regions, ordering, spacing, overflow, accessibility tree and draw list |
| Settings popup | `crates/windowsrice-ui/src/popup.rs` | themes, bar settings, module show/interval/order, actions |
| New roles | `tree.rs`, `keyboard.rs` | CheckBox (Toggle), RadioButton, Window; vertical navigation |
| New data sources (read-only) | `crates/windowsrice-windows/src/live/` | network traffic, battery, notification state, VRAM, audio and connectivity events |
| Scheduler | `apps/panel/src/scheduler.rs` | one worker per source, independent intervals, pause, cancellation |
| Work-area reservation | `apps/panel/src/appbar.rs` | application desktop toolbar registration |
| Application | `apps/panel/src/controller.rs`, `main.rs` | per-monitor bars, popup, auto-hide, fullscreen, display changes |
| Scripts | `scripts/bar-a11y.ps1`, `scripts/bar-visual.ps1` | real UI Automation and functional checks; visual captures |

Modules: menu, clock and date, CPU, GPU, VRAM, RAM, volume, network status, upload speed, download speed, active window, battery (hidden on computers without one), notification state, custom text and custom icon. None was deferred. The notification module shows Windows' notification state, not a count (ADR-0003 §7).

## 2. Results

### Full-width window and work area (real)

At native 150% the bar window is 0,0–3840,51. It is exactly full width and touches the top edge, with no margins.

**Work-area reservation**, checked by an independent process reading the monitor's work area:

| Moment | Work-area top |
|---|---|
| Before the bar started | 0 |
| With the bar running | 51 |
| After raising the height to 38 DIPs | 57 |
| Reservation turned off | 0 |
| Reservation turned on again | 57 |
| Auto-hide collapsed | 0 |
| After exit | 0 |

**Forced scales (simulated DPI):** the windows were 0,0–3840,34 / 43 / 68 / 102 / 136 at 100 / 125 / 200 / 300 / 400%.

The `work_area_after` value in the bar's own diagnostics is read immediately after `ABM_SETPOS` and can predate the shell's update. The independent reads above are the evidence.

### Modules, scheduler and configuration (unit, real)

- **Unit tests:**
  - module views never invent readings;
  - unavailable readings are disabled and keep their reason;
  - an absent battery hides its module;
  - ids are stable across reordering;
  - overflow shortens the title, then hides modules and names them;
  - every scale keeps modules on the pixel grid;
  - source intervals follow the enabled modules, and unused sources do no work;
  - configuration: validation, migration, round trip, refusing unknown keys, leaving invalid files untouched, `update_config`.
- **Real, first run:** all 8 sources sampled without errors. Collection cost was 0.9 ms for CPU, GPU and traffic, 4.7 ms for audio, and under 1 ms for the others. The battery module was correctly hidden on this desktop.
- **Real, popup changes:** checked through UI Automation and in the saved `config.toml`:
  - hiding CPU (it disappeared from the bar's tree; saved `enabled = false`);
  - moving VRAM before GPU (bar order and saved order);
  - RAM interval 5 s → 2 s (saved);
  - theme Glass (saved, bar restyled);
  - height 36 → 38 (bar 57 px, saved).
- **After a restart:** the changes persisted (CPU hidden, height 38, Glass applied).
- **Migration (real):** a v2 file was left untouched while only read. The first save preserved it byte for byte as `config.toml.v2-<time>.bak` and wrote a v3 file keeping the v2 values (Minimal theme, Korean).

### Overflow (unit, real)

Even at forced 400% (720 DIPs) every default module fitted on this monitor with real DirectWrite text, so the default set never overflows here. At 300% and 400% the clock correctly moved left of centre to avoid the status modules. Overflow was therefore forced on screen with a configuration of 14 long custom-text modules (`scripts/phase45-verify.ps1`, `docs/phase5/verify/overflow-real.png`): 6 of the 14 were hidden, each hidden module was named in the accessibility tree, and the layout lint stayed clean.

### Accessibility (real; screen reader open)

`scripts/bar-a11y.ps1`: **59 checks, 0 failed** on the final release build (`docs/phase5/a11y/checks.json`, `bar-tree.txt`, `popup-tree.txt`).
- **Bar:** a ToolBar with Left, Center and Right section groups. Every module is named with its value. The menu is a Button with Invoke and ExpandCollapse. All bounds are inside the bar, and hit testing works.
- **Focus and keyboard:** UIA SetFocus works, and Tab, Left, Home and End posted to the bar's own window move focus (provider view). The client received 5 focus-changed events and 8 Name-changed events.
- **Popup:** a Window named "windowsRICE settings" with 8 theme RadioButtons (one selected) and 18 CheckBoxes. The menu check box is disabled, the battery is listed as not on this computer, and every element is named.
- **Menu state:** the menu button reports Expanded while the popup is open and Collapsed after it closes.
- **Exit:** "Exit windowsRICE" exits cleanly and releases the reservation.
- **Bug found and fixed:** Toggle actions were dropped because the action decoder lacked the new action. A unit test now covers every action.
- **Limits:** a background script cannot give the bar the system foreground (Windows' foreground lock), so system-level focus and **Narrator are NOT VERIFIED**. Shift+Tab is covered by unit tests only, because a posted Shift key does not change `GetKeyState`.

### Visual (real)

Screenshots are in `docs/phase5/screens/`: native 150% and forced 100/125/200/300/400%, light, Glass with backplates, Glass with reduced transparency, Korean, the popup in Default and Glass, and the focus ring. Hangul renders correctly everywhere. Two popup defects seen in the first captures were fixed: the cut-off "on change" label, and the "px" unit on a value that is in DIPs.

**Contrast over real backdrops (real).** `scripts/phase45-verify.ps1` placed solid black, white and colourful windows directly under the bar and captured Light, Dark, Glass Light and Glass Dark over each. The lowest measured text contrast was 10.88:1 (`docs/phase5/verify/backdrops.json`).

**Finding: the translucency is not visible on screen.** The bar's pixels were identical over every backdrop. The bar is a never-activated window (`WS_EX_NOACTIVATE`), and Windows draws the Acrylic system backdrop of such windows with its solid fallback tint. So the Glass themes currently look like tinted solid bars. That is always readable, but it is not the translucent look the themes describe. This is recorded as a limitation (§5) rather than worked around: alternatives such as drawing our own blur of the desktop would need screen capture of other windows.

### Auto-hide and fullscreen

- **Auto-hide (real):** after the delay the bar collapsed to a 2 px strip and released the work area. A pointer move at the top edge expanded it to 57 px.
- **Fullscreen with a real fullscreen application: NOT VERIFIED.** The policy reacts to the foreground window covering the monitor and to the shell's `ABN_FULLSCREENAPP`. A background script cannot make a test window the foreground window, so a person must check this (§4). The hide/show path itself was verified in Phase 4.

### DPI and multiple monitors

- **Real 150%:** verified.
- **Forced scales:** simulated.
- **Not verified, needs a second monitor or a scaling change by the owner:**
  - real Windows scaling changes while running (`WM_DPICHANGED`);
  - resolution changes;
  - monitor connection and disconnection;
  - mixed-DPI arrangements.

### Full verification (Level 3)

`scripts/check.ps1 full`: fmt OK, check OK, Clippy with `-D warnings` OK, **212 tests passed, 0 failed**, release build OK. Release-mode tests were not run.

## 3. Performance

This was one comprehensive run at the end of the phase (`scripts/panel-perf.ps1 -Mode Full`, results in `docs/benchmarks/2026-11-19-bar/`): a 60 s warm-up, 3 repetitions of a 120 s window per configuration, with all modules at their defaults. Values are medians with min–max across repetitions. Other applications were running on the machine.

| Measurement | Result |
|---|---|
| Startup, process creation → first presented frame (10 launches) | 178.2 ms (174.1–194.7) |
| Device, swap chain and composition creation | 64.5 ms |
| Input, mouse-move message → frame presented (20 events) | median 1.12 ms, p95 1.44 ms |
| Frame draw / Present | median 0.38 ms / 0.20 ms |

| Idle configuration | CPU (% of one core) | Working set (MiB) | Private (MiB) | Dedicated / shared GPU (MiB) | GPU utilisation | Redraws per minute |
|---|---|---|---|---|---|---|
| Default (all modules) | 0.413 (0.376–0.493) | 56.1 | 47.7 | 21.1 / 3.8 | 0.003% | 37.1 (34.9–41.3) |
| Without GPU counters | 0.337 (0.285–0.389) | 49.9 | 41.8 | 21.1 / 3.7 | 0.004% | 51.3 (33.9–56.8) |
| UIA client reading the tree every 5 s | 0.505 (0.441–0.531) | 58.2 | 48.8 | 21.1 / 3.8 | 0.006% | 53.8 (34.8–54.7); client fetch 6.6 ms |

Compared with the Phase 4 panel's partial figures:
- **Startup** rose from 108 to 178 ms: 8 source threads, the work-area negotiation, and the larger theme and configuration load.
- **Idle CPU** rose from 0.16–0.34% to about 0.41% of one core, and private memory from 45 to 48 MiB. The bar shows more modules, which refresh more often: upload and download change every 2 s.
- **Redraw rate** is driven by modules whose displayed value changes. The wide ranges in the configurations without GPU counters and with the UIA client (34–57 per minute) are probably caused by changing network traffic during those windows rather than by the configuration. This was not verified.

The bar's own per-source collection costs are in each run's `scheduler` statistics; the "sampler cost" column of the generic summary does not apply to the Phase 5 scheduler.

## 4. Manual checks for the owner

1. **Run it:** `target\release\windowsrice-panel.exe`. This uses your real configuration; settings you change in the popup are saved to `%APPDATA%\windowsRICE\config.toml`.
2. **Maximise a window:** it should start below the bar. Exit the bar (menu → "Exit windowsRICE"), then maximise again: it should use the full height.
3. **Keyboard and Narrator:** start Narrator (Ctrl+Win+Enter), then press Ctrl+Alt+B.
   - Tab and Shift+Tab through the bar: each module should be announced with its value.
   - Press Enter on the menu: the settings window should be announced.
   - Move with Up and Down. Space toggles a module, and Enter activates a button or chooses a theme.
   - Escape returns to the bar, and Escape again returns to your previous window.
4. **Alt+F4** while the bar has focus closes windowsRICE.
5. **Fullscreen:** with the bar running, put a game or video into fullscreen. The bar should disappear and return afterwards.
6. **Auto-hide:** enable "Hide automatically", move the pointer away and then to the top edge.
7. **Scaling:** change Settings › Display › Scale to 125%, 100% and 200% while the bar runs. It should resize and stay sharp, and maximised windows should still start below it.
8. **Second monitor, if available:** connect it, check that a second bar appears, disconnect it, and check that the bar is removed cleanly.

## 5. Known limitations

- **Scope gap.** Multi-monitor, mixed-DPI and hot-plug behaviour, real fullscreen behaviour, real DPI changes and Narrator are not verified (§2, §4). The master specification's wallpaper and desktop-surface items remain outside this phase.
- **Device names: resolved (§6).** Per-monitor settings now record the monitor's stable device path and fall back to the device name only for older entries.
- **Notification indicator:** state only; there is no count API for desktop applications.
- **Weather** was not implemented; it is not part of this phase's module list.
- **One renderer per surface.** Each bar, the popup and the Design Studio has its own Direct3D device. That is about 21 MiB of dedicated GPU memory per bar on this monitor, so N monitors use about N × 21 MiB (§6).
- **Glass looks solid on the bar: resolved in Phase 7.** Windows drew the Acrylic backdrop of never-activated windows with its fallback tint (§2, Visual). The bar now draws translucency itself, without blur (`docs/phase7-live-design.md`).
- **Phases 4–6 are uncommitted.** They share one working tree on `master` above `20981ec`.

## 6. Completion (closed at the end of Phase 6, 2026-11-19)

All automated checks were re-run on the final release binary, which includes the Phase 6 Design Studio.
- **Accessibility (`scripts/bar-a11y.ps1`, real):** 59 checks, 0 failed (`docs/phase5/a11y-final/`). The popup tree now also contains the **Open Design Studio** button.
- **Remaining Phase 5 items (`scripts/phase45-verify.ps1`, real):** 15 checks, 0 failed (`docs/phase5/verify/checks.json`).
  - **Hide and resume:** no frames were presented while hidden, and drawing resumed afterwards.
  - **Alt+F4:** fixed and verified. The bar is a `WS_POPUP` window without a system menu, so the default window procedure never turned Alt+F4 into a close request. The bar now handles Alt+F4 itself: it closes the popup when the popup has focus, and otherwise exits windowsRICE. In the check, posted to the bar's own window, the exit reason was `Alt+F4` and the work-area top went from 51 back to 0. A real key press by a person remains a manual check (§4).
  - **Overflow and backdrop contrast:** real overflow and contrast over real backdrops (§2).
- **Stable monitor identity:** implemented (ADR-0003 §6). On this monitor the identity is a `\?\DISPLAY#...` device path. Renumbering with several monitors is NOT VERIFIED.
- **GPU memory per monitor:** about 21 MiB of dedicated GPU memory per bar at 3840×57 px (`docs/phase4-rendering-foundation.md` §6).
- **Level 3 on the final code:** fmt, check, Clippy with `-D warnings`, **223 tests passed, 0 failed**, and release and Debug builds succeeded.
- **Still NOT VERIFIED (needs a person or hardware):** the procedures in §4 apply.
  - Narrator;
  - real system keyboard focus through Ctrl+Alt+B;
  - Alt+F4 pressed by a person;
  - a real fullscreen game or video;
  - real DPI changes;
  - multiple monitors, hot-plug and mixed DPI;
  - high contrast.

**Phase 5 status: COMPLETE**, with the manual and hardware items above NOT VERIFIED.
