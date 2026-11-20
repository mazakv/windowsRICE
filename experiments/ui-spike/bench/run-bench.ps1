# Phase 3.5 UI spike benchmark (EXPERIMENTAL). PowerShell 7.
#
# A. Startup:     10 launches per configuration. In-app: process creation →
#                 first frame. External: Start-Process → window visible.
# B. Interaction: 60 s with --interaction-test (selection changes every 2 s):
#                 state-change → frame latency and frames per 180 ms animation.
# C. Idle:        per repetition and configuration (interleaved): warm-up,
#                 then a measurement window sampled every $SampleSec seconds:
#                 CPU time (TotalProcessorTime delta), working set, private
#                 bytes, GPU engine utilization (sum over engines) and GPU
#                 dedicated/shared memory for the PID, plus total system CPU.
#
# Writes only under results\bench\<run-id>. Changes no system settings.

param(
    [string]$Root = (Split-Path $PSScriptRoot -Parent),
    [int]$Reps = 3,
    [int]$WarmSec = 300,
    [int]$MeasureSec = 600,
    [int]$SampleSec = 5,
    [int]$StartupLaunches = 10,
    [int]$InteractionSec = 60,
    [string[]]$Phases = @('A', 'B', 'C')
)

$ErrorActionPreference = 'Stop'
Set-Location $Root
$env:TMP = Join-Path $Root 'tmp'; $env:TEMP = $env:TMP
$runId = Get-Date -Format 'yyyyMMdd-HHmmss'
$out = Join-Path $Root "results\bench\$runId"
New-Item -ItemType Directory -Force $out | Out-Null

Add-Type -Namespace Bench -Name Win -MemberDefinition @'
public delegate bool EnumProc(System.IntPtr h, System.IntPtr l);
[System.Runtime.InteropServices.DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, System.IntPtr l);
[System.Runtime.InteropServices.DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(System.IntPtr h, out uint pid);
[System.Runtime.InteropServices.DllImport("user32.dll")] public static extern bool IsWindowVisible(System.IntPtr h);
public static bool HasVisibleWindow(int pid) {
    bool found = false;
    EnumWindows((h, l) => { uint p; GetWindowThreadProcessId(h, out p); if (p == (uint)pid && IsWindowVisible(h)) { found = true; return false; } return true; }, System.IntPtr.Zero);
    return found;
}
'@

$configs = @(
    @{ tag = 'slint-femtovg'; exe = 'slint-bar.exe'; extra = @() },
    @{ tag = 'slint-software'; exe = 'slint-bar.exe'; extra = @('--renderer', 'software') },
    @{ tag = 'd2d'; exe = 'd2d-bar.exe'; extra = @() }
)

function Log($m) { $line = "[{0}] {1}" -f (Get-Date -Format 'HH:mm:ss'), $m; Write-Host $line; Add-Content (Join-Path $out 'log.txt') $line }

# ---- environment record
$os = Get-CimInstance Win32_OperatingSystem
$gpu = Get-CimInstance Win32_VideoController | Select-Object -First 1
$cv = Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion'
@{
    run_id = $runId
    windows = "$($os.Version).$($cv.UBR) ($($cv.DisplayVersion))"
    cpu = (Get-CimInstance Win32_Processor).Name.Trim()
    gpu = $gpu.Name; gpu_driver = $gpu.DriverVersion
    power_plan = (powercfg /getactivescheme) -join ' '
    params = @{ Reps = $Reps; WarmSec = $WarmSec; MeasureSec = $MeasureSec; SampleSec = $SampleSec; StartupLaunches = $StartupLaunches; InteractionSec = $InteractionSec }
} | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $out 'environment.json') -Encoding utf8

function Start-Candidate($c, [string[]]$args2) {
    Start-Process -FilePath (Join-Path $Root "target\release\$($c.exe)") -ArgumentList ($args2 + $c.extra) -PassThru
}

# ---- A. startup
if ($Phases -contains 'A') {
    $rows = @()
    for ($i = 1; $i -le $StartupLaunches; $i++) {
        foreach ($c in $configs) {
            $stats = Join-Path $out "startup-$($c.tag)-$i.json"
            $sw = [System.Diagnostics.Stopwatch]::StartNew()
            $p = Start-Candidate $c @('--exit-after', '3', '--stats', $stats)
            $visibleMs = $null
            while ($sw.ElapsedMilliseconds -lt 10000) {
                if ([Bench.Win]::HasVisibleWindow($p.Id)) { $visibleMs = $sw.Elapsed.TotalMilliseconds; break }
                Start-Sleep -Milliseconds 2
            }
            $null = $p.WaitForExit(15000)
            $j = Get-Content $stats -Raw | ConvertFrom-Json
            $rows += [pscustomobject]@{ config = $c.tag; launch = $i; window_visible_ms = $visibleMs; first_frame_ms = $j.first_frame_ms_since_process_creation; exit = $p.ExitCode }
            Start-Sleep -Milliseconds 500
        }
    }
    $rows | Export-Csv (Join-Path $out 'startup.csv') -NoTypeInformation
    Log "startup done ($($rows.Count) launches)"
}

# ---- B. interaction
if ($Phases -contains 'B') {
    foreach ($c in $configs) {
        $stats = Join-Path $out "interaction-$($c.tag).json"
        $p = Start-Candidate $c @('--exit-after', "$InteractionSec", '--interaction-test', '--stats', $stats)
        $null = $p.WaitForExit(($InteractionSec + 30) * 1000)
        Log "interaction $($c.tag) exit=$($p.ExitCode)"
    }
}

# ---- C. idle
if ($Phases -contains 'C') {
    for ($rep = 1; $rep -le $Reps; $rep++) {
        foreach ($c in $configs) {
            $tag = "$($c.tag)-rep$rep"
            $stats = Join-Path $out "idle-$tag.json"
            $exitAfter = $WarmSec + $MeasureSec + 5
            $p = Start-Candidate $c @('--exit-after', "$exitAfter", '--stats', $stats)
            Log "idle $tag pid=$($p.Id) warm-up ${WarmSec}s"
            Start-Sleep -Seconds $WarmSec
            $gp = Get-Process -Id $p.Id
            $cpu0 = $gp.TotalProcessorTime.TotalSeconds; $t0 = Get-Date
            $samples = @()
            $counters = @(
                "\GPU Engine(pid_$($p.Id)_*)\Utilization Percentage",
                "\GPU Process Memory(pid_$($p.Id)_*)\Dedicated Usage",
                "\GPU Process Memory(pid_$($p.Id)_*)\Shared Usage",
                '\Processor(_Total)\% Processor Time'
            )
            while (((Get-Date) - $t0).TotalSeconds -lt $MeasureSec) {
                $gp.Refresh()
                $cs = (Get-Counter -Counter $counters -ErrorAction SilentlyContinue).CounterSamples
                $samples += [pscustomobject]@{
                    t = [math]::Round(((Get-Date) - $t0).TotalSeconds, 1)
                    working_set = $gp.WorkingSet64
                    private_bytes = $gp.PrivateMemorySize64
                    gpu_util_pct = [math]::Round((($cs | Where-Object Path -like '*gpu engine*' | Measure-Object CookedValue -Sum).Sum), 4)
                    gpu_dedicated = ($cs | Where-Object Path -like '*dedicated usage' | Measure-Object CookedValue -Sum).Sum
                    gpu_shared = ($cs | Where-Object Path -like '*shared usage' | Measure-Object CookedValue -Sum).Sum
                    system_cpu_pct = [math]::Round(($cs | Where-Object Path -like '*processor(_total)*').CookedValue, 2)
                }
                Start-Sleep -Seconds $SampleSec
            }
            $gp.Refresh()
            $cpu1 = $gp.TotalProcessorTime.TotalSeconds; $elapsed = ((Get-Date) - $t0).TotalSeconds
            $samples | Export-Csv (Join-Path $out "idle-$tag.csv") -NoTypeInformation
            @{ config = $c.tag; rep = $rep; pid = $p.Id; cpu_seconds = $cpu1 - $cpu0; window_seconds = $elapsed;
               window_start_s = $WarmSec; window_end_s = $WarmSec + $elapsed } |
                ConvertTo-Json | Set-Content (Join-Path $out "idle-$tag-cpu.json") -Encoding utf8
            $null = $p.WaitForExit(60000)
            Log "idle $tag done cpu_s=$([math]::Round($cpu1 - $cpu0, 3)) exit=$($p.ExitCode)"
            Start-Sleep -Seconds 5
        }
    }
}
Log "benchmark complete: $out"
