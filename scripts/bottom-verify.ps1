# Phase 14: the bottom bar on the running application (isolated
# configuration; no screenshots). Checks that the top and bottom bars both
# appear, that the bottom bar reserves space through the Windows AppBar
# interface (above the Windows taskbar, never over it), that its panels open
# above it, and that exiting releases the reservation. Results:
# target\phase14\bottom\checks.json.
param(
    [string]$ProjectRoot = (Split-Path $PSScriptRoot -Parent),
    [string]$Exe = 'target\release\windowsrice-panel.exe'
)
$ErrorActionPreference = 'Stop'
Set-Location $ProjectRoot
if (@(Get-Process windowsrice-panel -ErrorAction SilentlyContinue).Count -gt 0) { throw 'a windowsRICE bar is running; close it first' }
$Out = Join-Path $ProjectRoot 'target\phase14\bottom'
if (Test-Path $Out) { Remove-Item -Recurse -Force $Out }
New-Item -ItemType Directory -Force $Out | Out-Null
$cfg = Join-Path $Out 'config.toml'
@'
schema_version = 3

[bottom_bar]
height = 36
'@ | Set-Content $cfg -Encoding utf8
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
$A = [System.Windows.Automation.AutomationElement]; $TS = [System.Windows.Automation.TreeScope]
Add-Type -Namespace BV -Name W -MemberDefinition @'
[DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindowExW(IntPtr parent, IntPtr after, string cls, string title);
[DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindowW(string c, string t);
[DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
[DllImport("user32.dll")] public static extern bool SetProcessDpiAwarenessContext(IntPtr v);
[DllImport("user32.dll")] public static extern IntPtr MonitorFromPoint(POINT pt, uint flags);
[DllImport("user32.dll")] public static extern bool GetMonitorInfoW(IntPtr h, ref MONITORINFO mi);
[DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr h, uint m, IntPtr w, IntPtr l);
[DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern uint RegisterWindowMessageW(string name);
[StructLayout(LayoutKind.Sequential)] public struct POINT { public int X; public int Y; }
[StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
[StructLayout(LayoutKind.Sequential)] public struct MONITORINFO { public int cbSize; public RECT rcMonitor; public RECT rcWork; public uint dwFlags; }
public static MONITORINFO Info() { var mi = new MONITORINFO(); mi.cbSize = Marshal.SizeOf(mi); GetMonitorInfoW(MonitorFromPoint(new POINT(), 1), ref mi); return mi; }
public static RECT Rect(IntPtr h) { RECT r; GetWindowRect(h, out r); return r; }
'@
[void][BV.W]::SetProcessDpiAwarenessContext([IntPtr](-4))
$checks = [System.Collections.Generic.List[object]]::new()
function Check([string]$what, [bool]$ok, [string]$detail = '') { $checks.Add([pscustomobject]@{ check = $what; pass = $ok; detail = $detail }); '{0,-62} {1} {2}' -f $what, $ok, $detail }
function Bars { $l = @(); $h = [IntPtr]::Zero; while ($true) { $h = [BV.W]::FindWindowExW([IntPtr]::Zero, $h, 'windowsRICE.Panel', [NullString]::Value); if ($h -eq [IntPtr]::Zero) { break }; $l += $h }; $l }

$before = [BV.W]::Info()
$p = Start-Process $Exe -ArgumentList @('--config', $cfg, '--no-hotkey', '--lang', 'en', '--exit-after', '60', '--title', '"Bottom bar test"', '--monitor', '0') -PassThru
for ($i = 0; $i -lt 60 -and @(Bars).Count -lt 2; $i++) { Start-Sleep -Milliseconds 200 }
Start-Sleep -Seconds 2
$bars = @(Bars)
$mon = $before.rcMonitor
$rects = $bars | ForEach-Object { [BV.W]::Rect($_) }
$top = $rects | Where-Object { $_.Top -eq $mon.Top }
$bottom = $rects | Where-Object { $_.Top -gt ($mon.Top + ($mon.Bottom - $mon.Top) / 2) }
Check 'a top bar and a bottom bar are shown' (($bars.Count -eq 2) -and $top -and $bottom) (($rects | ForEach-Object { "$($_.Left),$($_.Top),$($_.Right),$($_.Bottom)" }) -join ' | ')
$after = [BV.W]::Info()
Check 'the bottom bar spans the monitor width' ($bottom -and ($bottom.Left -eq $mon.Left) -and ($bottom.Right -eq $mon.Right))
Check 'the bottom bar does not cover the Windows taskbar' ($bottom -and ($bottom.Bottom -le $before.rcWork.Bottom)) "bar bottom $($bottom.Bottom), work bottom before $($before.rcWork.Bottom)"
Check 'it reserves space: the work area ends above it' ($bottom -and ($after.rcWork.Bottom -le $bottom.Top)) "work bottom $($after.rcWork.Bottom), bar top $($bottom.Top)"
Check 'the top bar still reserves the top' ($after.rcWork.Top -ge $top.Bottom) "work top $($after.rcWork.Top)"
# A panel from the bottom bar opens above it.
$bh = $bars | Where-Object { ([BV.W]::Rect($_)).Top -eq $bottom.Top } | Select-Object -First 1
$cpu = $A::FromHandle($bh).FindFirst($TS::Descendants, (New-Object System.Windows.Automation.PropertyCondition($A::AutomationIdProperty, 'module.cpu')))
if ($cpu) {
    $cpu.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke(); Start-Sleep -Milliseconds 900
    $ph = [BV.W]::FindWindowW('windowsRICE.Popup', [NullString]::Value)
    $pr = [BV.W]::Rect($ph)
    Check 'panels of the bottom bar open above it' (($ph -ne [IntPtr]::Zero) -and ($pr.Bottom -le $bottom.Top) -and ($pr.Top -ge $mon.Top)) "panel $($pr.Top)-$($pr.Bottom), bar top $($bottom.Top)"
} else {
    Check 'bottom bar shows the CPU module' $false
}
# Explorer restart (simulated: the TaskbarCreated broadcast is sent to the
# controller only; Explorer itself is not restarted): bars register again.
$tc = [BV.W]::RegisterWindowMessageW('TaskbarCreated')
[void][BV.W]::PostMessageW([BV.W]::FindWindowW('windowsRICE.Controller', [NullString]::Value), $tc, [IntPtr]::Zero, [IntPtr]::Zero)
Start-Sleep -Seconds 2
$again = [BV.W]::Info()
Check 'after TaskbarCreated both bars reserve their strips again' (($again.rcWork.Top -eq $after.rcWork.Top) -and ($again.rcWork.Bottom -eq $after.rcWork.Bottom) -and (@(Bars).Count -eq 2)) "work $($again.rcWork.Top)-$($again.rcWork.Bottom)"
# Clean exit releases both reservations.
[void][BV.W]::PostMessageW([BV.W]::FindWindowW('windowsRICE.Controller', [NullString]::Value), 0x8005, [IntPtr]::Zero, [IntPtr]::Zero)
$null = $p.WaitForExit(10000)
Start-Sleep -Milliseconds 800
$end = [BV.W]::Info()
Check 'exiting releases the reserved space' (($end.rcWork.Top -eq $before.rcWork.Top) -and ($end.rcWork.Bottom -eq $before.rcWork.Bottom)) "work $($end.rcWork.Top)-$($end.rcWork.Bottom), before $($before.rcWork.Top)-$($before.rcWork.Bottom)"
if (-not $p.HasExited) { Stop-Process -Id $p.Id -Force }
$checks | ConvertTo-Json | Set-Content (Join-Path $Out 'checks.json') -Encoding utf8
$failed = @($checks | Where-Object { -not $_.pass }).Count
"{0} checks, {1} failed" -f $checks.Count, $failed
exit [int]($failed -gt 0)
