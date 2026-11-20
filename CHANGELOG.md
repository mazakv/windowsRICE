# Changelog

All notable changes to windowsRICE. Versions follow [Semantic Versioning](https://semver.org/); pre-release versions (`-rc.N`) are for testing. The release workflow copies the section of the released version, and the "Known limitations" section, into the GitHub Release notes.

## [Unreleased]

### Added
- MIT license, third-party notices (`THIRD-PARTY-NOTICES.txt`, shipped in the zip), contribution and security policies.
- GitHub Actions: `CI` (format, Clippy, tests, Debug and Release builds, application checks, package and installer verification) and `Release` (builds a version tag, verifies it and publishes a GitHub Release with the zip and its SHA-256).
- `scripts/release.ps1`: verifies the tree, version, checks, package and checksum before tagging.
- Startup milestones in the `--stats` output and `scripts/startup-profile.ps1` (warm and fresh-copy launches, per-stage breakdown, budget).

### Fixed
- Startup: the first shell notification-state query (~36 ms) no longer delays the first frame; the Design Studio font list is built after the first frame.

### Changed
- The verification scripts derive screen width and scale from the primary monitor instead of assuming 3840 px at 150 %.

## [0.1.0-rc.1] - 2026-11-20

First release candidate (local build only; not published).

### Added
- Top bar with Direct2D, DirectWrite and DirectComposition rendering, UI Automation, per-monitor DPI, work-area reservation, auto-hide and fullscreen hiding; 15 module kinds.
- Design Studio: visual galleries for styles, themes, palettes, fonts, backgrounds, icons, modules, layouts and effects; live preview; presets; undo and redo; per-module appearance; motion.
- Per-user installer and uninstaller with upgrade, interruption recovery and settings protection; package verification.

### Fixed (release candidate audit)
- Local paths removed from the executable; installer and uninstaller no longer delete unrelated files; settings removal requires confirmation; monitor settings follow the physical monitor safely; the settings popup fits 1366×768.

## Known limitations

- Builds are **not code-signed**; Windows SmartScreen may warn. Verify the SHA-256.
- Not verified by a person: Narrator, Windows high-contrast themes, "Animation effects" off, real DPI changes, mixed-DPI and multi-monitor setups, monitor hot-plugging, Korean IME input by hand.
- A monitor that Windows does not identify (for example in standby) is matched by its display name.
- No blur behind the bar; translucency is drawn by the bar itself.
- Installation is per user only (no MSI or MSIX).
