# windowsRICE development guide: testing and verification

This guide defines how much verification each kind of change needs. The aim is to avoid repeating slow work that cannot find new problems, without weakening what is checked before a feature or phase is declared complete. No test may be deleted, weakened, skipped or hidden to save time, and a focused run must never be reported as full workspace verification.

## Where the time actually goes

Measured on the development machine (Ryzen 9 5900X, 2026-11-19) for the workspace as of Phase 3:

| Activity | Time |
|---|---|
| Cold dev build of all test targets | 32.9 s |
| Cold release build of the workspace | 27.9 s |
| No-op rebuild (nothing changed), dev or release | about 1.5 s |
| Executing all 123 tests | under 0.5 s in total, plus about 3 s of Cargo overhead |
| Full UI-framework benchmark (Phase 3.5) | about 2 h 20 min |

The slowest compilation units in a cold build are `windows` 0.62.2 (11–13 s), `time`, `syn`, `serde_core` and `toml`. These are compiled once and then cached. Test execution is not a bottleneck. In earlier phases the cost came from running the complete sequence (check, clippy, Debug tests, Debug build, Release build, Release tests) after every small change, from rebuilding the Slint spike workspace, and from the long benchmark. Re-measure with `cargo build --timings` (report in `target/cargo-timings/`) when compilation seems slow, before changing any build setting.

## Build cache

Cargo's defaults are in effect: there is no `.cargo/config.toml` and no `CARGO_*` or `RUSTFLAGS` variable, so incremental compilation is on for dev and test builds and off for release builds. Keep it that way unless a measurement shows a reason to change it. Do not run `cargo clean` or delete `target/` as part of normal work, and do not change dependency versions only to change build times. The release profile uses `lto = "thin"` and `codegen-units = 1` for runtime performance. This makes release builds the most expensive step, which is why they belong to the full level only. Cargo's default parallelism (one job per logical CPU) is used. Do not tune it without measurement.

## The three levels

`scripts/check.ps1` runs each level, prints the exact Cargo commands, records each step's duration and test counts (including how many tests a filter excluded), stops at the first failure, and states what was deferred. It keeps Cargo's temporary files inside `target/tmp`.

### Level 1: quick check, during development

Check and test only the packages touched by the change, optionally with a test-name filter while working on one behaviour.

```powershell
pwsh -File scripts/check.ps1 quick -Package windowsrice-theme
pwsh -File scripts/check.ps1 quick -Package windowsrice-theme -Filter activation
pwsh -File scripts/check.ps1 quick -Changed          # packages with files changed since HEAD
```

The equivalent Cargo commands are `cargo check -p <pkg> --all-targets` and `cargo test -p <pkg> [filter]`. Formatting, Clippy, other packages and release builds are deferred.

### Level 2: integration check, when a feature is substantially complete

This level runs formatting, `check`, Clippy with `-D warnings`, and the tests of the affected packages. If the change touches a shared interface, the configuration schema, theme data or other cross-crate behaviour, include the dependent packages with `-WithDependents`. Run the relevant real Windows check at this point when the feature involves Windows integration.

```powershell
pwsh -File scripts/check.ps1 integration -Package windowsrice-core -WithDependents
```

### Level 3: full check, before a phase is reported complete

```powershell
pwsh -File scripts/check.ps1 full                # fmt, check, clippy, test, release build
pwsh -File scripts/check.ps1 full -ReleaseTests  # also cargo test --workspace --release
```

This runs `cargo fmt --all -- --check`, `cargo check --workspace --all-targets`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` and `cargo build --workspace --release`. Release-mode tests are added for release validation, or when a change could behave differently under optimization (unsafe code, timing, FFI layout). A change to `Cargo.toml` or `Cargo.lock` at the workspace root always calls for the full level.

## Kinds of tests

Every report states which kind of evidence supports each claim.

| Kind | What it is | When it runs |
|---|---|---|
| Unit tests | Pure Rust logic (configuration, themes, layout, accessibility model, contrast maths). No Windows state. | Quick level, on every change to the package |
| Simulated integration tests | Real code paths driven by synthetic input: forced UI scale, simulated device loss, synthetic accessibility trees. | Integration level for the affected feature |
| Real Windows 11 Pro tests | Running the actual program on this machine: windows, DPI, DirectWrite output, UI Automation queried from another process, display changes. | When the related Windows integration code changes, and at milestones |
| Manual checks | Visual inspection of screenshots, screen-reader (Narrator) use, scaling changes the owner makes in Settings. | Major milestones. Claims stay NOT VERIFIED until done. |

Hardware and system-query tests (for example `windowsrice-windows` and `windowsrice-diag`) are not rerun after theme or layout changes that cannot affect them. Tests and checks never change Windows settings, the registry, startup configuration or security settings, even when that would make testing faster.

## Real-application scripts

These drive the release binary from another process, with a project-local configuration (`--config`), and write their results under `target/`. Run them with Windows PowerShell (`powershell.exe -File …`) after `cargo build -p windowsrice-panel --release`, when the related code changes and at phase ends.

| Script | What it checks |
|---|---|
| `scripts/bar-a11y.ps1` | The bar's and popup's UI Automation tree, focus, keyboard, menu state and exit |
| `scripts/bar-visual.ps1` | Bar screenshots at several scales, themes and languages |
| `scripts/phase45-verify.ps1` | Hide and resume, Alt+F4, real overflow, and contrast over black, white and colourful backdrops |
| `scripts/studio-verify.ps1` | The Design Studio: sections, colours, Apply to the real bar, drag and drop, themes, presets with the Windows file dialogs, confirmations, close behaviour and keyboard |
| `scripts/design-e2e.ps1` | End-to-end Apply: style, palette, font, background, module appearance, layout, icons and effects, each checked on the real bar, then a preset and a restart |
| `scripts/studio-shots.ps1` | Screenshots of every Studio section (`-Lang ko`, `-Appearance false` for Korean light) |
| `scripts/perf-quick.ps1` | Startup (3 runs), 60 s idle (CPU, memory, GPU memory, redraws, frame cost) and Design Studio memory over two open and close cycles; no screenshots |
| `scripts/package.ps1` | Release build and `target\dist\windowsRICE-<version>-win-x64.zip` with a SHA-256 file |
| `scripts/package-verify.ps1` | Installs, upgrades and uninstalls the zip in a sandbox under `target\pkgtest` (no registry writes); refuses to run while any windowsRICE runs |

Message-box buttons are not exposed with an Invoke pattern, and the Save and Open dialogs draw their name box with DirectUI. So `studio-verify.ps1` presses dialog buttons by control ID and fills the file name through the classic child windows. It never sends global input.

**Privacy.** Since Phase 7 the bar can be translucent, so anything behind it shows through. Every script that captures the bar keeps `scripts/backdrop-window.ps1` (a test pattern pinned to the top edge) behind it and refuses to capture unless that window is in place, and every run uses a fixed window title (`--title`). Studio captures use the window's visible frame bounds inset past its rounded corners. Desktop content must never appear in evidence.

## Accessibility testing

Accessibility tests are never removed to save time. During development, run the unit tests of the accessibility components that changed. At major milestones, verify the complete UI Automation tree from a separate process, keyboard navigation and focus behaviour, and real screen-reader behaviour. Unit tests alone never justify calling accessibility VERIFIED.

## Performance checks

The Slint versus Direct2D comparison is finished (ADR-0002) and is not repeated during normal development.

- **Quick performance check.** Use a 30–60 s run of the affected binary, measuring only what the change could affect (for example private memory and redraw count after a rendering change). Results go to `target/perf-quick/` and are not committed, because they are regression signals, not evidence.
- **Full performance verification.** Before completing a rendering or performance phase, run with a warm-up, repeated measurement windows and medians, covering CPU, memory, GPU utilization and memory, redraw frequency, input responsiveness and frame pacing. Results are kept under `docs/benchmarks/<date>-<topic>/` together with the environment description. The 5-minute warm-up, 10-minute window, three-repetition method of Phase 3.5 is reserved for major milestones and release validation.

Documentation edits, colour-token changes and unrelated logic changes need no performance run.

## Required routine for every change

1. Identify the modified packages.
2. Choose the level.
3. Run the focused checks and fix failures.
4. Widen the scope when cross-crate behaviour may be affected.
5. Run the full level before reporting a phase as complete.

Every report lists the checks that were run and the checks that were deliberately deferred.
