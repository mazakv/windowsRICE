# Phase 3.5 spike: accessibility-tree and keyboard check (EXPERIMENTAL).
#
# For each candidate: start it, dump its UI Automation tree (what screen
# readers see), give its own window focus, post Tab, Tab, Enter key messages
# to that window handle only (no global input injection), capture a
# screenshot, and read the stats file to see whether Enter changed the
# selection. Writes results under results\a11y.

param([string]$Root = (Split-Path $PSScriptRoot -Parent))

$ErrorActionPreference = 'Stop'
Set-Location $Root
$env:TMP = Join-Path $Root 'tmp'; $env:TEMP = $env:TMP
$out = Join-Path $Root 'results\a11y'
New-Item -ItemType Directory -Force $out | Out-Null

Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
Add-Type -Namespace Spike -Name Win -MemberDefinition @'
[DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr hWnd, uint msg, IntPtr wParam, IntPtr lParam);
[DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
[DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
[DllImport("user32.dll")] public static extern IntPtr GetFocus();
public delegate bool EnumProc(IntPtr h, IntPtr l);
[DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr l);
[DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
[DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
public static IntPtr FindByPid(int pid) {
    IntPtr found = IntPtr.Zero;
    EnumWindows((h, l) => { uint p; GetWindowThreadProcessId(h, out p); if (p == (uint)pid && IsWindowVisible(h)) { found = h; return false; } return true; }, IntPtr.Zero);
    return found;
}
'@

$WM_KEYDOWN = 0x0100; $WM_KEYUP = 0x0101; $WM_CHAR = 0x0102
function Send-Key([IntPtr]$h, [int]$vk, [int]$scan, [char]$ch) {
    $down = [IntPtr](1 -bor ($scan -shl 16))
    $up = [IntPtr]([long]1 -bor ($scan -shl 16) -bor (1L -shl 30) -bor (1L -shl 31))
    [void][Spike.Win]::PostMessageW($h, $WM_KEYDOWN, [IntPtr]$vk, $down)
    if ($ch -ne [char]0) { [void][Spike.Win]::PostMessageW($h, $WM_CHAR, [IntPtr][int]$ch, $down) }
    Start-Sleep -Milliseconds 120
    [void][Spike.Win]::PostMessageW($h, $WM_KEYUP, [IntPtr]$vk, $up)
    Start-Sleep -Milliseconds 300
}

# Run with Windows PowerShell 5.1 (powershell.exe): PowerShell 7 loads a
# second UIAutomationClient assembly and the types clash.
function Dump-Tree($el, [int]$depth, [System.Collections.Generic.List[string]]$lines) {
    if ($null -eq $el) { $lines.Add('(no UI Automation element for window)'); return }
    $fmt = '{0} name="{1}" focusable={2} enabled={3}'
    $c = $el.Current
    $lines.Add(($fmt -f $c.ControlType.ProgrammaticName, $c.Name, $c.IsKeyboardFocusable, $c.IsEnabled))
    $all = $el.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
    foreach ($d in $all) {
        $dc = $d.Current
        $lines.Add('  ' + ($fmt -f $dc.ControlType.ProgrammaticName, $dc.Name, $dc.IsKeyboardFocusable, $dc.IsEnabled))
    }
}

$configs = @(
    @{ tag = 'slint-femtovg'; exe = 'slint-bar.exe'; extra = @() },
    @{ tag = 'slint-software'; exe = 'slint-bar.exe'; extra = @('--renderer', 'software') },
    @{ tag = 'd2d'; exe = 'd2d-bar.exe'; extra = @() }
)
$summary = @()
foreach ($c in $configs) {
    $stats = Join-Path $out "$($c.tag)-stats.json"
    $p = Start-Process -FilePath "target\release\$($c.exe)" -ArgumentList (@('--exit-after', '14', '--stats', $stats) + $c.extra) -PassThru
    Start-Sleep -Seconds 3
    # Tool windows have no MainWindowHandle; find the visible window by PID.
    $hwnd = [Spike.Win]::FindByPid($p.Id)
    if ($hwnd -eq [IntPtr]::Zero) { throw "no visible window for $($c.tag)" }
    $lines = [System.Collections.Generic.List[string]]::new()
    $uiaRoot = [System.Windows.Automation.AutomationElement]::FromHandle($hwnd)
    Dump-Tree $uiaRoot 0 $lines
    $lines | Set-Content (Join-Path $out "$($c.tag)-uia-tree.txt") -Encoding utf8

    $fgBefore = [Spike.Win]::GetForegroundWindow()
    try { $uiaRoot.SetFocus() } catch { }
    [void][Spike.Win]::SetForegroundWindow($hwnd)
    Start-Sleep -Milliseconds 400
    $focused = ([Spike.Win]::GetForegroundWindow() -eq $hwnd)

    Send-Key $hwnd 0x09 0x0F ([char]9)     # Tab
    Send-Key $hwnd 0x09 0x0F ([char]9)     # Tab  → third pill (index 2) if focus started on pill 1
    & .\target\release\shot.exe (Join-Path $out "$($c.tag)-after-tab.png") 0 0 900 70 2
    Send-Key $hwnd 0x0D 0x1C ([char]13)    # Enter
    Start-Sleep -Milliseconds 500
    & .\target\release\shot.exe (Join-Path $out "$($c.tag)-after-enter.png") 0 0 900 70 2

    # Return foreground to whatever the user had before.
    if ($fgBefore -ne [IntPtr]::Zero) { [void][Spike.Win]::SetForegroundWindow($fgBefore) }
    $null = $p.WaitForExit(30000)
    $j = Get-Content $stats -Raw | ConvertFrom-Json
    $buttons = @($lines | Where-Object { $_ -match 'ControlType.Button' }).Count
    $summary += [pscustomobject]@{
        candidate = $c.tag
        uia_nodes = $lines.Count
        uia_buttons = $buttons
        window_got_foreground = $focused
        enter_changed_selection = (@($j.interactions).Count -gt 0)
        exit_code = $p.ExitCode
    }
}
$summary | Format-Table -AutoSize | Out-String -Width 200
$summary | ConvertTo-Json | Set-Content (Join-Path $out 'summary.json') -Encoding utf8
