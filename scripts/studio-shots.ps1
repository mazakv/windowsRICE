# Captures every Design Studio section from the running release build
# (Studio window only; isolated configuration; fixed window title).
# Windows PowerShell 5.1.
param(
    [string]$ProjectRoot = (Split-Path $PSScriptRoot -Parent),
    [string]$Out = (Join-Path (Split-Path $PSScriptRoot -Parent) 'target\phase8\shots'),
    [string]$Lang = 'en',
    [string]$Appearance = ''
)
$ErrorActionPreference = 'Stop'
Set-Location $ProjectRoot
if (Test-Path $Out) { Remove-Item -Recurse -Force $Out }
New-Item -ItemType Directory -Force $Out | Out-Null
if ($Appearance) { Set-Content (Join-Path $Out 'studio.toml') "appearance = $Appearance`n" }
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
Add-Type -Namespace WSS -Name W -MemberDefinition @'
[DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindowW(string cls, string title);
[DllImport("user32.dll")] public static extern bool SetProcessDpiAwarenessContext(IntPtr v);
[DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
[DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr h, int attr, out RECT r, int size);
[DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr hWnd, uint msg, IntPtr wParam, IntPtr lParam);
[StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
'@
[void][WSS.W]::SetProcessDpiAwarenessContext([IntPtr](-4))
$A = [System.Windows.Automation.AutomationElement]; $TS = [System.Windows.Automation.TreeScope]
$p = Start-Process 'target\release\windowsrice-panel.exe' -ArgumentList @('--studio', '--config', (Join-Path $Out 'config.toml'), '--stats', (Join-Path $Out 'run.json'), '--no-hotkey', '--lang', $Lang, '--exit-after', '120', '--title', '"Documents - File Explorer"') -PassThru
for ($i = 0; $i -lt 80; $i++) { $h = [WSS.W]::FindWindowW('windowsRICE.Studio', [NullString]::Value); if ($h -ne [IntPtr]::Zero) { break }; Start-Sleep -Milliseconds 100 }
Start-Sleep -Seconds 3
function ById([string]$id) { $A::FromHandle($h).FindFirst($TS::Descendants, (New-Object System.Windows.Automation.PropertyCondition($A::AutomationIdProperty, $id))) }
function Shot([string]$name) { $r = New-Object WSS.W+RECT; [void][WSS.W]::DwmGetWindowAttribute($h, 9, [ref]$r, 16); & "$ProjectRoot\experiments\ui-spike\target\release\shot.exe" (Join-Path $Out "$name.png") ($r.L + 8) ($r.T + 8) ($r.R - $r.L - 16) ($r.B - $r.T - 16) 1 | Out-Null }
function Wheel([int]$notches) { for ($k = 0; $k -lt [math]::Abs($notches); $k++) { $d = if ($notches -lt 0) { 120 } else { -120 }; $r = New-Object WSS.W+RECT; [void][WSS.W]::GetWindowRect($h, [ref]$r); $x = [int](($r.L + $r.R) / 2); $y = [int](($r.T + $r.B) / 2); [void][WSS.W]::PostMessageW($h, 0x020A, [IntPtr](($d -band 0xFFFF) -shl 16), [IntPtr](($y -shl 16) -bor ($x -band 0xFFFF))); Start-Sleep -Milliseconds 80 } ; Start-Sleep -Milliseconds 400 }
. (Join-Path $PSScriptRoot 'studio-nav.ps1')
foreach ($sec in $StudioPages) {
    $null = Open-StudioPage $sec { param($x) ById $x }
    Start-Sleep -Milliseconds 700
    Shot "section-$sec"
    if ($sec -in 'styles', 'typography', 'background', 'colors', 'modules') {
        Wheel 5
        Shot "section-$sec-scrolled"
    }
}
$r = New-Object WSS.W+RECT; [void][WSS.W]::GetWindowRect($h, [ref]$r)
[void][WSS.W]::PostMessageW($h, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)
Start-Sleep -Seconds 1
$bar = [WSS.W]::FindWindowW('windowsRICE.Panel', [NullString]::Value)
[void][WSS.W]::PostMessageW($bar, 0x0104, [IntPtr]0x73, [IntPtr]([long]1 -bor (0x3E -shl 16) -bor (1L -shl 29)))
$null = $p.WaitForExit(15000)
if (-not $p.HasExited) { Stop-Process -Id $p.Id -Force }
$j = Get-Content (Join-Path $Out 'run.json') -Raw | ConvertFrom-Json
$b = @($j.studio_build_us) | Sort-Object
"errors: $(@($j.errors).Count) a11y: $(@($j.a11y_problems).Count) builds: $($b.Count) median_us: $($b[[int]($b.Count/2)]) max_us: $($b[-1])"
$j.popup_log | Where-Object { $_ -match 'fonts' }
