# Phase 4: native rendering foundation

Phase 4 builds the smallest production rendering foundation on the stack chosen in ADR-0002 (Direct2D, DirectWrite, DirectComposition, Win32, UI Automation), and a 34 px windowsRICE Default top panel that exercises it. The architecture is described in `docs/adr/ADR-0002-ui-framework.md`; this document records what was verified and how.

Evidence is labelled by kind, as defined in `docs/development.md`:

- **unit:** pure Rust tests, no Windows state;
- **simulated:** real code paths driven by synthetic input (forced scale, simulated device loss);
- **real:** the release panel running on this Windows 11 Pro machine and inspected from outside;
- **manual:** requires a person, and stays NOT VERIFIED until done.

Test machine: Windows 11 Pro 25H2 build 26200.9974, Ryzen 9 5900X, Radeon RX 9070 XT (driver 32.0.32015.7029), one MSI MAG323UPF at 3840×2160, 160 Hz and 150% (144 DPI, re-queried on 2026-11-19). Windhawk and the AMD driver inject DLLs into processes, and other applications (including a game) were running during some runs.

## 1. Results by area

### Rendering initialisation (real)

The panel creates a hardware D3D11 device on the RX 9070 XT (feature level 11.0, not WARP), a 3816×51 composition swap chain at 144 DPI, and the composition tree. Device, swap chain and composition creation took 60–65 ms. The `windowsrice-render` real-machine test also renders to a hidden window, simulates device loss, recovers, renders again, resizes to 200% and runs the fade.

### Korean and English text (unit, real)

- **Font resolution.** DirectWrite resolves Hangul to Malgun Gothic and Latin text to Segoe UI Variable, through the explicit fallback in the typographic font model (render test and panel diagnostics).
- **Weights.** 400, 500 and 600 produce distinct widths, so 500 is not snapped to Semibold (render test).
- **Screenshots** at native 150% and forced 100%, 125% and 200% (`docs/phase4/screens/`) show correct Hangul in "문서 — 파일 탐색기" and "11월 19일 (목)". Unlike the Slint FemtoVG spike, they show no missing strokes or substituted syllables.
- **Bug found and fixed.** The first real run drew at 96 DPI inside the 150% window, because the Direct2D context DPI must be set explicitly. The unit and render tests could not detect this, so the screenshot check is part of the verification.

### DPI and display (unit, simulated, real; manual items open)

- **Placement (unit).** Tests cover 100/125/150/200% and attached, floating, bottom, offset-monitor and work-area cases.
- **Native 150% (real):** window 12,12–3828,63 (3816×51 px, 2544×34 DIPs).
- **Forced scales (simulated):** 100% gives 8,8–3832,42; 125% gives 10,10–3830,53; 200% gives 16,16–3824,84. Every element rectangle lies on the physical pixel grid (unit test at all four scales).
- **Real Windows scaling at 100%, 125% and 200%, and live `WM_DPICHANGED` transitions: NOT VERIFIED.** This machine has one monitor, so a real DPI transition needs the owner to change Settings › Display › Scale (§3.1). Forced scale is not the same thing and is not reported as such.

### Accessibility (unit, real; screen reader open)

- **Unit.** Tests cover tree structure, roles, names (Korean and English), stable automation ids, focus order, disabled unavailable readings, keyboard navigation (Tab, Shift+Tab, Left, Right, Home, End, Enter, Space, Down, Escape), ExpandCollapse state, hover, tree and draw-list synchronisation, and a lint that rejects unnamed elements, duplicate ids and out-of-parent bounds.
- **Real (out-of-process UI Automation client, `scripts/panel-a11y.ps1`): 34 checks, 0 failed** (`docs/phase4/a11y/checks.json`, tree in `uia-tree.txt`). Verified:
  - the ToolBar root with 3 named groups and 12 elements, each named, with FrameworkId `windowsRICE`;
  - the menu Button with Invoke and ExpandCollapse patterns and help text;
  - 8 Text elements with correct names;
  - bounds inside the window, and hit testing;
  - UIA `SetFocus`;
  - the navigation sequence through posted keys (Tab, Left, Home, End, Right);
  - 8 focus-changed and 6 Name-changed events received by the client;
  - ExpandCollapse switching from Collapsed to Expanded and back to Collapsed around the native menu;
  - clean shutdown;
  - the unavailable GPU reading exposed as disabled, not focusable and "GPU usage unavailable".
- **Limits of the automated run.** A background script cannot make the panel the foreground window: Windows' foreground lock refused `SetForegroundWindow`, and no global input was injected. So focus was verified at the provider level (`HasKeyboardFocus`) rather than through the system focus. Likewise, the native menu window was exposed but its items were not, which is consistent with a menu that is not in the foreground. Shift+Tab cannot be simulated by posting messages, because posted Shift does not change `GetKeyState`, so it is covered by unit tests only.
- **Screen reader (Narrator), real system focus through Ctrl+Alt+B, and the native menu items: NOT VERIFIED** (§3.2).

### Theme integration (unit, real)

- **Startup.** The panel starts the Phase 3 `ThemeService` (it reads `config.toml` and writes nothing) and accepts the resolved theme through a `ThemeSink`.
- **Run-only overrides.** `--theme` and `--mode` use a separate engine and are never saved.
- **Light/dark.** Windows light/dark changes call `ThemeService::reapply`, which is covered by a test asserting that the config file is not created.
- **Presets.** All eight presets build lint-clean panels in every mode they define (unit). Default dark and light and Glass dark and light were rendered on screen (real).

### Glass readability (unit, real)

See ADR-0002 §4.6.

| Glass configuration (150%, live Acrylic) | Surface opacity | Backplates | Worst case (black/white backdrop) | Measured text contrast behind the text |
|---|---|---|---|---|
| Dark, Auto | 0.55 | yes | 9.40:1 | 15.17:1 (backplate pixel #191D25) |
| Dark, Never | 0.55 | no | **2.12:1, warning shown** | 10.88:1 on that day's backdrop |
| Dark, opacity 0.90, Never | 0.90 | no | 6.96:1 | 14.30:1 |
| Dark, reduced transparency | 1.00, no backdrop | no | 9.40:1 | 15.17:1 |
| Light, Auto | 0.55 | yes | 7.97:1 | 15.83:1 (backplate pixel #F2F4F7) |
| Light, Never | 0.55 | no | **2.41:1, warning shown** | 13.84:1 on that day's backdrop |

The measured values without backplates only describe the backdrop present at capture time. Translucent Glass without backplates is not described as accessible.

### Resource cleanup (simulated, real)

- **Device loss (simulated).** 30 simulated device losses in 60 s gave 30 recoveries, 0 failures and 0 GDI objects. Handles stayed flat (933–941), and drawing continued after each recovery. Private memory rose by about 0.19 MiB per recovery (+5.6 MiB over 30), while a run without device loss stayed flat at 44.8 MiB. This retention is an open issue (§4).
- **Shutdown (real).** Every completed run exited with code 0, through `--exit-after` or `WM_CLOSE`. The menu's "Exit panel" item and Alt+F4 were not exercised automatically. Hooks, the hotkey and the sampler thread are released, and UI Automation providers are disconnected (`UiaReturnRawElementProvider(…, NULL)`, `UiaDisconnectAllProviders`).
- **Hide and show (real; the Gaming Mode path).** Hidden from 5 s to 35 s: 0 frames, 0 samples, and process CPU over 25 s hidden at 0.000 s, which is below the resolution of the method. The fullscreen yield uses the same path.

## 2. Performance

The full measurement (`scripts/panel-perf.ps1 -Mode Full`) was **stopped on request after 5 of its 9 idle runs**. Only completed runs are reported, in `docs/benchmarks/2026-11-19-panel/summary-partial.md`. With at most two repetitions per configuration, the full methodology (medians across three repetitions) is incomplete.

| Completed measurement | Result |
|---|---|
| Startup, process creation → first presented frame (10 launches) | median 108.3 ms (101.0–117.9) |
| Device, swap chain and composition creation | median 64.4 ms |
| Input, mouse-move message → frame presented (20 events) | median 1.00 ms, p95 1.25 ms |
| Frame draw / Present call | median 0.33 ms / 0.23 ms |
| Idle CPU, default (2 runs) | 0.335% and 0.156% of one core |
| Idle private / working set, default | 45.0–45.1 / 52.5–52.6 MiB |
| Idle private, without GPU counters (2 runs) | 38.5–38.7 MiB (GPU counters cost about 6.5 MiB) |
| Dedicated / shared GPU memory | 21.1 / 3.3–3.8 MiB; GPU utilisation 0.003% |
| Redraws | 22–24 per minute (2-second readings and the clock) |
| With a UIA client reading the tree every 5 s (1 run) | 0.246% CPU, +2 MiB working set, client fetch median 5.8 ms |
| Hidden (Gaming Mode path) | 0 frames, 0 samples, 0.000 s CPU in 25 s |

The 160 Hz frame-pacing measurement from Phase 3.5 has no Phase 4 equivalent. The panel has no application-driven animation; its show fade is a DirectComposition animation run by DWM.

## 3. Manual checks for the owner

These need a person, and the first changes a display setting. They are not run automatically.

### 3.1 Real DPI transitions

1. Run `target\release\windowsrice-panel.exe --stats dpi-test.json` from the project folder (build it first with `cargo build -p windowsrice-panel --release`).
2. Open Settings › System › Display › Scale and choose 125%. Check that the panel moves to the top of the screen at the new size with sharp text. Repeat for 100% and 200%, then return to 150%.
3. Exit the panel (Ctrl+Alt+B, Enter, then choose "패널 종료" / "Exit panel"). `dpi-test.json` lists each `WM_DPICHANGED` under `dpi_events` and each new placement under `display_events`.

### 3.2 Narrator and keyboard

1. Start the panel, then start Narrator (Ctrl+Win+Enter).
2. Press Ctrl+Alt+B. Narrator should announce the focused element (normally "windowsRICE menu, button").
3. Press Tab and Shift+Tab through all elements. Each should be announced with its name, for example "CPU usage 12%". The focus ring should be visible on each.
4. On the menu button, press Enter. Narrator should announce the menu and its items, and the version line should be announced as unavailable (disabled). Press Escape to close the menu, and Escape again to return to the previous window.
5. Turn Narrator off (Ctrl+Win+Enter).

## 4. Known limitations and open issues

- **Scope versus the master specification.** The master specification's Phase 4 also lists wallpaper support, basic desktop surfaces and multi-monitor handling. The approved Phase 4 scope covered the rendering foundation and the panel prototype only. Wallpaper rendering and desktop surfaces were not implemented, and multi-monitor support is limited to monitor enumeration, explicit monitor selection and fallback to the primary monitor (no per-monitor panels, not tested with more than one display).
- **Fullscreen yield.** It was verified only through the simulated hide/show path, not with a real fullscreen application.
- **Formatting, Clippy and the workspace tests** were not run at the end of Phase 4 itself. They were run at the end of Phase 6 on the final code (§6).

- **Not verified (manual):** screen-reader output, real system focus through Ctrl+Alt+B, native menu items in UI Automation, and real DPI transitions and other scaling levels (§3).
- **Native menu.** The menu uses Windows' standard light menu style rather than the theme.
- **Device-loss retention: resolved as bounded, not a leak (§6).** The 0.19 MiB per recovery seen in short runs was warm-up. Over 566 forced recoveries the growth flattened to about 1.5 KiB per recovery, with handles, GDI and USER objects flat. Attributing the remaining bytes to a specific driver allocation would need the D3D debug layer (Graphics Tools optional feature, not installed).
- **Busy foreground titles.** While the foreground window's title changes often (for example an animated terminal title), the panel redraws on each change, using about 0.5% of one core in a 60-second sample. Fixed titles give the idle figures in §2.
- **Polled readings (Phase 4 panel only).** The Phase 4 panel polled audio every 2 s and connectivity every 10 s. Phase 5 replaced both with event callbacks.
- **No work-area reservation (Phase 4 panel only).** Phase 5 added the reservation with approval.
- **Gaming Mode** detection beyond the fullscreen yield is out of scope.
- **Not tested:** multiple monitors and high-contrast mode on screen. High contrast is implemented and unit-tested, but turning it on changes a system setting.

## 5. Verification status at the end of Phase 4

Level 3 (full) verification was **not run**, by owner instruction. The checks below are the last ones that actually ran. None was repeated after the final code edits, except the release build.

| Check | Last result | Covers the final code? |
|---|---|---|
| `cargo build -p windowsrice-panel --release` | OK at 14:53:19 | yes (newer than the last source edit, 14:53:13) |
| `windowsrice-ui` tests (36) | pass | yes (only `cargo fmt` changed the crate afterwards) |
| `windowsrice-render` tests (5, real GPU) | pass | **no**: the `SetDpi` fix came later |
| `windowsrice-windows` + read-only guard (26) | pass | yes |
| `windowsrice-theme` reapply test (filtered run) | pass | yes; the full theme suite was not re-run |
| `cargo clippy -p windowsrice-panel -D warnings` | pass | **no**: later edits (startup error file, `--no-gpu-counter`, hide/resume, hover timing) |
| `cargo fmt` | applied mid-phase | **no** check after the last edits |
| Workspace-wide check, Clippy, tests, release build (Level 3) | not run | — |
| Real UIA checks (`scripts/panel-a11y.ps1`) | 34/34 | binary from before the hide/resume and hover changes |
| Visual, DPI and legibility captures (`scripts/panel-visual.ps1`) | 11/11 configurations | binary from before the hide/resume and hover changes |

## 6. Completion (closed at the end of Phase 6, 2026-11-19)

The open Phase 4 items were finished in the same working tree as Phases 5 and 6, so they were verified against the final code rather than the Phase 4 binary.

- **Device-loss retention (simulated, real GPU).** 566 forced device-loss recoveries in one process (`docs/phase4/device-loss-600.json`). Private memory rose about 0.13 MiB per recovery for roughly the first 50 recoveries and then flattened: over the last 100 recoveries it grew about 1.5 KiB per recovery, and the total stayed near +15 MiB from about recovery 100 onwards. Handles stayed between 966 and 969, GDI objects at 0 and USER objects at 19. This is bounded warm-up of the Direct3D runtime and driver caches, not a per-recovery leak. The earlier 0.19 MiB figure came from short runs that only covered the warm-up.
- **GPU memory per surface (real).** Each renderer (one per bar, plus the popup and the Design Studio while they are open) owns its own Direct3D device and swap chain. One bar at 3840×57 px uses about 21 MiB of dedicated GPU memory, read from the `GPU Process Memory` counter. With N monitors the bars use about N × 21 MiB. Sharing one device across bars would reduce this, but it would also couple device-loss recovery across monitors. That change is left for a later phase and recorded in ADR-0003.
- **Level 3 verification on the final code (2026-11-19):** `scripts/check.ps1 full` passed:
  - fmt;
  - check of all targets;
  - Clippy with `-D warnings`;
  - **223 tests passed, 0 failed**, including the 5 real-GPU renderer tests;
  - the release build.

  A Debug build of the workspace also succeeded.
- **Still NOT VERIFIED (needs a person or hardware):**
  - screen-reader output with Narrator;
  - real DPI transitions and other scaling levels;
  - high-contrast mode on screen;
  - multiple monitors.

  These are not claimed. The procedures in §3 apply unchanged.

**Phase 4 status: COMPLETE.** The automated verification is done on the final code. The manual items above remain open and are listed as NOT VERIFIED, not as passed.
