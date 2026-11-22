# Changelog

All notable changes to windowsRICE. Versions follow [Semantic Versioning](https://semver.org/); pre-release versions (`-rc.N`) are for testing. The release workflow copies the section of the released version, and the "Known limitations" section, into the GitHub Release notes.

## [Unreleased]

## [0.1.0-rc.2] - 2026-11-22

Second release candidate: the creator-focused Design Studio, design sharing, interactive modules, the bottom bar and animated colours.

### Added
- **Design Studio** in six groups: Templates, Bar layout, Appearance, Motion & media, My designs and Advanced. Templates is the starting page.
- **26 complete templates**, each with its theme, colours, typography, module tiles, icons, effects and module layout. Applying one keeps each module's own settings; Undo returns.
- **Create custom design** starts from scratch (menu and clock only) in Bar layout.
- **RICE Design Code**, a versioned declarative JSON format (`docs/design-code.md`):
  - **Import design code** pastes code or opens a `.json`/`.ricepack` file;
  - **Analyze** reports problems with severity, location (line and column), an explanation and a fix;
  - a line editor fixes the selected line;
  - **Preview**, **Save to My designs** and Apply use the result;
  - **Copy design code** and **Export .ricepack** share a design.
- **`.ricepack` packages:**
  - stored-only ZIP (no decompression bombs);
  - checked entry names (no traversal, absolute paths or symbolic links) and CRC-32;
  - files identified by content, not extension; size, resolution and length limits;
  - assets copied into the design library under generated names;
  - optional Microsoft Defender scan, reported as such.
- **Interactive modules:** CPU, GPU, VRAM, RAM, network, volume, battery, clock and notifications open panels:
  - details with a 60-sample history graph;
  - volume and mute controls;
  - a calendar with month navigation, today, selection and extra time zones;
  - a notification center.
- **Notification center:** explains that Windows notifications need package identity this installation does not have, and keeps windowsRICE's own notices with an unread badge (1–99, 99+).
- **Bottom bar** (`[bottom_bar]`):
  - its own modules, look, height, monitors and auto-hide;
  - registered on the bottom edge so the shell places it above the Windows taskbar;
  - registers again after an Explorer restart;
  - panels open above it;
  - the Studio edits either bar.
- **Colour animations:** animated text, rainbow text, accent, icons, clock and a scrolling background gradient.
  - They are fitted for readability.
  - They run only while visible and when Windows "Animation effects" is on.
  - The **Animation** page has a keyframe timeline, presets, interpolation, direction, loop and speed.
- **Image backgrounds and video poster frames** from the design library:
  - **Image and video** page;
  - the frame is chosen in one-second and single-frame steps.
- **Project:**
  - MIT license and third-party notices (shipped in the zip);
  - contribution and security policies;
  - GitHub Actions `CI` and `Release`, and `scripts/release.ps1`.
- **Startup milestones** in `--stats` and `scripts/startup-profile.ps1`.

### Fixed
- Choosing a light theme or mode could leave dark text on a dark background. Text, secondary text, icons, clock and per-module colours are now checked against what they are drawn on, corrected to the nearest readable colour, and the correction is reported.
- Startup: the first shell notification-state query (~36 ms) no longer delays the first frame.
- The Design Studio fits 1024×768 screens and 1366×768 laptops (minimum 1000×680; it was 1180×760); the theme editor's Save button was cut off on small windows.

### Changed
- The verification scripts work on any screen size and DPI, scroll for off-screen elements and wait for asynchronous actions.

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
- Not verified by a person: Narrator, Windows high-contrast themes, real DPI changes, mixed-DPI and multi-monitor setups, monitor hot-plugging, Korean IME input by hand, a real Explorer restart, a video poster frame from a real MP4 file.
- Windows notifications cannot be read or counted (no package identity); the notification center says so.
- Videos are shown as a still frame; continuous playback, trimming and playback speed are not implemented.
- The CPU panel does not show per-core usage or the current frequency; GPU temperature is not shown (no documented interface).
- A monitor that Windows does not identify (for example in standby) is matched by its display name.
- No blur behind the bar; translucency is drawn by the bar itself.
- Installation is per user only (no MSI or MSIX).
