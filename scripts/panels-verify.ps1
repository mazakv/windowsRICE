# Phase 13: module panels on the running bar (isolated configuration, no
# screenshots). Opens the CPU, memory, network, volume, clock (calendar) and
# notification panels through UI Automation and checks their content,
# keyboard closing and the calendar controls. Results:
# target\phase13\panels\checks.json. The volume is not changed (the volume
# buttons are checked for presence only).
param(
    [string]$ProjectRoot = (Split-Path $PSScriptRoot -Parent),
    [string]$Exe = 'target\release\windowsrice-panel.exe'
)
$ErrorActionPreference = 'Stop'
Set-Location $ProjectRoot
if (@(Get-Process windowsrice-panel -ErrorAction SilentlyContinue).Count -gt 0) { throw 'a windowsRICE bar is running; close it first' }
$Out = Join-Path $ProjectRoot 'target\phase13\panels'
if (Test-Path $Out) { Remove-Item -Recurse -Force $Out }
New-Item -ItemType Directory -Force $Out | Out-Null
$cfg = Join-Path $Out 'config.toml'
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
$A = [System.Windows.Automation.AutomationElement]; $TS = [System.Windows.Automation.TreeScope]
Add-Type -Namespace PV -Name W -MemberDefinition '[DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindowW(string c, string t); [DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr h, uint m, IntPtr w, IntPtr l);'
$checks = [System.Collections.Generic.List[object]]::new()
function Check([string]$what, [bool]$ok, [string]$detail = '') { $checks.Add([pscustomobject]@{ check = $what; pass = $ok; detail = $detail }); '{0,-62} {1} {2}' -f $what, $ok, $detail }
function Win([string]$cls) { for ($i = 0; $i -lt 60; $i++) { $h = [PV.W]::FindWindowW($cls, [NullString]::Value); if ($h -ne [IntPtr]::Zero) { return $h }; Start-Sleep -Milliseconds 150 }; [IntPtr]::Zero }
function ById($h, [string]$id) { $A::FromHandle($h).FindFirst($TS::Descendants, (New-Object System.Windows.Automation.PropertyCondition($A::AutomationIdProperty, $id))) }
function Inv($e) { $e.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke(); Start-Sleep -Milliseconds 700 }
function Panel { [PV.W]::FindWindowW('windowsRICE.Popup', [NullString]::Value) }
function Close-Panel { $p = Panel; if ($p -ne [IntPtr]::Zero) { $c = ById $p 'panel:close'; if ($c) { Inv $c } } }

$stats = Join-Path $Out 'run.json'
$p = Start-Process $Exe -ArgumentList @('--config', $cfg, '--stats', $stats, '--no-hotkey', '--lang', 'en', '--exit-after', '120', '--title', '"Panel test"') -PassThru
$bar = Win 'windowsRICE.Panel'
Start-Sleep -Seconds 4   # first readings

$cpu = ById $bar 'module.cpu'
Check 'CPU module is a button with Invoke' ($cpu -and $cpu.Current.ControlType.ProgrammaticName -eq 'ControlType.Button' -and ($cpu.GetSupportedPatterns() | Where-Object { $_.ProgrammaticName -like 'Invoke*' }))
Inv $cpu
$ph = Panel
Check 'clicking CPU opens a panel named CPU' (($ph -ne [IntPtr]::Zero) -and ($A::FromHandle($ph).Current.Name -eq 'CPU'))
$v = ById $ph 'panel:value'
Check 'CPU panel shows the current usage' ($v -and $v.Current.Name -match '^\d+%$') $(if ($v) { $v.Current.Name })
$rows = @($A::FromHandle($ph).FindAll($TS::Descendants, [System.Windows.Automation.Condition]::TrueCondition) | Where-Object { $_.Current.AutomationId -like 'panel:row*' } | ForEach-Object { $_.Current.Name })
Check 'unmeasured values are marked, not invented' (($rows -contains 'Per-core usage: not available') -and ($rows | Where-Object { $_ -like 'Processor: *' -and $_ -notlike '*not available' })) ($rows -join ' | ')
Start-Sleep -Seconds 5
$g = ById (Panel) 'panel:graph'
Check 'history graph is described for assistive technology' ($g -and $g.Current.Name -match 'History, last \d+ seconds: lowest \d+%, highest \d+%') $(if ($g) { $g.Current.Name })
# Escape closes (WM_KEYDOWN VK_ESCAPE to the panel).
[void][PV.W]::PostMessageW((Panel), 0x0100, [IntPtr]0x1B, [IntPtr]0); Start-Sleep -Milliseconds 700
Check 'Escape closes the panel' ((Panel) -eq [IntPtr]::Zero)

foreach ($m in @(@{ id = 'module.ram'; name = 'Memory'; row = 'Total: ' }, @{ id = 'module.download'; name = 'Network'; row = 'Connection: ' })) {
    $e = ById $bar $m.id
    if (-not $e) { Check "$($m.name) module present" $false; continue }
    Inv $e
    $ph = Panel
    $rows = @($A::FromHandle($ph).FindAll($TS::Descendants, [System.Windows.Automation.Condition]::TrueCondition) | Where-Object { $_.Current.AutomationId -like 'panel:row*' } | ForEach-Object { $_.Current.Name })
    Check "$($m.name) panel opens with its details" (($A::FromHandle($ph).Current.Name -eq $m.name) -and ($rows | Where-Object { $_ -like "$($m.row)*" })) ($rows -join ' | ')
    Close-Panel
}

$vol = ById $bar 'module.volume'
if ($vol -and $vol.Current.IsEnabled) {
    Inv $vol
    $ph = Panel
    $sl = ById $ph 'panel:volume'
    Check 'Volume panel has a slider with its value and buttons' ($sl -and ($sl.Current.ControlType.ProgrammaticName -eq 'ControlType.Slider') -and (ById $ph 'panel:vol-up') -and (ById $ph 'panel:mute'))
    Close-Panel
} else {
    $checks.Add([pscustomobject]@{ check = 'Volume panel'; pass = 'NOT RUN'; detail = 'no audio output device' })
}

$clock = ById $bar 'module.clock'
Inv $clock
$ph = Panel
$days = @($A::FromHandle($ph).FindAll($TS::Descendants, [System.Windows.Automation.Condition]::TrueCondition) | Where-Object { $_.Current.AutomationId -like 'cal:day:*' })
$today = @($days | Where-Object { $_.Current.Name -like '*, today' })
$now = Get-Date
Check 'clock opens the calendar for the current month' (($days.Count -eq [DateTime]::DaysInMonth($now.Year, $now.Month)) -and ((ById $ph 'cal:month').Current.Name -eq $now.ToString('MMMM yyyy', [Globalization.CultureInfo]::InvariantCulture))) "$($days.Count) days"
Check 'today is marked' ($today.Count -eq 1 -and $today[0].Current.AutomationId -eq "cal:day:$($now.Day)") $(if ($today) { $today[0].Current.Name })
Inv (ById $ph 'cal:next')
$next = $now.AddMonths(1)
Check 'next month navigation' ((ById (Panel) 'cal:month').Current.Name -eq $next.ToString('MMMM yyyy', [Globalization.CultureInfo]::InvariantCulture)) (ById (Panel) 'cal:month').Current.Name
Inv (ById (Panel) 'cal:day:15')
$sel = (ById (Panel) 'cal:day:15').GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Current.IsSelected
Check 'a day can be selected' $sel
Inv (ById (Panel) 'cal:today')
Check 'Today returns to the current month' ((ById (Panel) 'cal:month').Current.Name -eq $now.ToString('MMMM yyyy', [Globalization.CultureInfo]::InvariantCulture))
Close-Panel

$notif = ById $bar 'module.notifications'
Inv $notif
$ph = Panel
$acc = ById $ph 'notif:access'
Check 'notification center explains why Windows notifications are not readable' ($acc -and $acc.Current.Name -match 'package identity')
Check 'no invented notification count (no windowsRICE notices yet)' ((ById $ph 'notif:app').Current.Name -eq 'windowsRICE notices (0 unread)') (ById $ph 'notif:app').Current.Name
Close-Panel

[void][PV.W]::PostMessageW((Win 'windowsRICE.Controller'), 0x8001, [IntPtr]::Zero, [IntPtr]::Zero)
Start-Sleep -Seconds 1
Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 500
if (Test-Path $stats) {
    $j = Get-Content $stats -Raw | ConvertFrom-Json
    Check 'no accessibility problems or errors' ((@($j.a11y_problems).Count -eq 0) -and (@($j.errors).Count -eq 0)) ((@($j.a11y_problems) + @($j.errors)) -join '; ')
}
$checks | ConvertTo-Json | Set-Content (Join-Path $Out 'checks.json') -Encoding utf8
$failed = @($checks | Where-Object { $_.pass -is [bool] -and -not $_.pass }).Count
"{0} checks, {1} failed" -f $checks.Count, $failed
exit [int]($failed -gt 0)
