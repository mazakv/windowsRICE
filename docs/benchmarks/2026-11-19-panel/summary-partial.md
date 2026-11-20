# windowsRICE panel performance: full run, stopped early (partial)

This run of `scripts/panel-perf.ps1 -Mode Full` was stopped on request after 5 of the 9 planned idle runs (3 configurations × 3 repetitions). Only completed runs are summarised here. The script's own `summary.md` was never produced, and no values are estimated. Raw files are in this folder.

## Method

- **Machine:** Windows 11 Pro 25H2 build 26200.9974, Ryzen 9 5900X, RX 9070 XT (driver 32.0.32015.7029), 3840×2160 at 150%.
- **Run conditions:** the active-window title was fixed with `--title`. Windhawk and AMD DLL injection were present, and other applications were running.
- **Idle runs:** each run had a 60-second warm-up and a 120-second window sampled every 5 s.
- **CPU:** process time delta over the window (from `log.txt`).
- **Memory:** working set, private bytes and handles are medians of the samples in each run.
- **GPU:** utilisation and memory come from the per-process `GPU Engine` and `GPU Process Memory` counters.
- **Redraws:** frames reported by the panel inside the window.

## Completed

| Measurement | Result |
|---|---|
| Startup, process creation → first presented frame (10 launches) | median 108.3 ms (101.0–117.9) |
| Device + swap chain + composition creation | median 64.4 ms |
| Input, mouse-move message → frame presented (20 events) | median 1.00 ms, p95 1.25 ms, max 1.29 ms |
| Frame draw / Present call (input run) | median 0.33 ms / 0.23 ms |

| Idle run | CPU (% of one core) | Working set (MiB) | Private (MiB) | Handles | GPU util (%) | Dedicated / shared GPU (MiB) | Redraws/min | UIA provider calls | UIA client full-tree fetch |
|---|---|---|---|---|---|---|---|---|---|
| default, rep 1 | 0.335 | 52.5 | 45.1 | 1217 | 0.003 | 21.1 / 3.6 | 22.5 | 285 | — |
| default, rep 2 | 0.156 | 52.6 | 45.0 | 1221 | 0.003 | 21.1 / 3.6 | 22.0 | 258 | — |
| no GPU counters, rep 1 | 0.195 | 46.3 | 38.5 | 1209 | 0.003 | 21.1 / 3.3 | 24.0 | 255 | — |
| no GPU counters, rep 2 | 0.117 | 46.6 | 38.7 | 1213 | 0.003 | 21.1 / 3.6 | 23.0 | 276 | — |
| UIA client every 5 s, rep 1 | 0.246 | 54.6 | 45.2 | 1240 | 0.003 | 21.1 / 3.8 | 22.0 | 5472 | median 5.8 ms |

## Not completed

The following runs did not complete: UIA client rep 2, and all three repetitions 3. With at most two repetitions per configuration, medians and variation across repetitions are not reported.

## Observations limited to these runs

- **GPU counters.** Reading the GPU performance counters costs about 6.5 MiB of private memory (45.0–45.1 vs 38.5–38.7 MiB). They are not the source of the roughly 1,200 handles.
- **UIA client.** A UI Automation client reading the whole tree every 5 s raised provider calls from about 270 to 5,472 per run and added about 2 MiB of working set. The client fetched 12 elements in a median of 5.8 ms.
- **Redraws.** These are driven by the 2-second readings (CPU, GPU and memory percentages) and the clock.
