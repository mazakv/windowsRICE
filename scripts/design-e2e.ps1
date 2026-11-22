# windowsRICE Phases 7 and 8: critical end-to-end Apply test.
#
# Changes the design step by step in the running Design Studio (UI
# Automation), presses Apply after each step and checks the REAL running
# bar: pixels captured from the screen, the bar's own UI Automation tree,
# and the bar's "apply verified" record of the style it actually uses.
# A changed config.toml alone never counts as success.
#
# Privacy: the bar can be translucent, so a test backdrop window is kept
# behind it for the whole run (scripts/backdrop-window.ps1, pinned to the
# top edge, bar above it) and the window title is fixed. A bar capture is
# only taken when the backdrop is verifiably in place. Windows PowerShell 5.1.

param(
    [string]$ProjectRoot = (Split-Path $PSScriptRoot -Parent),
    [string]$Out = (Join-Path (Split-Path $PSScriptRoot -Parent) 'target\phase8\e2e'),
    [string]$Exe = 'target\release\windowsrice-panel.exe'
)
$ErrorActionPreference = 'Stop'
Set-Location $ProjectRoot
if (Test-Path $Out) { Remove-Item -Recurse -Force $Out }
New-Item -ItemType Directory -Force $Out | Out-Null
$cfg = Join-Path $Out 'config.toml'
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes, System.Drawing
Add-Type -Namespace WDE -Name W -MemberDefinition @'
[DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindowW(string cls, string title);
[DllImport("user32.dll")] public static extern bool SetProcessDpiAwarenessContext(IntPtr v);
[DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
[DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr h, int attr, out RECT r, int size);
[DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr hWnd, uint msg, IntPtr wParam, IntPtr lParam);
[DllImport("user32.dll")] public static extern IntPtr GetWindow(IntPtr h, uint cmd);
[StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
'@
[void][WDE.W]::SetProcessDpiAwarenessContext([IntPtr](-4))
$A = [System.Windows.Automation.AutomationElement]; $TS = [System.Windows.Automation.TreeScope]
$checks = [System.Collections.Generic.List[object]]::new()
function Check([string]$what, [bool]$ok, [string]$detail = '') { $checks.Add([pscustomobject]@{ check = $what; pass = $ok; detail = $detail }); Write-Host ("{0} {1} {2}" -f ($(if ($ok) { 'PASS' } else { 'FAIL' })), $what, $detail) }
function Wait-Win([string]$cls, [string]$title) { for ($i = 0; $i -lt 100; $i++) { $h = [WDE.W]::FindWindowW($cls, $title); if ($h -ne [IntPtr]::Zero) { return $h }; Start-Sleep -Milliseconds 100 }; [IntPtr]::Zero }
function Studio { [WDE.W]::FindWindowW('windowsRICE.Studio', [NullString]::Value) }
function ByIdRaw([string]$id) { $A::FromHandle((Studio)).FindFirst($TS::Descendants, (New-Object System.Windows.Automation.PropertyCondition($A::AutomationIdProperty, $id))) }
# Elements outside the visible part of a page are not in the tree until
# scrolled into view (small screens such as 1024x768 CI runners); scroll for them.
function ById([string]$id) { $e = ByIdRaw $id; if ($e) { return $e }; try { Reveal $id } catch { $null } }
function ByName([string]$name) { $A::FromHandle((Studio)).FindFirst($TS::Descendants, (New-Object System.Windows.Automation.PropertyCondition($A::NameProperty, $name))) }
function Sel($e) { if (-not $e) { throw 'element not found' }; $e.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Select(); Start-Sleep -Milliseconds 500 }
# Actions are processed asynchronously: a control is invoked once it is
# enabled (up to 5 s), so a click never lands on a still-disabled button.
function Wait-Enabled($e, [string]$id) { for ($w = 0; $w -lt 33 -and $e -and -not $e.Current.IsEnabled; $w++) { Start-Sleep -Milliseconds 150; $e = ById $id }; if ($e -and -not $e.Current.IsEnabled) { throw "$id stayed disabled" }; $e }
function Inv([string]$id) { $e = ById $id; if (-not $e) { throw "no $id" }; $e = Wait-Enabled $e $id; $e.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke(); Start-Sleep -Milliseconds 500 }
function SetVal([string]$id, [string]$v) { (ById $id).GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue($v); Start-Sleep -Milliseconds 500 }
. (Join-Path $PSScriptRoot 'studio-nav.ps1')
function Nav([string]$sec) { $null = Open-StudioPage $sec { param($x) ById $x }; Start-Sleep -Milliseconds 100 }
function Status { (ById 'status').Current.Name }
function IsSelected([string]$id) { $e = ById $id; $e -and $e.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Current.IsSelected }
# Scrolls the section until the element is in the tree.
function Reveal([string]$id) {
    $e = ByIdRaw $id; if ($e) { return $e }
    $r = New-Object WDE.W+RECT; [void][WDE.W]::GetWindowRect((Studio), [ref]$r)
    $x = [int]($r.L + ($r.R - $r.L) * 0.45); $y = [int]($r.B - 80)
    for ($k = 0; $k -lt 40; $k++) { [void][WDE.W]::PostMessageW((Studio), 0x020A, [IntPtr]([int](120 -band 0xFFFF) -shl 16), [IntPtr](($y -shl 16) -bor ($x -band 0xFFFF))) }
    Start-Sleep -Milliseconds 500
    for ($k = 0; $k -lt 30; $k++) {
        $e = ByIdRaw $id; if ($e) { return $e }
        $r = New-Object WDE.W+RECT; [void][WDE.W]::GetWindowRect((Studio), [ref]$r)
        $x = [int]($r.L + ($r.R - $r.L) * 0.45); $y = [int]($r.B - 80)
        [void][WDE.W]::PostMessageW((Studio), 0x020A, [IntPtr]([int](-120 -band 0xFFFF) -shl 16), [IntPtr](($y -shl 16) -bor ($x -band 0xFFFF)))
        Start-Sleep -Milliseconds 250
    }
    throw "could not reveal $id"
}
function Apply { Inv 'apply'; Start-Sleep -Milliseconds 1200; Status }
function Run-Json { Get-Content (Join-Path $Out 'run.json') -Raw | ConvertFrom-Json }

# The backdrop must cover the strip under the bar, and the bar must be
# above it, before any bar capture.
$script:bd = $null
function Start-Backdrop {
    $script:bd = Start-Process powershell.exe -ArgumentList @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', (Join-Path $ProjectRoot 'scripts\backdrop-window.ps1'), '-Kind', 'checker', '-Seconds', '600', '-Height', '240') -PassThru -WindowStyle Hidden
    $null = Wait-Win ([NullString]::Value) 'windowsRICE test backdrop'
}
function Bar-Shot([string]$name) {
    $bar = [WDE.W]::FindWindowW('windowsRICE.Panel', [NullString]::Value)
    $bdh = [WDE.W]::FindWindowW([NullString]::Value, 'windowsRICE test backdrop')
    $br = New-Object WDE.W+RECT; [void][WDE.W]::GetWindowRect($bdh, [ref]$br)
    $above = $false; $w = $bdh; for ($k = 0; $k -lt 500; $k++) { $w = [WDE.W]::GetWindow($w, 3); if ($w -eq [IntPtr]::Zero) { break }; if ($w -eq $bar) { $above = $true; break } }
    $r = New-Object WDE.W+RECT; [void][WDE.W]::GetWindowRect($bar, [ref]$r)
    if ($bdh -eq [IntPtr]::Zero -or $br.T -ne 0 -or $br.B -lt $r.B -or -not $above) { throw "backdrop not under the bar; refusing to capture ($name)" }
    $p = Join-Path $Out "bar-$name.png"
    & "$ProjectRoot\experiments\ui-spike\target\release\shot.exe" $p $r.L $r.T ($r.R - $r.L) ($r.B - $r.T) 1 | Out-Null
    $p
}
function Px([string]$png, [double]$fx, [int]$y = -1) {
    $b = [System.Drawing.Bitmap]::FromFile($png); if ($y -lt 0) { $y = [int]($b.Height / 2) }; $c = $b.GetPixel([int]($b.Width * $fx), [math]::Min($y, $b.Height - 1)); $b.Dispose(); $c
}
function Hex($c) { '#{0:X2}{1:X2}{2:X2}' -f $c.R, $c.G, $c.B }
function Region-Diff([string]$a, [string]$b, [double]$x0, [double]$x1) {
    $ia = [System.Drawing.Bitmap]::FromFile($a); $ib = [System.Drawing.Bitmap]::FromFile($b); $n = 0
    $h = [math]::Min($ia.Height, $ib.Height)
    for ($x = [int]($ia.Width * $x0); $x -lt [int]($ia.Width * $x1); $x += 2) { for ($y = 2; $y -lt $h - 2; $y += 2) { if ($ia.GetPixel($x, $y).ToArgb() -ne $ib.GetPixel($x, $y).ToArgb()) { $n++ } } }
    $ia.Dispose(); $ib.Dispose(); $n
}
function Start-App([string]$stats) {
    $p = Start-Process $Exe -ArgumentList @('--studio', '--config', $cfg, '--stats', (Join-Path $Out $stats), '--no-hotkey', '--lang', 'en', '--exit-after', '500', '--title', '"Documents - File Explorer"') -PassThru
    $null = Wait-Win 'windowsRICE.Studio' ([NullString]::Value); Start-Sleep -Seconds 3; $p
}
function Stop-App($p) {
    $bar = [WDE.W]::FindWindowW('windowsRICE.Panel', [NullString]::Value)
    [void][WDE.W]::PostMessageW($bar, 0x0104, [IntPtr]0x73, [IntPtr]([long]1 -bor (0x3E -shl 16) -bor (1L -shl 29)))
    $null = $p.WaitForExit(15000); if (-not $p.HasExited) { Stop-Process -Id $p.Id -Force }
}
# Element of a module in the bar's own UI Automation tree, with its region.
function Bar-Module([string]$id) {
    $bar = $A::FromHandle([WDE.W]::FindWindowW('windowsRICE.Panel', [NullString]::Value))
    $e = $bar.FindFirst($TS::Descendants, (New-Object System.Windows.Automation.PropertyCondition($A::AutomationIdProperty, "module.$id")))
    if (-not $e) { return $null }
    $parent = [System.Windows.Automation.TreeWalker]::RawViewWalker.GetParent($e)
    [pscustomobject]@{ name = $e.Current.Name; region = $parent.Current.AutomationId; x = $e.Current.BoundingRectangle.Left; w = $e.Current.BoundingRectangle.Width }
}

Start-Backdrop
$p = Start-App 'run1.json'
$before = Bar-Shot '0-before'
Check 'release bar and Design Studio are running' ((Studio) -ne [IntPtr]::Zero)

# ---- 1. overall style
Nav 'styles'
$pv0 = (ById 'preview:summary').Current.Name
Sel (ById 'card:style:4')                      # Light Modern
Check 'style card selected (whole card is the target)' (IsSelected 'card:style:4')
Check 'draft marked unapplied' ((Status) -match 'not applied')
$st = Apply
$s1 = Bar-Shot '1-style'
$bg1 = Px $s1 0.38
Check 'Light Modern reaches the real bar (light surface)' (($bg1.R + $bg1.G + $bg1.B) / 3 -gt 200) "bar $(Hex $bg1); $st"
Check 'Apply verified against the running bar' ($st -match 'Verified on 1 bar') $st

# ---- 2. colour palette
Nav 'colors'
Sel (Reveal 'card:palette:0')                  # Ocean Blue
Check 'palette card selected' (IsSelected 'card:palette:0')
$st = Apply
$s2 = Bar-Shot '2-palette'
$bg2 = Px $s2 0.38
Check 'Ocean Blue background on the real bar (#0E2A3F)' ((Hex $bg2) -eq '#0E2A3F') "bar $(Hex $bg2)"
Check 'palette accent reaches the menu mark' ((Region-Diff $s1 $s2 0.0 0.02) -gt 5)

# ---- 3. font
Nav 'typography'
SetVal 'field:font-search' 'Cascadia'
$card = Reveal 'font:0'
Check 'font card shows the family name' ($card.Current.Name -like 'Cascadia*') $card.Current.Name
Sel $card
Check 'font card selected' ($card.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Current.IsSelected -or (IsSelected 'font:0'))
$family = ($card.Current.Name -split '\.')[0]
$st = Apply
$s3 = Bar-Shot '3-font'
Check 'font change redraws the bar text' ((Region-Diff $s2 $s3 0.40 0.60) -gt 50)
Check 'status names the font the bar now uses' ($st -match [regex]::Escape($family)) $st

# ---- 4. background appearance
Nav 'background'
Sel (Reveal 'card:background:1')               # Translucent
$st = Apply
$s4 = Bar-Shot '4-background'
$bg4 = Px $s4 0.38
Check 'translucent background shows the backdrop through the real bar' ((Hex $bg4) -ne (Hex $bg2)) "solid $(Hex $bg2) -> translucent $(Hex $bg4)"

# ---- 5. module appearance
Nav 'modules'
Sel (Reveal 'card:look:3')                     # Icon + label + value
$st = Apply
$s5 = Bar-Shot '5-modules'
Check 'module appearance changes the right side of the real bar' ((Region-Diff $s4 $s5 0.62 0.98) -gt 50)

# ---- 6. layout
Nav 'layout'
$clockBefore = Bar-Module 'clock'
Sel (Reveal 'card:layout:3')                   # Right focus
$st = Apply
$clockAfter = Bar-Module 'clock'
Check 'layout moves the clock in the real bar (UI Automation)' ($clockAfter.region -eq 'region.right' -and $clockBefore.region -eq 'region.center') "$($clockBefore.region) -> $($clockAfter.region)"
$s6 = Bar-Shot '6-layout'

# ---- 7. icons and effects
Nav 'icons'
Sel (Reveal 'card:icons:3')                    # Colourful
Nav 'effects'
Sel (Reveal 'card:effect:10')                  # Accent indicator
Check 'effect card shows on' (IsSelected 'card:effect:10')
$st = Apply
$s7 = Bar-Shot '7-effects'
$b7 = [System.Drawing.Bitmap]::FromFile($s7); $line = $b7.GetPixel([int]($b7.Width * 0.38), $b7.Height - 2); $b7.Dispose()
Check 'accent line drawn along the bottom of the real bar' ((Hex $line) -eq '#4FB3FF') "bottom pixel $(Hex $line)"
Check 'colourful icons change the real bar' ((Region-Diff $s6 $s7 0.62 0.98) -gt 20)

# ---- 7b. Phase 9: one module's own appearance
Nav 'modules'
Sel (Reveal 'card:cpu')
Sel (Reveal 'look:c0:7')                      # CPU accent: Teal #1FA39A
Sel (Reveal 'look:content:4')                 # CPU shows icon, label and value
$st = Apply
$s7b = Bar-Shot '7b-module'
function Teal-Pixels([string]$png, $mod) {
    $b = [System.Drawing.Bitmap]::FromFile($png); $n = 0
    $bar = [WDE.W]::FindWindowW('windowsRICE.Panel', [NullString]::Value); $r = New-Object WDE.W+RECT; [void][WDE.W]::GetWindowRect($bar, [ref]$r)
    $x0 = [int]($mod.x - $r.L); $x1 = [int]($x0 + $mod.w)
    for ($x = [math]::Max(0, $x0); $x -lt [math]::Min($b.Width, $x1); $x++) { for ($y = 0; $y -lt $b.Height; $y++) { $c = $b.GetPixel($x, $y); if ([math]::Abs($c.R - 0x1F) -lt 28 -and [math]::Abs($c.G - 0xA3) -lt 28 -and [math]::Abs($c.B - 0x9A) -lt 28) { $n++ } } }
    $b.Dispose(); $n
}
$cpu = Bar-Module 'cpu'
$teal = Teal-Pixels $s7b $cpu
Check 'CPU module drawn in its own accent on the real bar' ($teal -gt 20) "teal pixels in CPU module: $teal"
$ram = Bar-Module 'ram'
Check 'other modules keep the bar style' ((Teal-Pixels $s7b $ram) -lt 5)
Check 'CPU module shows its label (Everything)' ($cpu.name -like 'CPU*')

# ---- 8. save the complete design as a preset
Nav 'presets'
SetVal 'field:preset-name' 'E2E Complete Look'
Inv 'props:psave'
$pf = Join-Path $Out 'presets\e2e-complete-look.toml'
$pt = if (Test-Path $pf) { Get-Content $pf -Raw } else { '' }
Check 'complete design saved as a preset' (($pt -match 'windowsrice-preset') -and ($pt -match '#0E2A3F') -and ($pt -match 'accent_line = "bottom"') -and ($pt -match 'Cascadia') -and ($pt -match 'color = "#1FA39A"')) $pf
$cfgText = Get-Content $cfg -Raw
Stop-App $p

# ---- 9. restart: the design persists
$p = Start-App 'run2.json'
$cfgAtRestart = Get-Content $cfg -Raw
$s9 = Bar-Shot '9-restarted'
$b9 = [System.Drawing.Bitmap]::FromFile($s9); $line9 = $b9.GetPixel([int]($b9.Width * 0.38), $b9.Height - 2); $b9.Dispose()
Check 'after restart the bar shows the saved design' ((Hex $line9) -eq '#4FB3FF' -and (Region-Diff $s7 $s9 0.0 0.30) -lt 400) "bottom $(Hex $line9)"
Check 'after restart the clock is still on the right' ((Bar-Module 'clock').region -eq 'region.right')
$s9b = Bar-Shot '9b-restarted-module'
Check 'after restart the CPU module keeps its own accent' ((Teal-Pixels $s9b (Bar-Module 'cpu')) -gt 20)
# ---- 10. animations off: the next Apply runs without a fade
Nav 'advanced'
(Reveal 'adv:animations').GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Toggle(); Start-Sleep -Milliseconds 500
$st = Apply
Check 'animation switch saved' ((Get-Content $cfg -Raw) -match 'animations = false') $st
Nav 'colors'
Check 'reopened Studio marks the palette' (IsSelected (Reveal 'card:palette:0').Current.AutomationId)
Nav 'effects'
Check 'reopened Studio marks the effect' (IsSelected (Reveal 'card:effect:10').Current.AutomationId)
Check 'config unchanged by restart' ($cfgAtRestart -eq $cfgText)
Stop-App $p
Stop-Process -Id $script:bd.Id -Force -ErrorAction SilentlyContinue

foreach ($f in 'run1.json', 'run2.json') {
    $j = Get-Content (Join-Path $Out $f) -Raw | ConvertFrom-Json
    Check "$f clean: no errors, no accessibility problems" ((@($j.errors).Count -eq 0) -and (@($j.a11y_problems).Count -eq 0)) "errors=$($j.errors -join '; ')"
    $j.popup_log | Where-Object { $_ -match 'apply verified' } | ForEach-Object { Write-Host "  $_" }
}
$j1 = Get-Content (Join-Path $Out 'run1.json') -Raw | ConvertFrom-Json
# The fade follows Windows "Animation effects" as the application read it:
# on, Apply must fade (the frame count depends on the machine's speed);
# off, no fade may run.
$fades = @($j1.popup_log | Where-Object { $_ -match 'apply fade' }).Count
if ($j1.windows_animations) {
    Check 'Apply fades the new design in when animations are on' ($fades -ge 2) "$fades fade frames (Windows animation effects on)"
} else {
    Check 'Apply does not fade when Windows animation effects are off' ($fades -eq 0) "$fades fade frames (Windows animation effects off)"
}
$j2 = Get-Content (Join-Path $Out 'run2.json') -Raw | ConvertFrom-Json
$log2 = @($j2.popup_log)
$lastVerified = [array]::FindLastIndex([string[]]$log2, [Predicate[string]]{ param($l) $l -match 'apply verified' })
$lastFade = [array]::FindLastIndex([string[]]$log2, [Predicate[string]]{ param($l) $l -match 'apply fade' })
Check 'no fade after animations were turned off' ($lastVerified -ge 0 -and $lastFade -lt $lastVerified -and $lastFade -eq -1) "verified@$lastVerified fade@$lastFade"
$checks | ConvertTo-Json | Set-Content (Join-Path $Out 'checks.json') -Encoding utf8
$failed = @($checks | Where-Object { $_.pass -is [bool] -and -not $_.pass }).Count
"{0} checks, {1} failed" -f $checks.Count, $failed
exit [int]($failed -gt 0)
