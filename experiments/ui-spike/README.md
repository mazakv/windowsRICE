# Phase 3.5 UI framework spike (EXPERIMENTAL)

This folder contains throwaway prototypes used to choose the UI technology for the windowsRICE desktop surfaces. Nothing here is product code, nothing here is distributed, and no production crate depends on it. It is a separate Cargo workspace with its own `Cargo.lock`, so neither candidate framework is a dependency of the production workspace.

## Candidates

- `slint-bar`: Slint 1.19.0 with the winit backend. Run with `--renderer femtovg` (default, OpenGL) or `--renderer software` (CPU). Slint is used under GPL-3.0 for internal evaluation only.
- `d2d-bar`: Direct2D 1.1 + DirectWrite + DirectComposition through `windows-rs` 0.62.

Both render the same 34 px top bar from the windowsRICE Default dark tokens, loaded from the production `windowsrice-theme` crate: four workspace buttons, a mixed Korean/English window title, a clock with a Korean date, three Segoe Fluent Icons glyphs and an input indicator. They share content, geometry, DWM window treatment (tool window, topmost, dark frame, rounded corners, Acrylic) and instrumentation through `common`. `shot` is a DPI-aware screen-region capture helper.

## Command-line options (both candidates)

`--exit-after <s>`, `--stats <file.json>`, `--scale <factor>` (forced UI scale), `--backdrop acrylic|none`, `--interaction-test` (changes the selection every 2 s), `--renderer femtovg|software` (Slint only).

## Reproducing

Builds use a project-local Cargo home so that no files are written outside the project:

```powershell
$env:CARGO_HOME = "$PWD\.cargo-home"; $env:TMP = "$PWD\tmp"; $env:TEMP = $env:TMP
cargo build --release
pwsh -File bench\run-bench.ps1                                  # about 2 h 20 min
pwsh -File bench\analyze.ps1 -RunDir results\bench\<run-id>
powershell.exe -ExecutionPolicy Bypass -File bench\test-a11y.ps1 -Root $PWD   # UIA tree + keyboard
```

`test-a11y.ps1` must run under Windows PowerShell 5.1: in PowerShell 7 two copies of the UI Automation assembly clash. It posts key messages to the prototype's own window handle only; it does not inject global input.

## Results

See `results/` (screenshots, accessibility trees, benchmark data) and the decision record `docs/adr/ADR-0002-ui-framework.md` at the project root. Direct2D + DirectWrite + DirectComposition was selected; this folder is kept as the evidence for that decision and is not built by the production workspace.
