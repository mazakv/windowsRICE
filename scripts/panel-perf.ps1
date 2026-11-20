# windowsRICE panel performance measurement (real Windows).
#
#   pwsh -File scripts/panel-perf.ps1 -Mode Quick   # ~2 min, regression signal
#   pwsh -File scripts/panel-perf.ps1 -Mode Full    # ~30 min, milestone evidence
#
# Quick results go to target/perf-quick/<run> (not committed). Full results
# go to docs/benchmarks/<date>-panel/ with the environment description.
#
# Measured per configuration and repetition (median across repetitions):
# CPU (process time delta over the window), working set and private bytes,
# GPU engine utilisation and dedicated/shared GPU memory of the process
# (performance counters), redraws (frames the panel reports). Also startup
# (process creation to first presented frame), input latency (key message
# received to frame presented, keys posted to the panel's own window only)
# and the cost of serving a UI Automation client.
#
# The active-window title is fixed (--title) so that redraws do not depend on
# whatever window the user has in front. Nothing here changes system settings.

param(
    [ValidateSet('Quick', 'Full')][string]$Mode = 'Quick',
    [string]$ProjectRoot = (Split-Path $PSScriptRoot -Parent),
    [string]$Exe = 'target\release\windowsrice-panel.exe',
    # Folder suffix of full results: docs/benchmarks/<date>-<Topic>.
    [string]$Topic = 'bar'
)
$ErrorActionPreference = 'Stop'
Set-Location $ProjectRoot
$runId = Get-Date -Format 'yyyyMMdd-HHmmss'
$out = if ($Mode -eq 'Full') { Join-Path $ProjectRoot "docs\benchmarks\$(Get-Date -Format 'yyyy-MM-dd')-$Topic" } else { Join-Path $ProjectRoot "target\perf-quick\$runId" }
New-Item -ItemType Directory -Force $out | Out-Null
$Settings = if ($Mode -eq 'Full') { @{ Warm = 60; Window = 120; Reps = 3; Launches = 10; Keys = 40; Configs = 'default', 'no-gpu-counter', 'uia-client' } } else { @{ Warm = 10; Window = 30; Reps = 1; Launches = 3; Keys = 10; Configs = 'default' } }
$SampleSec = 5
Add-Type -Namespace WRP -Name Win -MemberDefinition @'
[DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr hWnd, uint msg, IntPtr wParam, IntPtr lParam);
[DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindowW(string cls, string title);
'@
function Log($m) { $l = "[{0}] {1}" -f (Get-Date -Format 'HH:mm:ss'), $m; Write-Host $l; Add-Content (Join-Path $out 'log.txt') $l }
function Median([double[]]$v) { $v = @($v | Sort-Object); if (-not $v.Count) { return $null }; $m = [int][math]::Floor($v.Count / 2); if ($v.Count % 2) { $v[$m] } else { ($v[$m - 1] + $v[$m]) / 2 } }
function P95([double[]]$v) { $v = @($v | Sort-Object); if (-not $v.Count) { return $null }; $v[[int][math]::Min($v.Count - 1, [math]::Ceiling(0.95 * $v.Count) - 1)] }
function Panel([string[]]$extra) {
    # Isolated configuration (defaults; never written because nothing is changed).
    Start-Process $Exe -ArgumentList (@('--no-hotkey', '--lang', 'en', '--config', (Join-Path $ProjectRoot 'target\perf-config.toml'), '--title', '"Documents - File Explorer"') + $extra) -PassThru
}
function Wait-Window { for ($i = 0; $i -lt 50; $i++) { $h = [WRP.Win]::FindWindowW('windowsRICE.Panel', [NullString]::Value); if ($h -ne [IntPtr]::Zero) { return $h }; Start-Sleep -Milliseconds 100 }; throw 'no panel window' }

$gpu = Get-CimInstance Win32_VideoController | Select-Object -First 1
$os = Get-CimInstance Win32_OperatingSystem
$ubr = (Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion').UBR
[ordered]@{
    run_id = $runId; mode = $Mode; params = $Settings
    windows = "$($os.Version).$ubr"; cpu = (Get-CimInstance Win32_Processor | Select-Object -First 1).Name
    gpu = $gpu.Name; gpu_driver = $gpu.DriverVersion; power_plan = (powercfg /getactivescheme)
    note = 'Windhawk and the AMD driver inject DLLs into processes on this machine; other user applications were running.'
} | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $out 'environment.json') -Encoding utf8

# ---- startup
$startup = @()
for ($i = 1; $i -le $Settings.Launches; $i++) {
    $s = Join-Path $out "startup-$i.json"
    $p = Panel @('--exit-after', '2.5', '--stats', $s)
    $null = $p.WaitForExit(20000); Start-Sleep -Milliseconds 500
    $j = Get-Content $s -Raw | ConvertFrom-Json
    $startup += [pscustomobject]@{ first_frame_ms = $j.ms_process_to_first_frame; window_ms = $j.ms_process_to_window; render_init_ms = $j.render.init_ms; exit = $p.ExitCode }
}
Log "startup done"

# ---- input latency: mouse moves posted to the panel's own window, alternating
# over the menu button (hover highlight on) and empty surface (off). Keyboard
# input needs keyboard focus, which a background script cannot give the panel.
$s = Join-Path $out 'input.json'
$p = Panel @('--exit-after', ([string]($Settings.Keys * 0.4 + 6)), '--stats', $s)
$h = Wait-Window; Start-Sleep 3
for ($k = 0; $k -lt $Settings.Keys; $k++) {
    $x = if ($k % 2) { 600 } else { 37 }
    [void][WRP.Win]::PostMessageW($h, 0x0200, [IntPtr]::Zero, [IntPtr]($x -bor (25 -shl 16)))
    Start-Sleep -Milliseconds 400
}
$null = $p.WaitForExit(60000)
$input = Get-Content $s -Raw | ConvertFrom-Json
Log "input done: $(@($input.input_latency_ms).Count) samples"

# ---- UIA client helper (Windows PowerShell 5.1): reads the whole tree every 5 s
$uiaScript = Join-Path $out 'uia-client.ps1'
@'
param([string]$Log, [int]$Seconds)
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
Add-Type -Namespace WRU -Name Win -MemberDefinition '[DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindowW(string cls, string title);'
$end = (Get-Date).AddSeconds($Seconds)
while ((Get-Date) -lt $end) {
    $h = [WRU.Win]::FindWindowW('windowsRICE.Panel', [NullString]::Value)
    if ($h -ne [IntPtr]::Zero) {
        $sw = [Diagnostics.Stopwatch]::StartNew()
        $r = [System.Windows.Automation.AutomationElement]::FromHandle($h)
        $all = $r.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
        $names = foreach ($e in $all) { $e.Current.Name; $e.Current.BoundingRectangle }
        $sw.Stop(); Add-Content $Log ("{0} {1}" -f $all.Count, $sw.Elapsed.TotalMilliseconds)
    }
    Start-Sleep -Seconds 5
}
'@ | Set-Content $uiaScript -Encoding utf8

# ---- idle
$idle = @()
foreach ($rep in 1..$Settings.Reps) {
    foreach ($cfg in $Settings.Configs) {
        $tag = "idle-$cfg-rep$rep"; $s = Join-Path $out "$tag.json"
        $extra = @('--exit-after', [string]($Settings.Warm + $Settings.Window + 8), '--stats', $s)
        if ($cfg -eq 'no-gpu-counter') { $extra += '--no-gpu-counter' }
        $p = Panel $extra
        $null = Wait-Window
        $client = $null
        if ($cfg -eq 'uia-client') {
            $clientLog = Join-Path $out "$tag-uia.txt"
            $client = Start-Process powershell.exe -ArgumentList @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $uiaScript, '-Log', $clientLog, '-Seconds', [string]($Settings.Warm + $Settings.Window)) -PassThru -WindowStyle Hidden
        }
        Start-Sleep -Seconds $Settings.Warm
        $g = Get-Process -Id $p.Id; $cpu0 = $g.TotalProcessorTime.TotalSeconds; $t0 = Get-Date
        $rows = @()
        while (((Get-Date) - $t0).TotalSeconds -lt $Settings.Window) {
            $g.Refresh()
            $cs = (Get-Counter -Counter @("\GPU Engine(pid_$($p.Id)_*)\Utilization Percentage", "\GPU Process Memory(pid_$($p.Id)_*)\Dedicated Usage", "\GPU Process Memory(pid_$($p.Id)_*)\Shared Usage") -ErrorAction SilentlyContinue).CounterSamples
            $rows += [pscustomobject]@{
                t = [math]::Round(((Get-Date) - $t0).TotalSeconds, 1); ws = $g.WorkingSet64; priv = $g.PrivateMemorySize64; handles = $g.HandleCount
                gpu_util = ($cs | Where-Object Path -like '*gpu engine*' | Measure-Object CookedValue -Sum).Sum
                gpu_ded = ($cs | Where-Object Path -like '*dedicated usage' | Measure-Object CookedValue -Sum).Sum
                gpu_sh = ($cs | Where-Object Path -like '*shared usage' | Measure-Object CookedValue -Sum).Sum
            }
            Start-Sleep -Seconds $SampleSec
        }
        $g.Refresh(); $cpu1 = $g.TotalProcessorTime.TotalSeconds; $win = ((Get-Date) - $t0).TotalSeconds
        $rows | Export-Csv (Join-Path $out "$tag.csv") -NoTypeInformation
        $null = $p.WaitForExit(60000)
        if ($client) { $null = $client.WaitForExit(30000) }
        $j = Get-Content $s -Raw | ConvertFrom-Json
        $ws0 = $Settings.Warm; $frames = @($j.frames | Where-Object { $_ -ge $ws0 -and $_ -le ($ws0 + $win) }).Count
        $uiaMs = if ($cfg -eq 'uia-client' -and (Test-Path $clientLog)) { Median ([double[]]@(Get-Content $clientLog | ForEach-Object { ($_ -split ' ')[1] })) } else { $null }
        $idle += [pscustomobject]@{
            config = $cfg; rep = $rep; cpu_pct_core = 100 * ($cpu1 - $cpu0) / $win
            ws_mib = (Median ([double[]]$rows.ws)) / 1MB; priv_mib = (Median ([double[]]$rows.priv)) / 1MB; handles = Median ([double[]]$rows.handles)
            gpu_util = Median ([double[]]$rows.gpu_util); gpu_util_max = ($rows.gpu_util | Measure-Object -Maximum).Maximum
            gpu_ded_mib = (Median ([double[]]$rows.gpu_ded)) / 1MB; gpu_sh_mib = (Median ([double[]]$rows.gpu_sh)) / 1MB
            redraws = $frames; redraws_per_min = 60 * $frames / $win; uia_calls = $j.uia_provider_calls; uia_fetch_ms = $uiaMs
            sample_cost_ms = Median ([double[]]$j.sample_cost_ms)
        }
        Log "$tag done cpu=$([math]::Round($idle[-1].cpu_pct_core, 4))% priv=$([math]::Round($idle[-1].priv_mib, 1))"
    }
}

# ---- summary
$md = [System.Text.StringBuilder]::new()
$envj = Get-Content (Join-Path $out 'environment.json') -Raw | ConvertFrom-Json
[void]$md.AppendLine("# windowsRICE panel performance ($Mode, $runId)")
[void]$md.AppendLine("Windows $($envj.windows); $($envj.cpu); $($envj.gpu) driver $($envj.gpu_driver). Warm-up $($Settings.Warm) s, window $($Settings.Window) s, $($Settings.Reps) repetition(s), sampled every $SampleSec s. Values are medians with min–max across repetitions.")
[void]$md.AppendLine('')
function Fmt-Range($v, [int]$d) { $v = @($v | Where-Object { $null -ne $_ }); if (-not $v.Count) { return 'n/a' }; "{0} ({1}–{2})" -f [math]::Round((Median ([double[]]$v)), $d), [math]::Round(($v | Measure-Object -Minimum).Minimum, $d), [math]::Round(($v | Measure-Object -Maximum).Maximum, $d) }
[void]$md.AppendLine('| Startup | Value |'); [void]$md.AppendLine('|---|---|')
[void]$md.AppendLine("| Process creation → first presented frame (ms), $($startup.Count) launches | $(Fmt-Range $startup.first_frame_ms 1) |")
[void]$md.AppendLine("| Device + swap chain + composition creation (ms) | $(Fmt-Range $startup.render_init_ms 1) |")
$lat = [double[]]@($input.input_latency_ms)
[void]$md.AppendLine("| Input: mouse-move message → frame presented (ms), $($lat.Count) events | median $([math]::Round((Median $lat), 2)), p95 $([math]::Round((P95 $lat), 2)), max $([math]::Round(($lat | Measure-Object -Maximum).Maximum, 2)) |")
[void]$md.AppendLine("| Frame draw time (ms, all frames of the input run) | median $([math]::Round((Median ([double[]]$input.frame_draw_ms)), 2)), p95 $([math]::Round((P95 ([double[]]$input.frame_draw_ms)), 2)) |")
[void]$md.AppendLine("| Present call (ms) | median $([math]::Round((Median ([double[]]$input.frame_present_ms)), 2)), p95 $([math]::Round((P95 ([double[]]$input.frame_present_ms)), 2)) |")
[void]$md.AppendLine('')
[void]$md.AppendLine('| Idle config | Reps | CPU (% of one core) | Working set (MiB) | Private (MiB) | Handles | GPU util (%) | GPU util max (%) | Dedicated GPU (MiB) | Shared GPU (MiB) | Redraws/min | Sampler cost (ms) | UIA tree fetch (ms) |')
[void]$md.AppendLine('|---|---|---|---|---|---|---|---|---|---|---|---|---|')
foreach ($cfg in $Settings.Configs) {
    $r = @($idle | Where-Object config -eq $cfg)
    [void]$md.AppendLine("| $cfg | $($r.Count) | $(Fmt-Range $r.cpu_pct_core 3) | $(Fmt-Range $r.ws_mib 1) | $(Fmt-Range $r.priv_mib 1) | $(Fmt-Range $r.handles 0) | $(Fmt-Range $r.gpu_util 3) | $(Fmt-Range $r.gpu_util_max 2) | $(Fmt-Range $r.gpu_ded_mib 1) | $(Fmt-Range $r.gpu_sh_mib 1) | $(Fmt-Range $r.redraws_per_min 1) | $(Fmt-Range $r.sample_cost_ms 1) | $(Fmt-Range $r.uia_fetch_ms 1) |")
}
$md.ToString() | Set-Content (Join-Path $out 'summary.md') -Encoding utf8
@{ startup = $startup; idle = $idle; input_latency_ms = $lat } | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $out 'summary.json') -Encoding utf8
$md.ToString()
Log "results: $out"
