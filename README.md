# windowsRICE

windowsRICE is a Rust-based desktop customization environment for Windows 11 Pro, inspired by HyDE, Hyprland, Waybar and macOS. It runs alongside Explorer as a normal user-level application and never replaces the Windows shell, sign-in screen or any protected system interface.

## Status

Phase 3 (theme engine). Nothing in the current build changes Windows settings, and no shipped executable uses the theme engine yet.

| Component | Status | Notes |
|---|---|---|
| `windowsrice-core` | IMPLEMENTED, TESTED | Configuration (schema v2 with v1 migration), paths, logging, hotkey model, capability model |
| `windowsrice-journal` | IMPLEMENTED, TESTED | Append-only change journal for backup, verification and rollback |
| `windowsrice-windows` | IMPLEMENTED, TESTED | Read-only Windows queries: OS, CPU, memory, displays, GPUs, session |
| `windowsrice-diag` | IMPLEMENTED, TESTED | Read-only diagnostics CLI |
| `windowsrice-theme` | IMPLEMENTED, TESTED | Design tokens, 8 built-in presets, validation, WCAG AA contrast checks, theme catalog, activation with rollback, fallback to windowsRICE Default, selection saved in `config.toml` |
| Renderer, Theme Studio | PLANNED | Later phases |
| Desktop, bar, dock, launcher, widgets | PLANNED | Later phases |

### Accessibility note: windowsRICE Glass

windowsRICE Glass passes every WCAG AA check with its opaque colors, which are used whenever transparency is off. When drawn translucently at 55% surface opacity, the pessimistic worst case (pure white or black behind the panel) drops text contrast to 1.88:1 (dark) and 2.41:1 (light), below the 4.5:1 AA threshold. Glass is therefore not described as fully accessible in translucent use. The engine reports these warnings whenever Glass is activated. Adjustable opacity, solid text backplates and a reduced-transparency fallback will be evaluated once real rendering exists.

## Safety model

- Explorer stays the shell. windowsRICE does not modify Winlogon, boot files, security settings or anti-cheat software.
- The desktop components run without elevation. Administrator operations, when added, go through a separate helper that accepts only allowlisted operations.
- Every future system change is journaled with its original value before it is applied, verified afterwards, and can be rolled back by a recovery utility that does not depend on the main desktop.
- The Windows integration crate and the diagnostics tool are read-only in Phase 1. A test (`apps/diag/tests/read_only_guard.rs`) fails if write APIs appear in their source.
- Global hotkeys use `RegisterHotKey` only. No low-level keyboard hooks are used. The default launcher shortcut is `Ctrl+Alt+Space`.

## Layout

```
crates/windowsrice-core      platform-independent core
crates/windowsrice-journal   change journal
crates/windowsrice-windows   Windows integration (read-only in Phase 1)
crates/windowsrice-theme     design system, built-in presets (themes/*.toml) and theme engine
apps/diag                    windowsrice-diag
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

## Configuration

User configuration will live in `%APPDATA%\windowsRICE\config.toml`. Logs, the change journal and backups will live in `%LOCALAPPDATA%\windowsRICE`. The current build does not create these directories.

## License

No license has been selected yet.
