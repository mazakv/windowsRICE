# Short performance measurement of the release bar (Phase 9): startup,
# 60 s idle (CPU, memory, GPU memory, redraws, frame cost) and the Design
# Studio's memory while open and after closing. Isolated configuration, no
# screenshots. Results: target\perf-quick\results.json. Not a benchmark:
# one machine, one run per measurement (startup: three runs).
param([string]$ProjectRoot = (Split-Path $PSScriptRoot -Parent), [int]$IdleSeconds = 60)
$ErrorActionPreference = 'Stop'
Set-Location $ProjectRoot
if (@(Get-Process windowsrice-panel -ErrorAction SilentlyContinue).Count -gt 0) { throw 'a windowsRICE bar is running; close it first' }
$Out = Join-Path $ProjectRoot 'target\perf-quick'
if (Test-Path $Out) { Remove-Item -Recurse -Force $Out }
New-Item -ItemType Directory -Force $Out | Out-Null
$exe = 'target\release\windowsrice-panel.exe'
$cfg = Join-Path $Out 'config.toml'
Add-Type -Namespace WPQ -Name W -MemberDefinition '[DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindowW(string c, string t); [DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr h, uint m, IntPtr w, IntPtr l);'
function Median($xs) { $s = @($xs | Sort-Object); if ($s.Count -eq 0) { return $null }; $s[[int][math]::Floor($s.Count / 2)] }
function Gpu-Mib([int]$procId) {
    try {
        $c = Get-Counter "\GPU Process Memory(pid_${procId}_*)\Dedicated Usage" -ErrorAction Stop
        [math]::Round((($c.CounterSamples | Measure-Object CookedValue -Sum).Sum) / 1MB, 1)
    } catch { $null }
}
function Mem($p) { $p.Refresh(); [pscustomobject]@{ private_mib = [math]::Round($p.PrivateMemorySize64 / 1MB, 1); working_set_mib = [math]::Round($p.WorkingSet64 / 1MB, 1); gpu_dedicated_mib = (Gpu-Mib $p.Id); handles = $p.HandleCount } }
function Start-Bar([string]$tag, [int]$secs, [string[]]$extra = @()) {
    Start-Process $exe -ArgumentList (@('--config', $cfg, '--stats', (Join-Path $Out "$tag.json"), '--no-hotkey', '--exit-after', "$secs", '--title', '"Perf test"') + $extra) -PassThru
}

# ---- startup (three runs)
$startup = foreach ($i in 1..3) {
    $p = Start-Bar "startup$i" 4; $null = $p.WaitForExit(20000)
    $j = Get-Content (Join-Path $Out "startup$i.json") -Raw | ConvertFrom-Json
    [pscustomobject]@{ to_window_ms = $j.ms_process_to_window; to_first_frame_ms = $j.ms_process_to_first_frame }
}

# ---- idle
$p = Start-Bar 'idle' ($IdleSeconds + 25)
Start-Sleep -Seconds 15
$p.Refresh(); $cpu0 = $p.TotalProcessorTime.TotalMilliseconds; $t0 = Get-Date
Start-Sleep -Seconds $IdleSeconds
$p.Refresh(); $cpu1 = $p.TotalProcessorTime.TotalMilliseconds; $elapsed = ((Get-Date) - $t0).TotalMilliseconds
$idleMem = Mem $p
$null = $p.WaitForExit(60000)
$j = Get-Content (Join-Path $Out 'idle.json') -Raw | ConvertFrom-Json
$inWindow = @($j.frames | Where-Object { $_ -ge 15 -and $_ -le (15 + $IdleSeconds) })
$idle = [pscustomobject]@{
    cpu_percent_of_one_core = [math]::Round(100 * ($cpu1 - $cpu0) / $elapsed, 3)
    cpu_percent_of_machine = [math]::Round(100 * ($cpu1 - $cpu0) / $elapsed / [Environment]::ProcessorCount, 4)
    redraws_per_minute = [math]::Round($inWindow.Count * 60 / $IdleSeconds, 1)
    frame_draw_ms_median = Median $j.frame_draw_ms
    frame_present_ms_median = Median $j.frame_present_ms
    memory = $idleMem
}

# ---- Design Studio: open, use, close
$p = Start-Bar 'studio' 120
Start-Sleep -Seconds 8
$before = Mem $p
Start-Process $exe -ArgumentList @('--studio', '--config', $cfg) -Wait
Start-Sleep -Seconds 6
$open = Mem $p
$sh = [WPQ.W]::FindWindowW('windowsRICE.Studio', [NullString]::Value)
# Scroll the font gallery a little (wheel messages to the Studio window).
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
$A = [System.Windows.Automation.AutomationElement]
$nav = $A::FromHandle($sh).FindFirst([System.Windows.Automation.TreeScope]::Descendants, (New-Object System.Windows.Automation.PropertyCondition($A::AutomationIdProperty, 'nav:typography')))
$nav.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Select(); Start-Sleep -Milliseconds 800
for ($k = 0; $k -lt 20; $k++) { [void][WPQ.W]::PostMessageW($sh, 0x020A, [IntPtr]([int](-120 -band 0xFFFF) -shl 16), [IntPtr]((800 -shl 16) -bor 1500)); Start-Sleep -Milliseconds 120 }
Start-Sleep -Seconds 2
$used = Mem $p
[void][WPQ.W]::PostMessageW($sh, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)
Start-Sleep -Seconds 6
$closed = Mem $p
# Second cycle: memory that keeps growing would be a leak; a level value is
# a cache (DirectWrite font faces, interned family names).
Start-Process $exe -ArgumentList @('--studio', '--config', $cfg) -Wait
Start-Sleep -Seconds 5
$sh = [WPQ.W]::FindWindowW('windowsRICE.Studio', [NullString]::Value)
$nav = $A::FromHandle($sh).FindFirst([System.Windows.Automation.TreeScope]::Descendants, (New-Object System.Windows.Automation.PropertyCondition($A::AutomationIdProperty, 'nav:typography')))
$nav.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Select(); Start-Sleep -Milliseconds 800
for ($k = 0; $k -lt 20; $k++) { [void][WPQ.W]::PostMessageW($sh, 0x020A, [IntPtr]([int](-120 -band 0xFFFF) -shl 16), [IntPtr]((800 -shl 16) -bor 1500)); Start-Sleep -Milliseconds 120 }
Start-Sleep -Seconds 2
[void][WPQ.W]::PostMessageW($sh, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)
Start-Sleep -Seconds 6
$closed2 = Mem $p
$bar = [WPQ.W]::FindWindowW('windowsRICE.Panel', [NullString]::Value)
[void][WPQ.W]::PostMessageW($bar, 0x0104, [IntPtr]0x73, [IntPtr]([long]1 -bor (0x3E -shl 16) -bor (1L -shl 29)))
$null = $p.WaitForExit(20000)
$j = Get-Content (Join-Path $Out 'studio.json') -Raw | ConvertFrom-Json
$builds = @($j.studio_build_us)
$fontLine = @($j.popup_log | Where-Object { $_ -match 'studio fonts' }) -join '; '
$studio = [pscustomobject]@{
    before = $before; open = $open; after_scrolling_fonts = $used; after_close = $closed; after_second_close = $closed2
    scene_builds = $builds.Count; build_ms_median = [math]::Round((Median $builds) / 1000, 2); build_ms_max = [math]::Round((($builds | Measure-Object -Maximum).Maximum) / 1000, 2)
    font_discovery = $fontLine; errors = @($j.errors).Count; exit_reason = $j.exit_reason
}

$result = [pscustomobject]@{
    machine = "$((Get-CimInstance Win32_Processor).Name.Trim()), $([Environment]::ProcessorCount) logical CPUs"
    startup = $startup; startup_first_frame_ms_median = Median ($startup.to_first_frame_ms)
    idle = $idle; studio = $studio
}
$result | ConvertTo-Json -Depth 5 | Set-Content (Join-Path $Out 'results.json') -Encoding utf8
$result | ConvertTo-Json -Depth 5
