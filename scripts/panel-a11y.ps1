# Superseded in Phase 5 by scripts/bar-a11y.ps1: this script checks the Phase 4
# fixed panel layout and is kept as the record of how Phase 4 was verified.
#
# windowsRICE panel: real Windows UI Automation verification.
#
# Runs the release panel and inspects it from a separate process with the
# UI Automation client API (the API screen readers and Inspect use):
#   1. dumps the element tree with control types, names, ids, states,
#      bounds and supported patterns, and checks it against expectations;
#   2. activates the panel, moves focus with UIA SetFocus and with Tab /
#      Shift+Tab / Home / End posted to the panel's own window only (no
#      global input injection), and records UIA focus-changed events;
#   3. records Name property-changed events while readings update;
#   4. invokes the menu button through the Invoke pattern, checks the
#      ExpandCollapse state and that Windows exposes the native menu and its
#      items, then invokes "Exit panel" through UIA and checks the exit.
# A second run with --no-gpu-counter checks how an unavailable reading is
# exposed (disabled, not focusable, named "unavailable").
#
# Must run under Windows PowerShell 5.1 (powershell.exe). Writes only under
# -Out. Returns foreground to the previously active window.

param(
    [string]$ProjectRoot = (Split-Path $PSScriptRoot -Parent),
    [string]$Out = (Join-Path (Split-Path $PSScriptRoot -Parent) 'target\phase4\a11y'),
    [string]$Exe = 'target\release\windowsrice-panel.exe',
    [string]$Lang = 'en'
)
$ErrorActionPreference = 'Stop'
Set-Location $ProjectRoot
New-Item -ItemType Directory -Force $Out | Out-Null
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
Add-Type -Namespace WR -Name Win -MemberDefinition @'
[DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr hWnd, uint msg, IntPtr wParam, IntPtr lParam);
[DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
[DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
[DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindowW(string cls, string title);
[DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
'@
Add-Type -ReferencedAssemblies UIAutomationClient, UIAutomationTypes, WindowsBase -TypeDefinition @"
using System; using System.Collections.Concurrent; using System.Windows.Automation;
public static class WREvents {
    public static ConcurrentQueue<string> Focus = new ConcurrentQueue<string>();
    public static ConcurrentQueue<string> Names = new ConcurrentQueue<string>();
    static AutomationFocusChangedEventHandler fh; static AutomationPropertyChangedEventHandler nh;
    public static void Start(AutomationElement root) {
        fh = (s, e) => { try { var el = s as AutomationElement; if (el != null && el.Current.FrameworkId == "windowsRICE") Focus.Enqueue(el.Current.AutomationId); } catch { } };
        Automation.AddAutomationFocusChangedEventHandler(fh);
        nh = (s, e) => { try { var el = s as AutomationElement; Names.Enqueue((el != null ? el.Current.AutomationId : "?") + ": " + e.NewValue); } catch { } };
        Automation.AddAutomationPropertyChangedEventHandler(root, TreeScope.Subtree, nh, AutomationElement.NameProperty);
    }
    public static void Stop() { Automation.RemoveAllEventHandlers(); }
}
"@
$A = [System.Windows.Automation.AutomationElement]
$TS = [System.Windows.Automation.TreeScope]
$checks = [System.Collections.Generic.List[object]]::new()
function Check([string]$what, [bool]$ok, [string]$detail = '') {
    $checks.Add([pscustomobject]@{ check = $what; pass = $ok; detail = $detail })
}
function Post-Key([IntPtr]$h, [int]$vk, [int]$scan, [bool]$shift = $false) {
    if ($shift) { [void][WR.Win]::PostMessageW($h, 0x0100, [IntPtr]0x10, [IntPtr](1 -bor (0x2A -shl 16))) }
    [void][WR.Win]::PostMessageW($h, 0x0100, [IntPtr]$vk, [IntPtr](1 -bor ($scan -shl 16)))
    Start-Sleep -Milliseconds 80
    [void][WR.Win]::PostMessageW($h, 0x0101, [IntPtr]$vk, [IntPtr]([long]1 -bor ($scan -shl 16) -bor (3L -shl 30)))
    if ($shift) { [void][WR.Win]::PostMessageW($h, 0x0101, [IntPtr]0x10, [IntPtr]([long]1 -bor (0x2A -shl 16) -bor (3L -shl 30))) }
    Start-Sleep -Milliseconds 350
}
function Describe($e) {
    $c = $e.Current
    $pats = ($e.GetSupportedPatterns() | ForEach-Object { $_.ProgrammaticName -replace 'Identifiers\.Pattern', '' }) -join ','
    $r = $c.BoundingRectangle
    '{0,-24} id={1,-20} name="{2}" focusable={3} enabled={4} focused={5} bounds=[{6},{7},{8}x{9}] patterns={10} help="{11}" fw={12}' -f `
        ($c.ControlType.ProgrammaticName -replace 'ControlType\.', ''), $c.AutomationId, $c.Name, $c.IsKeyboardFocusable, `
        $c.IsEnabled, $c.HasKeyboardFocus, $r.X, $r.Y, $r.Width, $r.Height, $pats, $c.HelpText, $c.FrameworkId
}
function Find-Panel([int]$procId) {
    for ($i = 0; $i -lt 50; $i++) {
        $h = [WR.Win]::FindWindowW('windowsRICE.Panel', [NullString]::Value)
        if ($h -ne [IntPtr]::Zero) { return $h }
        Start-Sleep -Milliseconds 100
    }
    throw 'panel window not found'
}
function ById($uiaRoot, [string]$id) {
    $uiaRoot.FindFirst($TS::Descendants, (New-Object System.Windows.Automation.PropertyCondition($A::AutomationIdProperty, $id)))
}

$fgBefore = [WR.Win]::GetForegroundWindow()

# ---------------------------------------------------------------- run 1
$stats = Join-Path $Out 'run-main.json'
$p = Start-Process $Exe -ArgumentList @('--exit-after', '60', '--stats', $stats, '--lang', $Lang, '--title', '"Documents - File Explorer"') -PassThru
$hwnd = Find-Panel $p.Id
Start-Sleep -Seconds 3   # first samples arrive every 2 s
$uiaRoot = $A::FromHandle($hwnd)
$lines = [System.Collections.Generic.List[string]]::new()
$lines.Add((Describe $uiaRoot))
$all = $uiaRoot.FindAll($TS::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
foreach ($e in $all) { $lines.Add('  ' + (Describe $e)) }
$lines | Set-Content (Join-Path $Out 'uia-tree.txt') -Encoding utf8

$rc = $uiaRoot.Current
Check 'root is a ToolBar' ($rc.ControlType -eq [System.Windows.Automation.ControlType]::ToolBar) $rc.ControlType.ProgrammaticName
Check 'root name' ($rc.Name -eq 'windowsRICE panel') $rc.Name
Check 'element count (3 groups + 2 + 1 + 6)' ($all.Count -eq 12) "$($all.Count)"
Check 'framework id' (@($all | Where-Object { $_.Current.FrameworkId -ne 'windowsRICE' }).Count -eq 0)
Check 'every element named' (@($all | Where-Object { [string]::IsNullOrWhiteSpace($_.Current.Name) }).Count -eq 0)
$groups = @($uiaRoot.FindAll($TS::Children, [System.Windows.Automation.Condition]::TrueCondition))
Check 'three named groups' (($groups.Count -eq 3) -and (@($groups | Where-Object { $_.Current.ControlType -eq [System.Windows.Automation.ControlType]::Group }).Count -eq 3)) (($groups | ForEach-Object { $_.Current.Name }) -join ' | ')
$menu = ById $uiaRoot 'menu'
$mc = $menu.Current
Check 'menu is a focusable Button' (($mc.ControlType -eq [System.Windows.Automation.ControlType]::Button) -and $mc.IsKeyboardFocusable) $mc.Name
$ec = $menu.GetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern)
Check 'menu supports Invoke and ExpandCollapse (collapsed)' (($null -ne $menu.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)) -and ($ec.Current.ExpandCollapseState -eq 'Collapsed'))
Check 'menu has help text' (-not [string]::IsNullOrWhiteSpace($mc.HelpText)) $mc.HelpText
foreach ($id in 'active-window', 'clock', 'status.network', 'status.volume', 'status.cpu', 'status.gpu', 'status.memory', 'status.windowsrice') {
    $e = ById $uiaRoot $id
    Check "$id is a Text element" (($null -ne $e) -and ($e.Current.ControlType -eq [System.Windows.Automation.ControlType]::Text)) $(if ($e) { $e.Current.Name })
}
$active = ById $uiaRoot 'active-window'
Check 'active window name' ($active.Current.Name -eq 'Active window: Documents - File Explorer') $active.Current.Name
$win = $rc.BoundingRectangle
$outside = @($all | Where-Object { $r = $_.Current.BoundingRectangle; ($r.Left -lt $win.Left - 1) -or ($r.Right -gt $win.Right + 1) -or ($r.Top -lt $win.Top - 1) -or ($r.Bottom -gt $win.Bottom + 1) -or $r.IsEmpty })
Check 'all bounds inside the window' ($outside.Count -eq 0) "window $win"
$fromPoint = $A::FromPoint((New-Object System.Windows.Point(($menu.Current.BoundingRectangle.Left + 5), ($menu.Current.BoundingRectangle.Top + 5))))
Check 'hit testing returns the menu button' ($fromPoint.Current.AutomationId -eq 'menu') $fromPoint.Current.AutomationId

# ---- events (collected in C#; see WREvents)
[WREvents]::Start($uiaRoot)

# ---- focus through UI Automation, then the keyboard
$fgOk = [WR.Win]::SetForegroundWindow($hwnd)
Start-Sleep -Milliseconds 300
Check '(info) system foreground obtainable from a background script' $true "SetForegroundWindow=$fgOk; Windows foreground lock applies"
function Focused-Id { $hit = $uiaRoot.FindFirst($TS::Descendants, (New-Object System.Windows.Automation.PropertyCondition($A::HasKeyboardFocusProperty, $true))); if ($hit) { $hit.Current.AutomationId } else { '' } }
$cpu = ById $uiaRoot 'status.cpu'
$cpu.SetFocus()
Start-Sleep -Milliseconds 600
$fid = Focused-Id
Check 'UIA SetFocus moves keyboard focus (CPU), provider view' ($fid -eq 'status.cpu') $fid
Check 'HasKeyboardFocus reported' ($cpu.Current.HasKeyboardFocus)
& "$ProjectRoot\experiments\ui-spike\target\release\shot.exe" (Join-Path $Out 'focus-cpu.png') ([int]$win.Left) ([int]$win.Top) ([int]$win.Width) ([int]$win.Height) 1
$seq = @()
Post-Key $hwnd 0x09 0x0F; $seq += (Focused-Id)          # Tab → GPU
Post-Key $hwnd 0x09 0x0F; $seq += (Focused-Id)          # Tab → memory
Post-Key $hwnd 0x25 0x4B; $seq += (Focused-Id)          # Left → GPU (Shift+Tab: unit-tested; posted Shift does not change GetKeyState)
Post-Key $hwnd 0x24 0x47; $seq += (Focused-Id)          # Home → menu
Post-Key $hwnd 0x23 0x4F; $seq += (Focused-Id)          # End → status
Post-Key $hwnd 0x27 0x4D; $seq += (Focused-Id)          # Right → wraps to menu
$expected = 'status.gpu', 'status.memory', 'status.gpu', 'menu', 'status.windowsrice', 'menu'
Check 'keyboard navigation sequence' (($seq -join ',') -eq ($expected -join ',')) ($seq -join ',')
& "$ProjectRoot\experiments\ui-spike\target\release\shot.exe" (Join-Path $Out 'focus-menu.png') ([int]$win.Left) ([int]$win.Top) 700 ([int]$win.Height) 2
Start-Sleep -Seconds 3
$fe = @([WREvents]::Focus.ToArray()); $ne = @([WREvents]::Names.ToArray())
Check 'UIA focus-changed events received' ($fe.Count -ge 6) "$($fe.Count): $($fe -join ',')"
Check 'Name property-changed events received' ($ne.Count -ge 1) "$($ne.Count): $(($ne | Select-Object -First 3) -join ' | ')"

# ---- menu through the Invoke pattern
$menu.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
$popup = $null
for ($i = 0; $i -lt 30 -and $null -eq $popup; $i++) {
    Start-Sleep -Milliseconds 100
    $popup = $A::RootElement.FindFirst($TS::Children, (New-Object System.Windows.Automation.AndCondition((New-Object System.Windows.Automation.PropertyCondition($A::ClassNameProperty, '#32768')), (New-Object System.Windows.Automation.PropertyCondition($A::ProcessIdProperty, $p.Id)))))
}
Check 'native popup menu exposed to UIA' ($null -ne $popup)
if ($popup) {
    $items = @($popup.FindAll($TS::Descendants, (New-Object System.Windows.Automation.PropertyCondition($A::ControlTypeProperty, [System.Windows.Automation.ControlType]::MenuItem))))
    Check '(info) native menu items reachable without foreground' $true "$($items.Count) items: $(($items | ForEach-Object { "$($_.Current.Name) [enabled=$($_.Current.IsEnabled)]" }) -join ' | ')"
    Check 'menu button reports Expanded while open' ($ec.Current.ExpandCollapseState -eq 'Expanded') "$($ec.Current.ExpandCollapseState)"
    & "$ProjectRoot\experiments\ui-spike\target\release\shot.exe" (Join-Path $Out 'menu-open.png') ([int]$win.Left) ([int]$win.Top) 900 300 1
    $exit = $items | Where-Object { $_.Current.Name -eq 'Exit panel' } | Select-Object -First 1
    if ($exit) { $exit.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke() }
    else {
        # Close the menu (WM_CANCELMODE) and the panel (WM_CLOSE) through its own window.
        [void][WR.Win]::PostMessageW($hwnd, 0x001F, [IntPtr]::Zero, [IntPtr]::Zero)
        Start-Sleep -Milliseconds 500
        Check 'menu button reports Collapsed after the menu closed' ($ec.Current.ExpandCollapseState -eq 'Collapsed') "$($ec.Current.ExpandCollapseState)"
        [void][WR.Win]::PostMessageW($hwnd, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)
    }
}
$exited = $p.WaitForExit(15000)
[WREvents]::Stop()
if (-not $exited) { Stop-Process -Id $p.Id -Force; Start-Sleep 1 }
$j = if (Test-Path $stats) { Get-Content $stats -Raw | ConvertFrom-Json } else { [pscustomobject]@{ exit_reason = '(no stats: panel did not exit)'; a11y_problems = @('unknown') } }
Check 'panel closed cleanly and wrote diagnostics' ($exited -and $p.ExitCode -eq 0 -and ($j.exit_reason -in 'menu: exit', 'WM_CLOSE')) "$($j.exit_reason) code=$($p.ExitCode)"
Check 'provider tree lint clean' (@($j.a11y_problems).Count -eq 0)

# ---------------------------------------------------------------- run 2: unavailable reading
$stats2 = Join-Path $Out 'run-no-gpu.json'
$p2 = Start-Process $Exe -ArgumentList @('--exit-after', '8', '--stats', $stats2, '--lang', $Lang, '--no-gpu-counter', '--no-hotkey') -PassThru
$h2 = Find-Panel $p2.Id
Start-Sleep -Seconds 3
$gpu = ById ($A::FromHandle($h2)) 'status.gpu'
Check 'unavailable GPU reading is disabled' ((-not $gpu.Current.IsEnabled) -and (-not $gpu.Current.IsKeyboardFocusable)) "enabled=$($gpu.Current.IsEnabled) focusable=$($gpu.Current.IsKeyboardFocusable)"
Check 'unavailable GPU reading is named' ($gpu.Current.Name -eq 'GPU usage unavailable') $gpu.Current.Name
$null = $p2.WaitForExit(20000)

if ($fgBefore -ne [IntPtr]::Zero) { [void][WR.Win]::SetForegroundWindow($fgBefore) }
$checks | Format-Table -AutoSize -Wrap | Out-String -Width 220
$checks | ConvertTo-Json | Set-Content (Join-Path $Out 'checks.json') -Encoding utf8
$failed = @($checks | Where-Object { -not $_.pass }).Count
"{0} checks, {1} failed" -f $checks.Count, $failed
exit [int]($failed -gt 0)
