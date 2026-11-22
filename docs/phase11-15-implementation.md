# Phases 11–15: creator Studio, customizable bars, GitHub releases

This report covers the work from the Phase 10 release candidate (0.1.0-rc.1) to 0.1.0-rc.2. Each item is marked **implemented and tested** (automated tests and/or checks on the running application), **partially implemented**, or **NOT VERIFIED** (needs a person, other hardware, or a system change that tests must not make).

## 1. GitHub setup (Phase 11)

- **Initial state:** branch `master` at `20981ec`. Phase 3.5–10 work was uncommitted (13 modified, 25 untracked paths). The repository `mazakv/windowsRICE` was private and empty, with `main` as default branch; the authenticated account `mazakv` had admin rights.
- **Audit before the first push:**
  - no secrets, keys or build output;
  - screenshots show only the test backdrop and generic window titles;
  - 44 recorded-evidence files contained the local user path, which was replaced by `<project>` and `%USERPROFILE%`.
- **Push:** remote `origin` added, `master` pushed (no force push, nothing overwritten), and `master` made the default branch. The repository stays **private**.
- **Actions settings:** only GitHub-owned actions are allowed, SHA pinning is required, the default token is read-only, and workflows cannot approve pull requests.

### Commits

| Commit | Content |
|---|---|
| `dcdae6c` | Phases 3.5–10 (existing work) |
| `4c0fadd` | Phase 11: startup milestones and first-frame fix |
| `aee9fbb` | Phase 11: MIT license, CI and release automation |
| `2be58cf`, `a1df9aa` | Phase 12: readable text across themes and modes |
| `6ddf859` | Phase 12: Studio in six groups; CI builds the screenshot tool |
| `9a16579` | Phase 12: 26 templates; readable text on tiles |
| `eaa13ae` | Phase 12: RICE Design Code, `.ricepack`, custom designs |
| `8f97680` | Phase 13: interactive modules, calendar, notification center |
| `af9afd0` | Phase 14: bottom bar; robust checks on small screens |
| `ba42617` | Phase 14: colour animations and editor |
| `d27f19d` | Phase 14: image and video-frame backgrounds; small-window fixes |
| (release commit) | Phase 15: 0.1.0-rc.2, documents |

## 2. License

`LICENSE`: MIT, copyright 2026 mazakv; every crate declares `license = "MIT"`. Third-party crates are **not** all MIT:
- most are MIT or Apache-2.0 (used under MIT where a choice is offered);
- `unicode-ident` adds Unicode-3.0, `memchr` also offers Unlicense, and `r-efi` also offers LGPL.

`scripts/third-party-notices.ps1` generates `packaging/THIRD-PARTY-NOTICES.txt` from each crate's own license files (52 crates), and CI fails if it is out of date. The zip ships `LICENSE.txt` and `THIRD-PARTY-NOTICES.txt`. Also added: `CONTRIBUTING.md`, `SECURITY.md`, `CHANGELOG.md` and `docs/releasing.md`.

## 3. CI and release automation

- **`CI`** (`.github/workflows/ci.yml`, `windows-2025`, `contents: read`): runs on pushes to `master`, pull requests, `v*` tags and manual start. Steps:
  - format, Clippy with `-D warnings`, workspace tests and release build;
  - Debug build;
  - third-party notices check;
  - runner screen and session report;
  - application checks (`scripts/app-tests.ps1`: end-to-end Apply, Design Studio, bar and popup UI Automation, Phase 4/5, module panels, bottom bar, background image);
  - package and installer verification;
  - check results kept as artifacts.
- **`Release`** (`.github/workflows/release.yml`), on a `v*` tag:
  - the build job (`contents: read`) checks out the exact tag, requires it to equal the Cargo version, runs every CI check, builds the zip, generates and verifies the SHA-256 and writes the notes (changelog section, install steps, checksum, known limitations, signing status);
  - the publish job, the only one with `contents: write` and running no project code, verifies the checksum again and creates the release;
  - pre-release tags are published as pre-releases; stable tags become drafts.
- **Actions:** `actions/checkout`, `cache`, `upload-artifact` and `download-artifact`, pinned to commit SHAs. No secrets, no external services.
- **`scripts/release.ps1 -Version X -Prerelease`:** refuses a dirty tree, a branch other than `master`, a `master` not level with `origin/master`, a version mismatch, a missing changelog section or an existing tag. It runs all checks, the package, the checksum and the installer verification before it creates and pushes the annotated tag. It never force-pushes.
- **CI history:** the first runs failed for environmental reasons, each fixed at its cause:
  - the screenshot tool was not built;
  - one runner image had Rust 1.99.1 (the stable toolchain is now updated first);
  - the 1024×768 runner screen exposed two product defects (§6) and script assumptions about screen size and timing.

## 4. Startup regression (Phase 11)

Startup now records milestones from process creation to the first presented frame in `--stats` (precise clock), and `scripts/startup-profile.ps1` reports warm and fresh-copy launches with a per-stage breakdown and a 250 ms budget.

- **The Phase 10 figure (261–296 ms) did not reproduce** under controlled conditions; the data is kept in `docs/phase11/`.
- **Stage costs:**
  - graphics device creation ~70 ms;
  - AppBar position negotiation and window placement ~20 ms, depending on Explorer;
  - monitor identity ~1.3 ms.
- **Confirmed cause of extra delay:** the first `SHQueryUserNotificationState` call in the process (fullscreen detection) took ~36 ms on the critical path. Before the first frame only the geometric fullscreen test runs now, and the shell query follows right after. The Studio font prefetch starts after the first frame.
- **Results:** warm median ~180 → ~165 ms after the fix (Phase 11). With all Phase 12–14 features the final build measures **194.5 ms warm and 199.9 ms on freshly copied executables** (7 and 3 runs, budget passed); `perf-quick` measured 215, 225 and 215 ms.
- **A true cold start** after a reboot is NOT VERIFIED.

## 5. Design Studio, templates and readability (Phase 12)

- **Implemented and tested:**
  - six groups (Templates, Bar layout, Appearance, Motion & media, My designs, Advanced); the open group lists its pages, Templates is the starting page;
  - **26 complete templates** (15 earlier styles plus Dark Glass, Paper Light, Aurora, Neon Night, Gaming, Productivity, Sunset Gradient, Cyber Terminal, Elegant, Ocean Breeze and Forest). Each sets theme, mode, colours, typography, tiles, icons, effects and module layout; modules keep their own settings; Apply and Undo as before;
  - **Create custom design:** starts with the menu and clock, in Bar layout › Modules.
- **Light theme fix (implemented and tested):**
  - **Cause:** the mode switch kept the previous design's gradient and colours.
  - **Palette roles:** text roles are checked against the surfaces they are actually drawn on (the background, or the gradient stops that replace it).
  - **Drawn colours:** module text and icons are fitted to their tile as drawn, and clock, value, icon, per-module and animated colours to the surface under them.
  - **Correction:** a failing colour becomes the nearest readable one (4.5:1 text, 3:1 icons), moving towards white or black and keeping the hue.
  - **Reporting:** the Studio reports every correction, next to the colour editor too; the configuration is not changed.
  - **Tests:** every template is switched to Light and Dark mode and every text and icon draw command of the real bar is checked.

## 6. RICE Design Code and safe import (Phase 12)

**Implemented and tested** (`docs/design-code.md`, `core::design_code`, `core::design_pack`, 23 unit tests, 13 checks on the running Studio).

- **Format:** versioned, strictly declarative JSON (theme, `top_bar`, `bottom_bar`, `assets`); unknown fields are errors.
- **Analyzer:** reports every finding with severity, path, line and column, explanation and fix. It checks:
  - syntax, format and version, unknown and missing fields, types;
  - duplicate ids, references, colours and ranges;
  - installed fonts and themes;
  - animation limits, text length and sizes;
  - script and command content, web and file addresses, absolute and parent paths, executable file types.
- **Import page:**
  - paste, or open `.json`/`.ricepack`;
  - Analyze, then a line editor fixes the selected problem's line;
  - Preview loads into the draft (Undo returns; nothing is written before Apply);
  - Save to My designs;
  - Copy design code and Export `.ricepack`.
- **`.ricepack`:**
  - stored-only ZIP, so no decompression bombs;
  - entry names checked (no traversal, absolute paths, drive letters, backslashes, symlinks, duplicates or unexpected files), CRC-32 verified;
  - files identified by content (PNG/JPEG signatures, MP4 `ftyp`), not extension;
  - limits: 64 MB, 16 files, images 16 MB and 7680×4320, videos 48 MB, 3840×2160 and 120 s (read from the MP4 header);
  - assets copied into the design library under content-derived generated names.
- **Statuses** are stated plainly: syntax, schema, "No executable content accepted", asset checks, antivirus scan (not checked / passed / failed / unavailable) and "Review required". An optional **Microsoft Defender** scan (`MpCmdRun -DisableRemediation`) of an opened package is offered. A design is never labelled malware-free.
- **NOT VERIFIED:** the Defender scan of a real package on this machine (the button is present; no scan result was recorded).

## 7. Bars and modules (Phase 13)

- **Implemented and tested:**
  - the Bar layout editor from earlier phases: regions, drag and drop, order, show and hide, add and remove, width, labels, per-module appearance;
  - templates and design code change layouts;
  - removing a module never affects its provider.
- **Module panels (implemented, 17 checks on the running bar).** Clicking a module, or pressing Enter on it, opens:
  - CPU: usage, 60-sample history, name, cores and threads, base frequency;
  - GPU and VRAM: usage, adapter, used, free and total memory;
  - RAM: used, available, total;
  - Network: connection, rates, totals since start;
  - Volume: slider value, ±5 buttons, mute;
  - Battery: charge, power, charging.

  Escape and the close button close a panel. Values not measured (per-core CPU, current frequency, GPU temperature) are marked "not available", never invented.
- **Calendar (implemented and tested):** month grid, today, selection, previous/next month, Today, extra time zones (clock `zones`, Windows time zone ids, read-only lookup) and Open Windows Clock.
- **Notification center:**
  - `UserNotificationListener` requires package identity (an MSIX declaring the capability) and the user's consent. The unpackaged per-user installation has neither, so windowsRICE **cannot read or count Windows notifications**; the panel explains this and opens Windows' own notifications and settings.
  - windowsRICE's own notices (for example "Design applied") are kept separately with an unread badge (none, 1–99, 99+).
  - Reading Windows notifications is **not implemented** (it needs MSIX packaging).
- **Small screens (fixed):** the Studio minimum is now 1000×680 (was 1180×760, larger than a 1366×768 work area), and the theme editor's Save button could be cut off.

## 8. Bottom bar, animations and backgrounds (Phase 14)

- **Bottom bar (implemented and tested; 8 checks on this machine and on the CI runner):**
  - `[bottom_bar]` with its own modules, look, height, monitors and auto-hide; `enabled` hides a bar and keeps its settings;
  - registered on the bottom edge through `SHAppBarMessage` (`ABM_QUERYPOS`/`ABM_SETPOS`), so the shell places it above the Windows taskbar (verified on the CI runner with a visible taskbar), and it is removed on exit;
  - panels open above it;
  - `TaskbarCreated` registers both bars again (simulated by sending the broadcast; a real Explorer restart is NOT VERIFIED);
  - the Studio edits either bar.
  - NOT VERIFIED: multiple monitors and mixed DPI with two bars.
- **Animations (implemented and tested):**
  - targets: text, rainbow text, accent, icons, clock and scrolling background gradient;
  - keyframes, interpolation, direction, seamless loop;
  - limits: 1–120 s cycles (no flashing), at most 16 keyframes and 4 animations;
  - the **Animation** page has a timeline, presets, keyframe editing and a preview position;
  - a 20 fps clock runs only while a visible bar has an enabled animation and Windows "Animation effects" is on. That setting is **off on this machine**, and the bar correctly stayed still; `--force-motion` was used to measure: ~18 frames/s, 0.39% of one core over 20 s (0.31% without), draw median 0.78 ms.
- **Backgrounds (partially implemented):**
  - images from the design library are drawn scaled to cover the bar (verified on the running bar);
  - a video is shown as **one still frame** chosen in 1 s and single-frame steps on the **Image and video** page, decoded with Media Foundation;
  - continuous video playback, trim, loop, speed and scrubbing playback are **not implemented**;
  - the video frame decode was **NOT VERIFIED** (no MP4 test file on this machine).

## 9. Performance (release build, this machine)

| Measure | Phase 10 | rc.2 |
|---|---|---|
| Startup, warm median | 261–296 ms (not reproduced) | 194.5 ms (fresh copies 199.9 ms) |
| Idle CPU | 0.52% of one core | 0.52% of one core |
| Bar private / GPU memory | 46.8 / 21.1 MiB | 49.5 / 19.3 MiB |
| Studio open / after close (private) | 83.4 / 70.7 MiB | 89.9 / 75.2 MiB |
| Studio GPU after close | 20.6 MiB | 18.8 MiB |
| Studio scene build, median | 1.91 ms | 1.89 ms |

Closing the Studio still returns its GPU memory and handles, and a second open and close does not grow memory.

## 10. Tests

- **Workspace:** format, Clippy `-D warnings`, 290+ tests, Debug and Release builds.
- **Application checks** (on this machine and in CI):
  - end-to-end Apply 33;
  - Design Studio 64;
  - bar UI Automation 59;
  - Phase 4/5 15;
  - module panels 17;
  - bottom bar 8;
  - background image 3.
- **Package and installer verification:** 30 checks.
- **Intermittent failures:** the Studio checks showed occasional timing failures during development. The scripts now wait for asynchronous actions; two consecutive local runs passed after the change.

The final counts and the release run are listed in §12.

## 11. NOT VERIFIED (needs a person, hardware or system changes)

- Narrator, real high-contrast themes, real DPI changes and mixed DPI.
- Multiple monitors, hot-plugging and standby with both bars.
- Korean IME by hand.
- A real Explorer restart, and the bottom bar with the taskbar on another edge.
- A video poster frame from a real MP4; a Defender scan of a real package.
- The installer in the owner's real profile.
- A true cold start after a reboot.
- **Code signing:** not signed, since no certificate is available.

## 12. Release

Filled in after the release workflow ran (see the final report of this phase).
