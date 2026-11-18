# windowsRICE

windowsRICE is a Rust-based desktop customization environment for Windows 11 Pro, inspired by HyDE, Hyprland, Waybar and macOS. It runs alongside Explorer as a normal user-level application and never replaces the Windows shell, sign-in screen or any protected system interface.

## Status

Phase 1 (project foundation). Nothing in the current build changes Windows settings.

| Component | Status | Notes |
|---|---|---|
| `windowsrice-core` | IMPLEMENTED, TESTED | Configuration, paths, logging, hotkey model, capability model |
| `windowsrice-journal` | IMPLEMENTED, TESTED | Append-only change journal for backup, verification and rollback |
| `windowsrice-windows` | IMPLEMENTED, TESTED | Read-only Windows queries: OS, CPU, memory, displays, GPUs, session |
| `windowsrice-diag` | IMPLEMENTED, TESTED | Read-only diagnostics CLI |
| Desktop, bar, dock, launcher, widgets | PLANNED | Later phases |

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
