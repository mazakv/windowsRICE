# Phase 10: release candidate 0.1.0-rc.1

Phase 10 audits windowsRICE for its first release candidate, fixes the defects found, hardens the installer, verifies the package, and documents what still needs a person or hardware. Evidence labels follow `docs/development.md`.

All tests used isolated configurations, sandbox folders under `target\`, fixed window titles and pinned test backdrops:
- the owner's configuration was not written;
- the owner's registry was not written;
- the owner's Start menu and Startup folder were not changed.

## 1. Initial state

- **Git:** branch `master`, HEAD `20981ec` ("Phase 3: theme engine and configuration integration"), no stashes. 13 modified tracked files and 25 untracked paths, holding the uncommitted work of Phases 3.5 to 9.
- **Package:** `target\dist\windowsRICE-0.1.0-win-x64.zip`, 5 files, checksum file matching (`8E54D47D…`). Its executable (15:25) was older than the current release build (15:42), so the package no longer matched the code.

## 2. Audit findings and fixes

| # | Finding | Severity | Fix |
|---|---|---|---|
| 1 | **The release executable contained the builder's user folder** (76 occurrences of `C:\Users\<name>\.cargo…`, `.rustup…` and the PDB path) | Privacy | The package build uses its own target folder, remaps source paths (`--remap-path-prefix`) and records only the PDB file name (`/PDBALTPATH`). `package.ps1` refuses to package an executable that still contains `\Users\<name>`. Verified: 0 occurrences. |
| 2 | **The uninstaller deleted the whole install folder**, including files a user put there; a wrong `-InstallDir` would delete that folder | Data loss | Uninstall removes only windowsRICE's own files and keeps the folder if anything else remains. The installer refuses drive and profile roots and non-empty folders that are not a windowsRICE install. |
| 3 | **An upgrade deleted unrelated files** in the install folder | Data loss | Files that are not windowsRICE's are carried over into the new install. |
| 4 | **`-RemoveSettings` deleted settings without confirmation** and accepted any folder | Data loss | Settings removal needs typing YES or `-Yes`; non-interactive runs without `-Yes` are refused with nothing changed. Only a folder named `windowsRICE` is accepted. |
| 5 | **An install interrupted between its two renames** left no installation, and the next run deleted the backup | Broken state | The next run restores the previous install first, then completes. Stale staging folders are cleaned up. |
| 6 | **No way to stop starting with Windows** after opting in | Missing | `-NoStartWithWindows`. |
| 7 | **Uninstall removed shortcuts by name** | Safety | Only shortcuts whose target is this install are removed. |
| 8 | **Monitor identity:** Windows reported no device path for the monitor, and per-monitor settings saved with an id would no longer match | Settings loss | (a) `EnumDisplayDevices` (`EDD_GET_DEVICE_INTERFACE_NAME`) is used as a documented second source. (b) Matching: an unidentified monitor uses a setting saved for its device name, unless that setting's id belongs to another connected monitor. An identified monitor never takes another monitor's setting. (c) The Studio toggle edits the setting that matching uses, keeping a known id. |
| 9 | **A first version of fix 8 merged two monitors** (an identified monitor B took monitor A's setting at A's old device name); caught by the existing `monitor_rules` test | Correctness | The fallback is limited to unidentified monitors; a new test covers the case. |
| 10 | **The settings popup grew to 800 DIPs** with 17 themes (Phase 8) and did not fit a 1366×768 screen at 100% | Clipping | Themes are shown in two columns with short labels when there are more than 8; the popup is now 582 DIPs. A regression test requires at most 734 DIPs with every built-in theme plus a custom one. |
| 11 | **`studio.toml` was written in place**; a crash could truncate it | Robustness | Written through a temporary file and renamed; a failed write is logged and the temporary file removed. |
| 12 | **The preview's right-end close-up painted a dark block** where it skipped a cut module | Visual | The skipped area is left empty. |
| 13 | **The Animations switch was misaligned** with the other switches in Advanced | Visual | Same width as the rows above. |
| 14 | **The stale package** (§1) | Release | Rebuilt as `0.1.0-rc.1`. |

Also reviewed and found sound:
- **Writes:** `config.toml` is written with a temporary file, flush and rename, and the original of an older schema is kept before the first save. Presets and custom themes use temporary files and renames.
- **Invalid input:** invalid presets are refused with a reason before they reach the draft. An invalid `config.toml` is refused with every problem listed and left untouched.
- **Offline:** windowsRICE contains no network code. Diagnostics are written only with `--stats`.

## 3. Installation and upgrade (real, sandbox)

`scripts/package-verify.ps1`: **28 checks, 0 failed** (`docs/phase10/package-verify-checks.json`). The registry is never written; the Apps & features values are checked through `-RegistryPlan`.

| Area | Checked |
|---|---|
| Fresh install | From the zip extracted into a clean folder; Start menu shortcuts point to the installed exe; no Startup shortcut unless asked; planned Apps & features values (name, version, install location, uninstall command) |
| Upgrade | While the installed bar runs, it is closed with `--quit` (exit reason recorded); settings and the user's own files kept; start-with-Windows choice kept; opt-out removes the Startup shortcut |
| Interruption | After an interrupted install (old install moved aside, stale staging folder), the next run restores and completes |
| Refusals | Unrelated non-empty folder; profile root; removing settings without confirmation; removing a folder that is not windowsRICE settings. Nothing is changed in each case |
| Uninstall | Removes windowsRICE files and shortcuts, keeps settings and the user's own files |
| Repetition | Three install and uninstall cycles leave no installation and keep settings |
| Confirmed removal | `-RemoveSettings -Yes` removes only the settings folder |

Not verified: the real Apps & features entry in the owner's registry, and running the installer into the owner's profile (procedure in §8).

## 4. Package integrity

- **Package:** `target\dist\windowsRICE-0.1.0-rc.1-win-x64.zip`, 1,133 KB.
- **SHA-256:** `9DF935DFF2B51803AD340913B50867CD88BE5A8BBCBDF4E9FA11161E4D67C8B9`, in `windowsRICE-0.1.0-rc.1-win-x64.zip.sha256` (verified).
- **Checks:**
  - exactly `install.ps1`, `uninstall.ps1`, `INSTALL.txt`, `windowsrice-panel.exe` and `windowsrice.ico` (no screenshots, configurations or development files);
  - version information `windowsRICE 0.1.0-rc.1`, "windowsRICE Bar";
  - the embedded icon is present, and the icon file matches `assets\windowsrice.ico`;
  - no user name, local path, password, key or secret in any shipped file;
  - the shipped exe is the package build.
- **Not signed.** Signing needs a code-signing certificate, `signtool` and a timestamp server; none is available. SmartScreen will warn on first run.

## 5. Monitors and DPI

- **Observed (real):** while the DisplayPort monitor was apparently asleep, `EnumDisplayDevices` listed adapter entries `\\.\DISPLAY7` to `\\.\DISPLAY11`, none attached to the desktop and none with a monitor. The desktop still ran on `\\.\DISPLAY1` at 3840×2160. DisplayConfig gave a friendly name but no device path. In this state no documented source identifies the physical monitor, so the stable identity is **NOT VERIFIED** in this phase.
- **Automated (unit):**
  - `crates/windowsrice-core/tests/monitor_identity.rs` (6 tests): renumbered identified monitor, unidentified monitor keeping its setting, never borrowing a connected monitor's setting, legacy settings without id, exact id over device name, a different identified monitor never taking a setting;
  - the original `monitor_rules` test;
  - the Studio test for toggling while unidentified.
- **Arrival, removal, standby and wake, resolution changes, mixed DPI:** handled by the existing reconciliation (`WM_DISPLAYCHANGE`, `WM_SETTINGCHANGE`, `WM_DPICHANGED`). **NOT VERIFIED** on hardware (one monitor).
- **Cost:** the identity query takes about 1 ms (measured); the new fallback 1.5 µs.

## 6. Accessibility and Windows integration

- **Automated (real):**
  - `bar-a11y.ps1`: **58 passed, 0 failed, 1 not run.** The auto-hide collapse check was not run because the real mouse pointer rested on the bar strip (236, 0), which keeps an auto-hidden bar open by design (`GetCursorPos` in the controller). The script never moves the user's pointer; the check passed in earlier runs.
  - Studio: **51/51** including keyboard, grid and slider navigation.
  - End-to-end: **33/33**.
  - Phase 4/5 checks: **15/15**.
  - Accessibility lint at every Studio size, language and scroll position (unit).
- **NOT VERIFIED** (no person at the machine, or the test would change system settings or needs hardware):
  - Narrator;
  - real high-contrast mode;
  - Windows "Animation effects" off;
  - real DPI changes;
  - mixed DPI;
  - multiple monitors and hot-plugging;
  - Korean typed with the IME by hand;
  - clipboard shortcuts in the running window (posted messages cannot carry the Ctrl state, and the scripts send no global input).

## 7. Performance (real, release, `scripts/perf-quick.ps1`)

| Measure | Phase 9 | Phase 10 |
|---|---|---|
| Startup to first frame (3 runs) | 190, 195, 202 ms | 261, 296, 270 ms |
| Idle CPU | 0.42% of one core | 0.52% of one core |
| Idle redraws | 54 per minute | 62 per minute |
| Draw / present (median) | 0.74 / 0.20 ms | 0.78 / 0.24 ms |
| Bar private / GPU memory | 46.8 / 19.1 MiB | 46.8 / 21.1 MiB |
| Studio open / after close / after second close (private) | 83.2 / 71.2 / 71.7 MiB | 83.4 / 70.7 / 72.4 MiB |
| Studio GPU memory after close | 18.4 MiB | 20.6 MiB (bar level) |
| Studio scene build (median / max) | 1.76 / 6 ms | 1.91 / 7.3 ms |

Startup is about 70 ms slower than in Phase 9. The only Phase 10 change on the startup path is the monitor identity fallback, measured at about 1 ms, so the cause is not established (system load and display state differ between runs). The value is reported as observed. Idle CPU and redraws vary with how often readings change. Memory and cleanup are unchanged, and closing the Studio still returns its GPU memory and handles.

## 8. Manual verification

1. **Installer (real profile):** extract the zip, check the SHA-256 (`INSTALL.txt`), run `install.ps1 -Launch`. Then check:
   - Start menu shows windowsRICE and windowsRICE Design Studio;
   - Settings › Apps lists windowsRICE 0.1.0-rc.1;
   - uninstall from there, and `%APPDATA%\windowsRICE` remains.
2. **Narrator:** start Narrator (Ctrl+Win+Enter), press Ctrl+Alt+B, Tab through the bar, open the menu (Enter), move through the settings and the Design Studio galleries with Tab and arrows. Each item should be announced with name and state.
3. **High contrast:** Settings › Accessibility › Contrast themes; the bar, popup and Studio should use system colours.
4. **Animation effects off:** Settings › Accessibility › Visual effects; menus and Apply should not fade.
5. **DPI:** change Scale while the bar and the Studio are open; text stays sharp and layouts fit; maximised windows start below the bar.
6. **Monitors:** connect a second monitor (a second bar appears); turn it off in Studio › Advanced and Apply; let a monitor sleep and wake (its setting should still apply); disconnect (the bar is removed cleanly).
7. **Korean IME:** in a Studio text field (Modules › custom text, or Presets › name), type Korean with the IME; the committed text appears.
8. **Clipboard:** in a Studio text field, use Ctrl+A, Ctrl+C, Ctrl+X and Ctrl+V.
9. **Auto-hide:** with the pointer away from the top edge, turn on "Hide automatically"; the bar collapses to a 2 px strip after the delay and reopens at the top edge.

## 9. Test results (final code)

- **Level 3** (`scripts/check.ps1 full`): fmt, check of all targets, Clippy with `-D warnings`, **250 tests passed, 0 failed**, release build.
- **Debug build:** succeeded.
- **Real application:**
  - end-to-end 33/33;
  - Studio 51/51;
  - bar accessibility 58 passed, 0 failed, 1 not run;
  - Phase 4/5 checks 15/15;
  - package and installer 28/28;
  - screenshots: 0 errors, 0 accessibility problems.

## 10. Release candidate outputs

- **Executable:** `target\release\windowsrice-panel.exe` (development build); the shipped build is `target\package\release\windowsrice-panel.exe`.
- **Package:** `target\dist\windowsRICE-0.1.0-rc.1-win-x64.zip` and `.sha256`.
- **Rebuild:** `powershell -ExecutionPolicy Bypass -File scripts\package.ps1`, then `scripts\package-verify.ps1`.

Older local artefacts (`windowsRICE-0.1.0-*`) remain in `target\dist` and are not part of the release candidate.

## 11. Limitations

- Not code-signed; no MSI or MSIX.
- No blur behind the bar.
- Stable monitor identity depends on Windows reporting a device path; when it does not, settings follow the device name.
- A monitor whose device path changes (for example a cable moved to another port) starts from the default settings.
- Manual and hardware items in §6 are NOT VERIFIED.

## 12. Files

- **New:**
  - `crates/windowsrice-core/tests/monitor_identity.rs`;
  - this report and `docs/phase10/` (evidence).
- **Modified:**
  - Packaging: `packaging/install.ps1`, `packaging/uninstall.ps1`, `packaging/INSTALL.txt`.
  - Scripts:
    - `scripts/package.ps1` (clean build, path check);
    - `scripts/package-verify.ps1` (28 checks);
    - `scripts/bar-a11y.ps1` (not-run reporting).
  - Code:
    - `crates/windowsrice-core/src/bar.rs` (identity matching);
    - `crates/windowsrice-windows/src/display.rs` (second identity source);
    - `apps/panel/src/controller.rs` (connected monitors);
    - `apps/panel/src/studio_win.rs` (atomic `studio.toml`);
    - `crates/windowsrice-ui/src/popup.rs` (two-column themes);
    - `crates/windowsrice-ui/src/studio/mod.rs` (monitor toggle, preview crop);
    - `crates/windowsrice-ui/src/studio/gallery.rs` (switch width).
  - Tests: `crates/windowsrice-ui/tests/bar.rs`, `crates/windowsrice-ui/tests/studio.rs`.
  - `Cargo.toml` and `Cargo.lock` (version 0.1.0-rc.1), `README.md`.

## 13. Status

**Phase 10 status: COMPLETE as a release candidate** for everything that can be verified on this machine:
- automated checks pass;
- Debug and Release builds succeed;
- install and uninstall are verified in a sandbox;
- the package contents and checksum are verified;
- Phase 7–9 behaviour is intact.

Hardware and manual items (§6, §8) are NOT VERIFIED, and the release is not code-signed.
