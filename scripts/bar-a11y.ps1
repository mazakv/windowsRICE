# windowsRICE Bar (Phase 5): real Windows verification through UI Automation.
#
# Drives the release bar from a separate process with the UI Automation
# client API and messages posted to the bar's own windows only (no global
# input). Uses a project-local config file (never %APPDATA%). Checks:
#   - bar geometry (full width, top edge) and the work-area reservation;
#   - bar tree: roles, names, ids, states, patterns, bounds, hit testing;
#   - provider-level keyboard focus and UIA focus / name-change events;
#   - the settings popup: tree, roles, states; toggling a module, moving a
#     module, changing an interval, choosing a theme, changing the height,
#     releasing and re-taking the reserved space, auto-hide; each change is
#     checked in the saved config.toml and in the bar's tree or geometry;
#   - closing the popup and exiting from it, releasing the reservation;
#   - persistence after a restart, and v2 -> v3 migration with the original
#     file preserved before the first save.
# Must run under Windows PowerShell 5.1 (powershell.exe).

param(
    [string]$ProjectRoot = (Split-Path $PSScriptRoot -Parent),
    [string]$Out = (Join-Path (Split-Path $PSScriptRoot -Parent) 'target\phase5\a11y'),
    [string]$Exe = 'target\release\windowsrice-panel.exe'
)
$ErrorActionPreference = 'Stop'
Set-Location $ProjectRoot
New-Item -ItemType Directory -Force $Out | Out-Null
Get-ChildItem $Out -Filter 'config.toml*' -ErrorAction SilentlyContinue | Remove-Item -Force
$cfg = Join-Path $Out 'config.toml'
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
Add-Type -Namespace WB -Name Win -MemberDefinition @'
[DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr hWnd, uint msg, IntPtr wParam, IntPtr lParam);
[DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindowW(string cls, string title);
[DllImport("user32.dll")] public static extern bool SetProcessDpiAwarenessContext(IntPtr v);
[DllImport("user32.dll")] public static extern IntPtr MonitorFromPoint(POINT pt, uint flags);
[DllImport("user32.dll")] public static extern bool GetMonitorInfoW(IntPtr h, ref MONITORINFO mi);
[DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
[StructLayout(LayoutKind.Sequential)] public struct POINT { public int X; public int Y; }
[StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
[StructLayout(LayoutKind.Sequential)] public struct MONITORINFO { public int cbSize; public RECT rcMonitor; public RECT rcWork; public uint dwFlags; }
public static RECT Work() { var mi = new MONITORINFO(); mi.cbSize = Marshal.SizeOf(mi); GetMonitorInfoW(MonitorFromPoint(new POINT(), 1), ref mi); return mi.rcWork; }
public static RECT WindowRect(IntPtr h) { RECT r; GetWindowRect(h, out r); return r; }
'@
[void][WB.Win]::SetProcessDpiAwarenessContext([IntPtr](-4))

# Primary monitor geometry (physical pixels) and scale, so the checks work on
# any screen (CI runners included), not only the 3840 px / 150 % original.
Add-Type -Namespace WScr -Name S -MemberDefinition '[DllImport("user32.dll")] public static extern int GetSystemMetrics(int i); [DllImport("user32.dll")] public static extern uint GetDpiForSystem();'
$ScreenW = [WScr.S]::GetSystemMetrics(0)
$Scale = [WScr.S]::GetDpiForSystem() / 96.0
function Px([double]$dip) { [int][math]::Round($dip * $Scale) }
# The bar can be translucent, so whatever is behind it shows through. A
# pinned test backdrop (scripts/backdrop-window.ps1) stays behind it for the
# whole run, and no screenshot is taken unless that backdrop is in place.
Add-Type -Namespace WGuard -Name G -MemberDefinition '[DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindowW(string c, string t); [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r); [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }'
$guardProc = Start-Process powershell.exe -ArgumentList @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', (Join-Path $ProjectRoot 'scripts\backdrop-window.ps1'), '-Kind', 'checker', '-Seconds', '1200', '-Height', '1600') -PassThru -WindowStyle Hidden
for ($gi = 0; $gi -lt 100; $gi++) { if ([WGuard.G]::FindWindowW([NullString]::Value, 'windowsRICE test backdrop') -ne [IntPtr]::Zero) { break }; Start-Sleep -Milliseconds 100 }
function Assert-Backdrop { $gh = [WGuard.G]::FindWindowW([NullString]::Value, 'windowsRICE test backdrop'); $gr = New-Object WGuard.G+RECT; [void][WGuard.G]::GetWindowRect($gh, [ref]$gr); if ($gh -eq [IntPtr]::Zero -or $gr.T -ne 0) { throw 'test backdrop not in place; refusing to capture' } }

Add-Type -ReferencedAssemblies UIAutomationClient, UIAutomationTypes, WindowsBase -TypeDefinition @"
using System; using System.Collections.Concurrent; using System.Windows.Automation;
public static class WBEvents {
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
$CT = [System.Windows.Automation.ControlType]
$checks = [System.Collections.Generic.List[object]]::new()
function Check([string]$what, [bool]$ok, [string]$detail = '') { $checks.Add([pscustomobject]@{ check = $what; pass = $ok; detail = $detail }) }
function Wait-Window([string]$class) { for ($i = 0; $i -lt 60; $i++) { $h = [WB.Win]::FindWindowW($class, [NullString]::Value); if ($h -ne [IntPtr]::Zero) { return $h }; Start-Sleep -Milliseconds 100 }; return [IntPtr]::Zero }
function ById($root, [string]$id) { $root.FindFirst($TS::Descendants, (New-Object System.Windows.Automation.PropertyCondition($A::AutomationIdProperty, $id))) }
function Focused-Id($root) { $h = $root.FindFirst($TS::Descendants, (New-Object System.Windows.Automation.PropertyCondition($A::HasKeyboardFocusProperty, $true))); if ($h) { $h.Current.AutomationId } else { '' } }
function Post-Key([IntPtr]$h, [int]$vk, [int]$scan) {
    [void][WB.Win]::PostMessageW($h, 0x0100, [IntPtr]$vk, [IntPtr](1 -bor ($scan -shl 16)))
    Start-Sleep -Milliseconds 60
    [void][WB.Win]::PostMessageW($h, 0x0101, [IntPtr]$vk, [IntPtr]([long]1 -bor ($scan -shl 16) -bor (3L -shl 30)))
    Start-Sleep -Milliseconds 300
}
function Invoke-Id($root, [string]$id) { (ById $root $id).GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke(); Start-Sleep -Milliseconds 500 }
function Toggle-Id($root, [string]$id) { (ById $root $id).GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Toggle(); Start-Sleep -Milliseconds 500 }
function Config-Text { if (Test-Path $cfg) { Get-Content $cfg -Raw } else { '' } }
function Module-Block([string]$text, [string]$id) { $m = [regex]::Match($text, "(?s)\[\[bar\.modules\]\]\s*id = `"$id`".*?(?=\[\[bar\.modules\]\]|\z)"); $m.Value }
function Start-Bar([string[]]$extra) { Start-Process $Exe -ArgumentList (@('--config', $cfg, '--no-hotkey', '--lang', 'en', '--title', '"Documents - File Explorer"') + $extra) -PassThru }
function Shot([string]$name, [int]$x, [int]$y, [int]$w, [int]$h) { Assert-Backdrop; & "$ProjectRoot\experiments\ui-spike\target\release\shot.exe" (Join-Path $Out $name) $x $y $w $h 1 }

$workBefore = [WB.Win]::Work()

# ================================================================ run 1
$stats1 = Join-Path $Out 'run1.json'
$p = Start-Bar @('--exit-after', '120', '--stats', $stats1)
$bar = Wait-Window 'windowsRICE.Panel'
Check 'bar window created' ($bar -ne [IntPtr]::Zero)
Start-Sleep -Seconds 3
$wr = [WB.Win]::WindowRect($bar)
$mon = New-Object WB.Win+MONITORINFO; $mon.cbSize = [Runtime.InteropServices.Marshal]::SizeOf($mon)
$work = [WB.Win]::Work()
Check 'full width, top edge' (($wr.Left -eq 0) -and ($wr.Top -eq 0) -and ($wr.Right -eq $ScreenW)) "$($wr.Left),$($wr.Top),$($wr.Right),$($wr.Bottom)"
Check 'work area starts below the bar' ($work.Top -eq $wr.Bottom) "work top $($work.Top) (before $($workBefore.Top)), bar bottom $($wr.Bottom)"
$root = $A::FromHandle($bar)
$all = $root.FindAll($TS::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
$lines = [System.Collections.Generic.List[string]]::new()
foreach ($e in @($root) + @($all)) { $c = $e.Current; $r = $c.BoundingRectangle; $pats = ($e.GetSupportedPatterns() | ForEach-Object { $_.ProgrammaticName -replace 'Identifiers\.Pattern', '' }) -join ','; $lines.Add(('{0,-10} id={1,-24} name="{2}" focusable={3} enabled={4} bounds=[{5},{6},{7}x{8}] patterns={9}' -f ($c.ControlType.ProgrammaticName -replace 'ControlType\.', ''), $c.AutomationId, $c.Name, $c.IsKeyboardFocusable, $c.IsEnabled, $r.X, $r.Y, $r.Width, $r.Height, $pats)) }
$lines | Set-Content (Join-Path $Out 'bar-tree.txt') -Encoding utf8
Check 'bar root is a ToolBar named "windowsRICE bar"' (($root.Current.ControlType -eq $CT::ToolBar) -and ($root.Current.Name -eq 'windowsRICE bar'))
$groups = @($root.FindAll($TS::Children, [System.Windows.Automation.Condition]::TrueCondition) | ForEach-Object { $_.Current.Name })
Check 'three region groups' (($groups -join '|') -eq 'Left section|Center section|Right section') ($groups -join ' | ')
Check 'every element named' (@($all | Where-Object { [string]::IsNullOrWhiteSpace($_.Current.Name) }).Count -eq 0)
foreach ($id in 'module.menu', 'module.active-window', 'module.clock', 'module.download', 'module.upload', 'module.network', 'module.volume', 'module.cpu', 'module.gpu', 'module.vram', 'module.ram', 'module.notifications') {
    $e = ById $root $id
    Check "$id present" ($null -ne $e) $(if ($e) { $e.Current.Name })
}
Check 'battery hidden on a computer without one' ($null -eq (ById $root 'module.battery'))
$menu = ById $root 'module.menu'
$ec = $menu.GetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern)
Check 'menu: Button with Invoke + ExpandCollapse, collapsed' (($menu.Current.ControlType -eq $CT::Button) -and ($ec.Current.ExpandCollapseState -eq 'Collapsed'))
$outside = @($all | Where-Object { $r = $_.Current.BoundingRectangle; ($r.Left -lt $wr.Left - 1) -or ($r.Right -gt $wr.Right + 1) -or ($r.Top -lt $wr.Top - 1) -or ($r.Bottom -gt $wr.Bottom + 1) })
Check 'all bounds inside the bar' ($outside.Count -eq 0)
$mr = $menu.Current.BoundingRectangle
Check 'hit testing returns the menu' (($A::FromPoint((New-Object System.Windows.Point(($mr.Left + 4), ($mr.Top + 4))))).Current.AutomationId -eq 'module.menu')
Shot 'bar-full.png' 0 0 $ScreenW ($wr.Bottom + 6)

[WBEvents]::Start($root)
(ById $root 'module.cpu').SetFocus(); Start-Sleep -Milliseconds 500
Check 'UIA SetFocus (provider view)' ((Focused-Id $root) -eq 'module.cpu') (Focused-Id $root)
$seq = @()
Post-Key $bar 0x09 0x0F; $seq += (Focused-Id $root)
Post-Key $bar 0x25 0x4B; $seq += (Focused-Id $root)
Post-Key $bar 0x24 0x47; $seq += (Focused-Id $root)
Post-Key $bar 0x23 0x4F; $seq += (Focused-Id $root)
Check 'keyboard: Tab, Left, Home, End' (($seq -join ',') -eq 'module.gpu,module.cpu,module.menu,module.notifications') ($seq -join ',')
Shot 'bar-focus.png' 3000 0 840 ($wr.Bottom + 4)
Start-Sleep -Seconds 2.5
$fe = @([WBEvents]::Focus.ToArray()); $ne = @([WBEvents]::Names.ToArray())
Check 'UIA focus-changed events' ($fe.Count -ge 5) "$($fe.Count): $($fe -join ',')"
Check 'UIA Name-changed events' ($ne.Count -ge 1) "$($ne.Count): $(($ne | Select-Object -First 2) -join ' | ')"

# ---- popup
Invoke-Id $root 'module.menu'
$pop = Wait-Window 'windowsRICE.Popup'
Check 'popup window opened' ($pop -ne [IntPtr]::Zero)
Start-Sleep -Milliseconds 600
$proot = $A::FromHandle($pop)
$pall = $proot.FindAll($TS::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
$plines = [System.Collections.Generic.List[string]]::new()
foreach ($e in @($proot) + @($pall)) { $c = $e.Current; $pats = ($e.GetSupportedPatterns() | ForEach-Object { $_.ProgrammaticName -replace 'Identifiers\.Pattern', '' }) -join ','; $plines.Add(('{0,-12} id={1,-28} name="{2}" focusable={3} enabled={4} patterns={5} help="{6}"' -f ($c.ControlType.ProgrammaticName -replace 'ControlType\.', ''), $c.AutomationId, $c.Name, $c.IsKeyboardFocusable, $c.IsEnabled, $pats, $c.HelpText)) }
$plines | Set-Content (Join-Path $Out 'popup-tree.txt') -Encoding utf8
Check 'popup root: Window "windowsRICE settings"' (($proot.Current.ControlType -eq $CT::Window) -and ($proot.Current.Name -eq 'windowsRICE settings'))
Check 'menu reports Expanded while popup is open' ($ec.Current.ExpandCollapseState -eq 'Expanded')
$radios = @($pall | Where-Object { $_.Current.ControlType -eq $CT::RadioButton })
Check 'one theme radio button per built-in theme (17), one selected' (($radios.Count -eq 17) -and (@($radios | Where-Object { $_.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Current.IsSelected }).Count -eq 1)) "$($radios.Count)"
$checksBoxes = @($pall | Where-Object { $_.Current.ControlType -eq $CT::CheckBox })
Check 'module and bar check boxes (15 + 3)' ($checksBoxes.Count -eq 18) "$($checksBoxes.Count)"
Check 'menu check box disabled' (-not (ById $proot 'module:menu:show').Current.IsEnabled)
Check 'battery listed as not on this computer' ((ById $proot 'module:battery:show').Current.Name -eq 'Show Battery — not on this computer') (ById $proot 'module:battery:show').Current.Name
Check 'every popup element named' (@($pall | Where-Object { [string]::IsNullOrWhiteSpace($_.Current.Name) }).Count -eq 0)
$pr = [WB.Win]::WindowRect($pop)
Shot 'popup.png' $pr.Left $pr.Top ($pr.Right - $pr.Left) ($pr.Bottom - $pr.Top)

# toggle CPU off
Toggle-Id $proot 'module:cpu:show'
Check 'toggle CPU off: bar no longer shows CPU' ($null -eq (ById $root 'module.cpu'))
Check 'toggle CPU off: saved' ((Module-Block (Config-Text) 'cpu') -match 'enabled = false')
Check 'toggle state reported Off' ((ById $proot 'module:cpu:show').GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Current.ToggleState -eq 'Off')
# move GPU up (before network-related modules: swap with cpu entry above it)
function Right-Order { @((ById $root 'region.right').FindAll($TS::Children, [System.Windows.Automation.Condition]::TrueCondition) | ForEach-Object { $_.Current.AutomationId }) }
Invoke-Id $proot 'module:vram:up'
$orderAfter = Right-Order
$gi = [array]::IndexOf($orderAfter, 'module.gpu'); $vi = [array]::IndexOf($orderAfter, 'module.vram')
Check 'move VRAM up: VRAM now before GPU in the bar' ($vi -ge 0 -and $vi -lt $gi) (($orderAfter) -join ',')
Check 'move VRAM up: saved order' ((Config-Text).IndexOf('id = "vram"') -lt (Config-Text).IndexOf('id = "gpu"'))
# interval
Invoke-Id $proot 'module:ram:shorter'
Check 'RAM interval 5 s -> 2 s saved' ((Module-Block (Config-Text) 'ram') -match 'interval_ms = 2000')
# theme
$glass = ById $proot 'theme:windowsrice-glass'
$glass.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Select(); Start-Sleep -Milliseconds 1200
$proot = $A::FromHandle($pop)
Check 'theme Glass selected and saved' (((Config-Text) -match 'active = "windowsrice-glass"') -and (ById $proot 'theme:windowsrice-glass').GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Current.IsSelected)
Shot 'bar-glass.png' 0 0 $ScreenW (Px 47)
$pr = [WB.Win]::WindowRect($pop); Shot 'popup-glass.png' $pr.Left $pr.Top ($pr.Right - $pr.Left) ($pr.Bottom - $pr.Top)
# height (Glass bar token is 36): +2 -> 38 DIPs (57 px at 150 %)
Invoke-Id $proot 'bar:height+'
Start-Sleep -Milliseconds 500
$wr2 = [WB.Win]::WindowRect($bar); $work2 = [WB.Win]::Work()
Check 'height +: bar 38 DIPs and work area follows' (($wr2.Bottom -eq (Px 38)) -and ($work2.Top -eq (Px 38))) "bar bottom $($wr2.Bottom), work top $($work2.Top)"
Check 'height saved' ((Config-Text) -match '(?m)^height = 38')
# reserve off and on
Toggle-Id $proot 'bar:reserve'
Start-Sleep -Milliseconds 500
$work3 = [WB.Win]::Work()
Check 'reserve off: work area released' ($work3.Top -eq $workBefore.Top) "work top $($work3.Top)"
Toggle-Id $proot 'bar:reserve'
Start-Sleep -Milliseconds 500
Check 'reserve on: work area taken again' (([WB.Win]::Work()).Top -eq (Px 38)) "work top $(([WB.Win]::Work()).Top)"
# auto-hide: collapse after the delay (pointer not over the bar), expand on pointer
Toggle-Id $proot 'bar:autohide'
Invoke-Id $proot 'close'
Check 'close button closes the popup' (([WB.Win]::FindWindowW('windowsRICE.Popup', [NullString]::Value)) -eq [IntPtr]::Zero)
Check 'menu reports Collapsed after closing' ($ec.Current.ExpandCollapseState -eq 'Collapsed')
[void][WB.Win]::PostMessageW($bar, 0x02A3, [IntPtr]::Zero, [IntPtr]::Zero)   # WM_MOUSELEAVE starts the delay
Start-Sleep -Milliseconds 1500
$wc = [WB.Win]::WindowRect($bar)
# An auto-hidden bar stays open while the real mouse pointer rests on its
# strip (by design). The script never moves the user's pointer, so in that
# case the collapse check is reported as NOT RUN instead of passed or failed.
Add-Type -AssemblyName System.Windows.Forms
$cur = [System.Windows.Forms.Cursor]::Position
$barRect = [WB.Win]::WindowRect($bar)
if ($cur.Y -le ($barRect.Bottom + 2) -and $cur.X -ge $barRect.Left -and $cur.X -lt $barRect.Right) {
    $checks.Add([pscustomobject]@{ check = 'auto-hide: collapsed to a 2 px strip, work area released'; pass = 'NOT RUN'; detail = "the mouse pointer rests on the bar strip at $($cur.X),$($cur.Y), which keeps an auto-hidden bar open by design" })
} else {
    Check 'auto-hide: collapsed to a 2 px strip, work area released' ((($wc.Bottom - $wc.Top) -eq 2) -and (([WB.Win]::Work()).Top -eq $workBefore.Top)) "height $($wc.Bottom - $wc.Top), work top $(([WB.Win]::Work()).Top)"
}
[void][WB.Win]::PostMessageW($bar, 0x0200, [IntPtr]::Zero, [IntPtr](100 -bor (1 -shl 16)))   # WM_MOUSEMOVE at the top edge
Start-Sleep -Milliseconds 400
$we = [WB.Win]::WindowRect($bar)
Check 'auto-hide: pointer at the top edge expands it' (($we.Bottom - $we.Top) -eq (Px 38)) "height $($we.Bottom - $we.Top)"
# exit from the popup (auto-hide back off first, so the next run starts reserved)
Invoke-Id $root 'module.menu'
$pop = Wait-Window 'windowsRICE.Popup'; Start-Sleep -Milliseconds 500
$proot = $A::FromHandle($pop)
Toggle-Id $proot 'bar:autohide'
Invoke-Id $proot 'exit'
$exited = $p.WaitForExit(15000)
[WBEvents]::Stop()
$j = Get-Content $stats1 -Raw | ConvertFrom-Json
Check 'exit from the popup: clean exit, diagnostics written' ($exited -and $p.ExitCode -eq 0 -and $j.exit_reason -eq 'settings: exit') "$($j.exit_reason) code=$($p.ExitCode)"
Start-Sleep -Milliseconds 500
Check 'reservation released after exit' (([WB.Win]::Work()).Top -eq $workBefore.Top) "work top $(([WB.Win]::Work()).Top)"
Check 'provider lint clean during the run' (@($j.a11y_problems).Count -eq 0) ($j.a11y_problems -join '; ')
Check 'no errors during the run' (@($j.errors).Count -eq 0) ($j.errors -join '; ')

# ================================================================ run 2: persistence
$stats2 = Join-Path $Out 'run2.json'
$p2 = Start-Bar @('--exit-after', '6', '--stats', $stats2)
$bar2 = Wait-Window 'windowsRICE.Panel'; Start-Sleep -Seconds 3
$root2 = $A::FromHandle($bar2); $wr = [WB.Win]::WindowRect($bar2)
Check 'restart: CPU still hidden, height 38 kept' (($null -eq (ById $root2 'module.cpu')) -and ($wr.Bottom -eq (Px 38))) "bottom $($wr.Bottom)"
$null = $p2.WaitForExit(20000)
$j2 = Get-Content $stats2 -Raw | ConvertFrom-Json
Check 'restart: Glass theme applied' ($j2.theme.id -eq 'windowsrice-glass') $j2.theme.id

# ================================================================ run 3: migration
Get-ChildItem $Out -Filter 'config.toml*' | Remove-Item -Force
$v2 = "schema_version = 2`n[general]`nlanguage = `"korean`"`n[theme]`nactive = `"windowsrice-minimal`"`n"
[IO.File]::WriteAllText($cfg, $v2)
$stats3 = Join-Path $Out 'run3.json'
$p3 = Start-Bar @('--exit-after', '40', '--stats', $stats3)
$bar3 = Wait-Window 'windowsRICE.Panel'; Start-Sleep -Seconds 3
Check 'v2 file untouched while only read' ((Get-Content $cfg -Raw) -eq $v2)
$root3 = $A::FromHandle($bar3)
Invoke-Id $root3 'module.menu'
$pop3 = Wait-Window 'windowsRICE.Popup'; Start-Sleep -Milliseconds 500
Toggle-Id ($A::FromHandle($pop3)) 'module:gpu:show'
$preserved = @(Get-ChildItem $Out -Filter 'config.toml.v2-*.bak')
Check 'first save preserves the v2 original' (($preserved.Count -eq 1) -and ((Get-Content $preserved[0].FullName -Raw) -eq $v2)) ($preserved.Name -join ',')
$t3 = Config-Text
Check 'migrated file is v3 and keeps v2 values' (($t3 -match 'schema_version = 3') -and ($t3 -match 'active = "windowsrice-minimal"') -and ($t3 -match 'language = "korean"') -and ((Module-Block $t3 'gpu') -match 'enabled = false'))
[void][WB.Win]::PostMessageW($bar3, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)
$null = $p3.WaitForExit(20000)
$j3 = Get-Content $stats3 -Raw | ConvertFrom-Json
Check 'migration reported as a notice' (@($j3.theme.notices | Where-Object { $_ -match 'ConfigMigrated' }).Count -ge 1)

$checks | Format-Table -AutoSize -Wrap | Out-String -Width 230
$checks | ConvertTo-Json | Set-Content (Join-Path $Out 'checks.json') -Encoding utf8
$failed = @($checks | Where-Object { -not $_.pass }).Count
"{0} checks, {1} failed, {2} not run" -f $checks.Count, $failed, @($checks | Where-Object { $_.pass -is [string] }).Count
Stop-Process -Id $guardProc.Id -Force -ErrorAction SilentlyContinue
exit [int]($failed -gt 0)
