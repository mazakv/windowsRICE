# Summarizes a benchmark run produced by run-bench.ps1 into summary.md and
# summary.json in the same folder. Reports medians with min–max ranges.

param([Parameter(Mandatory)][string]$RunDir, [int]$LogicalCpus = 24)
$ErrorActionPreference = 'Stop'

function Median([double[]]$v) {
    $v = @($v | Where-Object { $null -ne $_ } | Sort-Object)
    if ($v.Count -eq 0) { return $null }
    $m = [int][math]::Floor($v.Count / 2)
    if ($v.Count % 2) { return $v[$m] } else { return ($v[$m - 1] + $v[$m]) / 2 }
}
function P95([double[]]$v) {
    $v = @($v | Sort-Object); if ($v.Count -eq 0) { return $null }
    return $v[[int][math]::Min($v.Count - 1, [math]::Ceiling(0.95 * $v.Count) - 1)]
}
function Fmt($x, [int]$d = 2) { if ($null -eq $x) { 'n/a' } else { [math]::Round([double]$x, $d) } }
function Range([double[]]$v, [int]$d = 2) {
    $v = @($v | Where-Object { $null -ne $_ })
    if ($v.Count -eq 0) { return 'n/a' }
    "{0} ({1}–{2})" -f (Fmt (Median $v) $d), (Fmt ($v | Measure-Object -Minimum).Minimum $d), (Fmt ($v | Measure-Object -Maximum).Maximum $d)
}
$MiB = 1MB
$configs = 'slint-femtovg', 'slint-software', 'd2d'
$summary = [ordered]@{}
$md = [System.Text.StringBuilder]::new()
$envj = Get-Content (Join-Path $RunDir 'environment.json') -Raw | ConvertFrom-Json
[void]$md.AppendLine("# UI spike benchmark $($envj.run_id)")
[void]$md.AppendLine("Windows $($envj.windows); $($envj.cpu); $($envj.gpu) driver $($envj.gpu_driver); $($envj.power_plan)")
[void]$md.AppendLine('')

# ---- startup
$st = Import-Csv (Join-Path $RunDir 'startup.csv')
[void]$md.AppendLine('## Startup (median, min–max)')
[void]$md.AppendLine('| Config | Launches | Window visible (ms) | First frame since process creation (ms) |')
[void]$md.AppendLine('|---|---|---|---|')
foreach ($c in $configs) {
    $r = @($st | Where-Object config -eq $c)
    $vis = [double[]]@($r | ForEach-Object { if ($_.window_visible_ms) { [double]$_.window_visible_ms } })
    $ff = [double[]]@($r | ForEach-Object { if ($_.first_frame_ms) { [double]$_.first_frame_ms } })
    [void]$md.AppendLine("| $c | $($r.Count) | $(Range $vis 1) | $(Range $ff 1) |")
    $summary["startup_$c"] = @{ window_visible_ms = (Median $vis); first_frame_ms = (Median $ff) }
}
[void]$md.AppendLine('')

# ---- interaction
[void]$md.AppendLine('## Interaction (selection change every 2 s, 180 ms animation)')
[void]$md.AppendLine('| Config | Samples | State change → frame (ms), median / p95 | Frames per animation (median) | Animation frame rate (fps, median) |')
[void]$md.AppendLine('|---|---|---|---|---|')
foreach ($c in $configs) {
    $j = Get-Content (Join-Path $RunDir "interaction-$c.json") -Raw | ConvertFrom-Json
    $it = @($j.interactions | Where-Object { $_.anim_ms -gt 0 })
    if ($it.Count -eq 0) {
        [void]$md.AppendLine("| $c | 0 | n/a (renderer does not report frames) | n/a | n/a |")
        continue
    }
    $lat = [double[]]@($it.latency_ms); $fr = [double[]]@($it.anim_frames)
    $fps = [double[]]@($it | ForEach-Object { $_.anim_frames / ($_.anim_ms / 1000.0) })
    [void]$md.AppendLine("| $c | $($it.Count) | $(Fmt (Median $lat) 1) / $(Fmt (P95 $lat) 1) | $(Fmt (Median $fr) 0) | $(Fmt (Median $fps) 1) |")
    $summary["interaction_$c"] = @{ latency_median_ms = (Median $lat); latency_p95_ms = (P95 $lat); frames_median = (Median $fr); fps_median = (Median $fps) }
}
[void]$md.AppendLine('')

# ---- idle
[void]$md.AppendLine('## Idle (per repetition: 10-minute window after 5-minute warm-up; table shows median across repetitions with min–max)')
[void]$md.AppendLine('| Config | Reps | CPU (% of one core) | CPU (% of all 24 threads) | Working set (MiB) | Private bytes (MiB) | GPU util (% median of samples) | GPU util max (%) | Dedicated GPU mem (MiB) | Shared GPU mem (MiB) | Redraws in window | Redraws/s | System CPU during run (%) |')
[void]$md.AppendLine('|---|---|---|---|---|---|---|---|---|---|---|---|---|')
foreach ($c in $configs) {
    $reps = @(Get-ChildItem $RunDir -Filter "idle-$c-rep*-cpu.json")
    $cpu1 = @(); $cpuAll = @(); $ws = @(); $pb = @(); $gu = @(); $gmax = @(); $gd = @(); $gs = @(); $rd = @(); $rps = @(); $sys = @()
    foreach ($f in $reps) {
        $cj = Get-Content $f.FullName -Raw | ConvertFrom-Json
        $tag = $f.Name -replace '-cpu\.json$', ''
        $csv = Import-Csv (Join-Path $RunDir "$tag.csv")
        $pct = 100.0 * $cj.cpu_seconds / $cj.window_seconds
        $cpu1 += $pct; $cpuAll += $pct / $LogicalCpus
        $ws += (Median ([double[]]@($csv.working_set))) / $MiB
        $pb += (Median ([double[]]@($csv.private_bytes))) / $MiB
        $gu += (Median ([double[]]@($csv.gpu_util_pct)))
        $gmax += ([double[]]@($csv.gpu_util_pct) | Measure-Object -Maximum).Maximum
        $gd += (Median ([double[]]@($csv.gpu_dedicated))) / $MiB
        $gs += (Median ([double[]]@($csv.gpu_shared))) / $MiB
        $sys += (Median ([double[]]@($csv.system_cpu_pct)))
        $sj = Get-Content (Join-Path $RunDir "$tag.json") -Raw | ConvertFrom-Json
        if ($sj.frames_total -gt 0 -or $c -ne 'slint-software') {
            $n = @($sj.frame_times_s | Where-Object { $_ -ge $cj.window_start_s -and $_ -le $cj.window_end_s }).Count
            $rd += $n; $rps += $n / $cj.window_seconds
        }
    }
    $redraw = if ($rd.Count) { Range ([double[]]$rd) 0 } else { 'n/a (not reported)' }
    $redrawS = if ($rps.Count) { Range ([double[]]$rps) 4 } else { 'n/a' }
    [void]$md.AppendLine("| $c | $($reps.Count) | $(Range $cpu1 4) | $(Range $cpuAll 5) | $(Range $ws 1) | $(Range $pb 1) | $(Range $gu 3) | $(Range $gmax 2) | $(Range $gd 1) | $(Range $gs 1) | $redraw | $redrawS | $(Range $sys 1) |")
    $summary["idle_$c"] = @{
        reps = $reps.Count; cpu_pct_one_core = (Median $cpu1); cpu_pct_total = (Median $cpuAll)
        working_set_mib = (Median $ws); private_mib = (Median $pb); gpu_util_median = (Median $gu); gpu_util_max = (Median $gmax)
        gpu_dedicated_mib = (Median $gd); gpu_shared_mib = (Median $gs); redraws = (Median $rd); system_cpu_pct = (Median $sys)
    }
}
$md.ToString() | Set-Content (Join-Path $RunDir 'summary.md') -Encoding utf8
$summary | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $RunDir 'summary.json') -Encoding utf8
$md.ToString()
