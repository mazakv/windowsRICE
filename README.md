# windowsRICE

windowsRICE is a Rust-based desktop customization environment for Windows 11 Pro, inspired by HyDE, Hyprland, Waybar and macOS. It runs alongside Explorer as a normal user-level application and never replaces the Windows shell, sign-in screen or any protected system interface.

## Status

Release candidate 0.1.0-rc.1 (Phase 10), after Phase 9 (product readiness: per-module appearance, motion, packaging), Phase 8 (visual Design Studio) and Phase 7 (design changes verified on the running bar). Phases 4 to 10 are complete and uncommitted; items that need a person or other hardware (Narrator, real DPI changes, multiple monitors, real fullscreen applications) are NOT VERIFIED and listed in the phase reports. The rendering stack is Direct2D + DirectWrite + DirectComposition with a UI Automation provider (`docs/adr/ADR-0002-ui-framework.md`); the bar architecture is in `docs/adr/ADR-0003-top-bar.md`, the Design Studio in `docs/adr/ADR-0004-design-studio.md`, and live design application with the visual galleries in `docs/adr/ADR-0005-visual-design-studio.md`. While the bar runs it reserves its strip of the screen through the Windows application desktop toolbar interface, so maximised windows start below it; the reservation ends when it exits. No Windows setting, registry value or shell configuration is changed.

| Component | Status | Notes |
|---|---|---|
| `windowsrice-core` | IMPLEMENTED, TESTED | Configuration (schema v3 with `[bar]`; v1 and v2 migrate), paths, logging, hotkey model, capability model |
| `windowsrice-journal` | IMPLEMENTED, TESTED | Append-only change journal for backup, verification and rollback |
| `windowsrice-windows` | IMPLEMENTED, TESTED | Read-only Windows queries: OS, CPU, memory, displays, GPUs, session |
| `windowsrice-diag` | IMPLEMENTED, TESTED | Read-only diagnostics CLI |
| `windowsrice-theme` | IMPLEMENTED, TESTED | Design tokens, 17 built-in presets, validation, WCAG AA contrast checks, theme catalog, activation with rollback, fallback to windowsRICE Default, selection saved in `config.toml` |
| `windowsrice-ui` | IMPLEMENTED, TESTED | Platform-independent model: bar modules and layout, settings popup, accessibility element tree, keyboard navigation, draw list, Glass legibility policy |
| `windowsrice-render` | IMPLEMENTED, TESTED | Direct3D 11 / Direct2D / DirectWrite / DirectComposition renderer with Hangul fallback, exact weights and device-loss recovery |
| `windowsrice-panel` | IMPLEMENTED, TESTED | windowsRICE Bar: one full-width bar per monitor, work-area reservation, 15 module kinds with independent refresh, settings popup, auto-hide, fullscreen hiding. Narrator, real fullscreen, real DPI changes and multi-monitor are NOT VERIFIED (manual, see `docs/phase5-top-bar.md`) |
| Design Studio | IMPLEMENTED, TESTED | Visual editor inside the bar: galleries for 15 design styles, 17 themes, 14 palettes, colour roles, backgrounds and gradients, every installed font (rendered in its own typeface), icons, module appearance, layouts and effects; live preview of the real bar layout; Apply verified on the running bar; presets, undo and redo. Narrator and real DPI changes NOT VERIFIED (see `docs/phase8-visual-design-studio.md`) |
| Dock, launcher, widgets | PLANNED | Later phases |

### Accessibility note: windowsRICE Glass

windowsRICE Glass passes every WCAG AA check with its opaque colors, which are used whenever transparency is off. When drawn translucently at 55% surface opacity, the pessimistic worst case (pure white or black behind the panel) drops text contrast to 1.88:1 (dark) and 2.41:1 (light), below the 4.5:1 AA threshold. Glass is therefore not described as fully accessible in translucent use, and the engine reports these warnings whenever Glass is activated. Since Phase 4 the renderer applies a legibility policy without changing any setting: solid text backplates are drawn automatically when the worst case fails AA, surface opacity can be overridden per run, and surfaces become opaque when Windows "Transparency effects" is off, in high-contrast mode, or when the backdrop is unavailable. With backplates turned off, the panel shows a status warning instead of claiming accessible contrast.

## Safety model

- Explorer stays the shell. windowsRICE does not modify Winlogon, boot files, security settings or anti-cheat software.
- The desktop components run without elevation. Administrator operations, when added, go through a separate helper that accepts only allowlisted operations.
- Every future system change is journaled with its original value before it is applied, verified afterwards, and can be rolled back by a recovery utility that does not depend on the main desktop.
- The Windows integration crate and the diagnostics tool are read-only (Phase 1 diagnostics and the Phase 4 live readings). A test (`apps/diag/tests/read_only_guard.rs`) fails if write APIs appear in their source.
- Global hotkeys use `RegisterHotKey` only. No low-level keyboard hooks are used. The default launcher shortcut is `Ctrl+Alt+Space`.

## Layout

```
crates/windowsrice-core      platform-independent core
crates/windowsrice-journal   change journal
crates/windowsrice-windows   Windows integration (read-only)
crates/windowsrice-theme     design system, built-in presets (themes/*.toml) and theme engine
crates/windowsrice-ui        UI model: layout, accessibility tree, keyboard, draw list (no Windows APIs)
crates/windowsrice-render    Direct2D / DirectWrite / DirectComposition renderer
apps/diag                    windowsrice-diag
apps/panel                   windowsrice-panel (windowsRICE Bar)
docs/                        ADRs, development guide, phase reports, benchmark results
scripts/                     verification, accessibility, visual and performance scripts
experiments/ui-spike         Phase 3.5 framework comparison (evidence only; separate workspace)
```

## Building

Requires the Rust MSVC toolchain (see `rust-toolchain.toml`; minimum Rust 1.100) and the Visual Studio C++ build tools.

```
cargo build --workspace
cargo test --workspace
cargo run -p windowsrice-diag            # text report
cargo run -p windowsrice-diag -- --json  # JSON report
```

`windowsrice-diag` writes only to stdout and stderr.

```
cargo run -p windowsrice-panel --release                    # windowsRICE Bar; Ctrl+Alt+B focuses it
cargo run -p windowsrice-panel --release -- --studio        # bar with Design Studio open
cargo run -p windowsrice-panel --release -- --help          # test and diagnostics options
```

The bar reads `config.toml` and writes it only when you change a setting in its settings popup (the original of an older-schema file is kept as `config.toml.v2-<time>.bak` first). `--config <file>` uses another file; `--theme`, `--mode`, `--opacity` and `--backplates` affect only the current run and are never saved. Open the settings with the menu button (Ctrl+Alt+B, then Enter); exit with "Exit windowsRICE" there. **Open Design Studio** in the same menu (or `--studio`) opens the visual editor; its changes reach the bar only when you choose Apply. Custom themes, presets and the Studio's own window settings are stored next to `config.toml` (`themes\`, `presets\`, `studio.toml`).

During development, use the verification levels in `docs/development.md` (`scripts/check.ps1 quick|integration|full`) instead of rebuilding and retesting the whole workspace after every change.

## Installing

`scripts/package.ps1` builds the release candidate `target\dist\windowsRICE-0.1.0-rc.1-win-x64.zip` and its `.sha256` (local paths are stripped from the executable). Extract it and run `install.ps1` (per-user, no administrator rights; `-StartWithWindows` / `-NoStartWithWindows` for starting at sign-in). Upgrades keep settings and a running bar is closed first; an interrupted install is restored on the next run. `uninstall.ps1` removes only windowsRICE's own files and keeps `%APPDATA%\windowsRICE` unless `-RemoveSettings` is given and confirmed. Builds are not code-signed. `scripts/package-verify.ps1` checks the package and the installer in a sandbox. Details: `packaging/INSTALL.txt` and `docs/phase10-release-candidate.md`.

## Configuration

User configuration will live in `%APPDATA%\windowsRICE\config.toml`. Logs, the change journal and backups will live in `%LOCALAPPDATA%\windowsRICE`. The current build does not create these directories; the bar writes `config.toml` only when a setting is changed in its popup or applied from the Design Studio, and diagnostics only when asked to with `--stats <file>`.

## License

No license has been selected yet.
