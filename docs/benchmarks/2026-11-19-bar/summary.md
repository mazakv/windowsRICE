# windowsRICE panel performance (Full, 20261119-155925)
Windows 10.0.26200.9974; AMD Ryzen 9 5900X 12-Core Processor            ; AMD Radeon RX 9070 XT driver 32.0.32015.7029. Warm-up 60 s, window 120 s, 3 repetition(s), sampled every 5 s. Values are medians with min–max across repetitions.

| Startup | Value |
|---|---|
| Process creation → first presented frame (ms), 10 launches | 178.2 (174.1–194.7) |
| Device + swap chain + composition creation (ms) | 64.5 (62.3–69.4) |
| Input: mouse-move message → frame presented (ms), 20 events | median 1.12, p95 1.44, max 1.62 |
| Frame draw time (ms, all frames of the input run) | median 0.38, p95 0.53 |
| Present call (ms) | median 0.2, p95 0.31 |

| Idle config | Reps | CPU (% of one core) | Working set (MiB) | Private (MiB) | Handles | GPU util (%) | GPU util max (%) | Dedicated GPU (MiB) | Shared GPU (MiB) | Redraws/min | Sampler cost (ms) | UIA tree fetch (ms) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| default | 3 | 0.413 (0.376–0.493) | 56.1 (56–56.3) | 47.7 (47.7–48.2) | 1250 (1250–1252) | 0.003 (0–0.004) | 0.01 (0–0.01) | 21.1 (21.1–21.1) | 3.8 (3.6–3.8) | 37.1 (34.9–41.3) | n/a | n/a |
| no-gpu-counter | 3 | 0.337 (0.285–0.389) | 49.9 (49.7–49.9) | 41.8 (41.7–42) | 1242 (1242–1244) | 0.004 (0.003–0.007) | 0.01 (0–0.01) | 21.1 (21.1–21.1) | 3.7 (3.6–3.7) | 51.3 (33.9–56.8) | n/a | n/a |
| uia-client | 3 | 0.505 (0.441–0.531) | 58.2 (58.1–58.3) | 48.8 (48.6–49.1) | 1272 (1271–1276) | 0.006 (0.004–0.007) | 0.01 (0.01–0.01) | 21.1 (21.1–21.1) | 3.8 (3.7–3.8) | 53.8 (34.8–54.7) | n/a | 6.6 (6.5–6.7) |

