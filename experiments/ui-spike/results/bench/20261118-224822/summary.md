# UI spike benchmark 20261118-224822
Windows 10.0.26200.9974 (25H2); AMD Ryzen 9 5900X 12-Core Processor; AMD Radeon RX 9070 XT driver 32.0.32015.7029; 전원 구성표 GUID: 381b4222-f694-41f0-9685-ff5bb260df2e  (균형 조정)

## Startup (median, min–max)
| Config | Launches | Window visible (ms) | First frame since process creation (ms) |
|---|---|---|---|
| slint-femtovg | 10 | 41.5 (31.2–243.4) | 437.2 (429.7–458.2) |
| slint-software | 10 | 32 (31.3–47.7) | n/a |
| d2d | 10 | 32.5 (31.7–78.3) | 119.1 (117.6–134) |

## Interaction (selection change every 2 s, 180 ms animation)
| Config | Samples | State change → frame (ms), median / p95 | Frames per animation (median) | Animation frame rate (fps, median) |
|---|---|---|---|---|
| slint-femtovg | 29 | 0.4 / 0.6 | 31 | 167.7 |
| slint-software | 0 | n/a (renderer does not report frames) | n/a | n/a |
| d2d | 31 | 0.7 / 0.9 | 32 | 177.5 |

## Idle (per repetition: 10-minute window after 5-minute warm-up; table shows median across repetitions with min–max)
| Config | Reps | CPU (% of one core) | CPU (% of all 24 threads) | Working set (MiB) | Private bytes (MiB) | GPU util (% median of samples) | GPU util max (%) | Dedicated GPU mem (MiB) | Shared GPU mem (MiB) | Redraws in window | Redraws/s | System CPU during run (%) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| slint-femtovg | 3 | 0.0026 (0–0.0078) | 0.00011 (0–0.00032) | 218.3 (217.8–219.3) | 258.6 (258.4–258.8) | 0 (0–0) | 0 (0–0.01) | 123.7 (123.6–123.7) | 109.2 (109.1–109.7) | 10 (10–10) | 0.0166 (0.0166–0.0166) | 1.8 (1.5–4.3) |
| slint-software | 3 | 0.0026 (0–0.013) | 0.00011 (0–0.00054) | 22.2 (22.2–22.7) | 4.1 (4–4.4) | 0 (0–0) | 0 (0–0) | 0 (0–0) | 0 (0–0) | n/a (not reported) | n/a | 1.5 (1.4–2.5) |
| d2d | 3 | 0.0026 (0–0.0181) | 0.00011 (0–0.00076) | 47 (46.5–47.1) | 38.1 (37.4–38.3) | 0 (0–0) | 0 (0–0) | 19.3 (19.3–19.3) | 5.5 (5.2–5.5) | 11 (10–12) | 0.0183 (0.0166–0.0199) | 2.2 (2–2.8) |

