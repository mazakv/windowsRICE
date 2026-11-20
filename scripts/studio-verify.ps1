# windowsRICE Design Studio (Phase 6): real Windows verification.
#
# Drives the release application from a separate process through UI
# Automation (the API screen readers use), messages posted to the Studio's
# own window (mouse drag), and the native dialogs' own UI Automation
# elements. Uses a project-local config folder only. Windows PowerShell 5.1.

param(
    [string]$ProjectRoot = (Split-Path $PSScriptRoot -Parent),
    [string]$Out = (Join-Path (Split-Path $PSScriptRoot -Parent) 'target\phase6\verify'),
    [string]$Exe = 'target\release\windowsrice-panel.exe'
)
$ErrorActionPreference = 'Stop'
Set-Location $ProjectRoot
if (Test-Path $Out) { Remove-Item -Recurse -Force $Out }
New-Item -ItemType Directory -Force $Out | Out-Null
$cfg = Join-Path $Out 'config.toml'
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
Add-Type -Namespace WSV -Name W -MemberDefinition @'
[DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr hWnd, uint msg, IntPtr wParam, IntPtr lParam);
[DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindowW(string cls, string title);
[DllImport("user32.dll")] public static extern bool SetProcessDpiAwarenessContext(IntPtr v);
[DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
[DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr h, int attr, out RECT r, int size);
[DllImport("user32.dll")] public static extern bool ScreenToClient(IntPtr h, ref POINT p);
[DllImport("user32.dll")] public static extern IntPtr GetDlgItem(IntPtr d, int id);
[DllImport("user32.dll")] public static extern IntPtr GetWindow(IntPtr h, uint cmd);
[DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr SendMessageW(IntPtr h, uint m, IntPtr w, string l);
[DllImport("user32.dll")] public static extern int GetDlgCtrlID(IntPtr h);
[DllImport("user32.dll")] public static extern IntPtr GetParent(IntPtr h);
public delegate bool EnumProc(IntPtr h, IntPtr l);
[DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr p, EnumProc f, IntPtr l);
[DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassNameW(IntPtr h, System.Text.StringBuilder s, int n);
public static IntPtr FileNameEdit(IntPtr dlg) {
    IntPtr found = IntPtr.Zero;
    EnumChildWindows(dlg, (h, l) => {
        var sb = new System.Text.StringBuilder(64); GetClassNameW(h, sb, 64);
        if (sb.ToString() == "Edit" && (GetDlgCtrlID(GetParent(h)) == 1148 || GetDlgCtrlID(h) == 1001 || GetDlgCtrlID(GetParent(h)) == 1001)) { found = h; return false; }
        return true;
    }, IntPtr.Zero);
    return found;
}
[StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
[StructLayout(LayoutKind.Sequential)] public struct POINT { public int X; public int Y; }
'@
[void][WSV.W]::SetProcessDpiAwarenessContext([IntPtr](-4))
$A = [System.Windows.Automation.AutomationElement]; $TS = [System.Windows.Automation.TreeScope]; $CT = [System.Windows.Automation.ControlType]
$checks = [System.Collections.Generic.List[object]]::new()
function Check([string]$what, [bool]$ok, [string]$detail = '') { $checks.Add([pscustomobject]@{ check = $what; pass = $ok; detail = $detail }) }
function Wait-Win([string]$cls) { for ($i = 0; $i -lt 80; $i++) { $h = [WSV.W]::FindWindowW($cls, [NullString]::Value); if ($h -ne [IntPtr]::Zero) { return $h }; Start-Sleep -Milliseconds 100 }; [IntPtr]::Zero }
function Root($h) { $A::FromHandle($h) }
function ById($root, [string]$id) { $root.FindFirst($TS::Descendants, (New-Object System.Windows.Automation.PropertyCondition($A::AutomationIdProperty, $id))) }
function Invoke-Id($root, [string]$id) { $e = ById $root $id; if (-not $e) { throw "no element $id" }; $e.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke(); Start-Sleep -Milliseconds 450 }
. (Join-Path $PSScriptRoot 'studio-nav.ps1')
function Select-Id($root, [string]$id) {
    if ($id -like 'nav:*') { $null = Open-StudioPage $id.Substring(4) { param($x) ById $root $x }; return }
    $e = ById $root $id; if (-not $e) { throw "no element $id" }; $e.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Select(); Start-Sleep -Milliseconds 450 }
function Toggle-Id($root, [string]$id) { (ById $root $id).GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Toggle(); Start-Sleep -Milliseconds 450 }
function Set-Id($root, [string]$id, [string]$v) { (ById $root $id).GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue($v); Start-Sleep -Milliseconds 450 }
function Status($root) { (ById $root 'status').Current.Name }
function Shot([IntPtr]$h, [string]$name) { $r = New-Object WSV.W+RECT; [void][WSV.W]::DwmGetWindowAttribute($h, 9, [ref]$r, 16); & "$ProjectRoot\experiments\ui-spike\target\release\shot.exe" (Join-Path $Out $name) ($r.L + 8) ($r.T + 8) ($r.R - $r.L - 16) ($r.B - $r.T - 16) 1 }
function Cfg { if (Test-Path $cfg) { Get-Content $cfg -Raw } else { '' } }
# Owned windows (message boxes, file dialogs) appear under their owner in the UI Automation tree.
function Find-Dialog([int]$procId) {
    $cond = New-Object System.Windows.Automation.AndCondition((New-Object System.Windows.Automation.PropertyCondition($A::ClassNameProperty, '#32770')), (New-Object System.Windows.Automation.PropertyCondition($A::ProcessIdProperty, $procId)))
    $h = [WSV.W]::FindWindowW('windowsRICE.Studio', [NullString]::Value)
    if ($h -ne [IntPtr]::Zero) { $d = $A::FromHandle($h).FindFirst($TS::Children, $cond); if ($d) { return $d } }
    $A::RootElement.FindFirst($TS::Children, $cond)
}
# Answers a modal dialog of process $procId by pressing the button with $buttonId.
function Answer-Dialog([int]$procId, [string]$buttonId) {
    # Message-box buttons are exposed to UI Automation as panes without an
    # Invoke pattern, so the script clicks the button by its dialog ID.
    for ($i = 0; $i -lt 40; $i++) {
        $d = Find-Dialog $procId
        if ($d) {
            $b = [WSV.W]::GetDlgItem([IntPtr]$d.Current.NativeWindowHandle, [int]$buttonId)
            if ($b -ne [IntPtr]::Zero) { [void][WSV.W]::PostMessageW($b, 0x00F5, [IntPtr]::Zero, [IntPtr]::Zero); Start-Sleep -Milliseconds 700; return $true }
        }
        Start-Sleep -Milliseconds 150
    }
    return $false
}
# Fills a file dialog's name box and presses its default button. The
# common item dialog draws with DirectUI; its name box (an Edit inside
# combo box 1148) and IDOK button are classic child windows.
function File-Dialog([int]$procId, [string]$path) {
    for ($i = 0; $i -lt 40; $i++) {
        $d = Find-Dialog $procId
        if ($d) {
            Start-Sleep -Milliseconds 800
            $dh = [IntPtr]$d.Current.NativeWindowHandle
            $edit = [WSV.W]::FileNameEdit($dh)
            if ($edit -eq [IntPtr]::Zero) { $script:dlgDebug = "no name box in dialog $dh"; return $false }
            [void][WSV.W]::SendMessageW($edit, 0x000C, [IntPtr]::Zero, $path)
            Start-Sleep -Milliseconds 300
            $ok = [WSV.W]::GetDlgItem($dh, 1)
            if ($ok -eq [IntPtr]::Zero) { return $false }
            [void][WSV.W]::PostMessageW($ok, 0x00F5, [IntPtr]::Zero, [IntPtr]::Zero)
            Start-Sleep -Milliseconds 1200
            return $true
        }
        Start-Sleep -Milliseconds 150
    }
    return $false
}
function Post-Mouse([IntPtr]$h, [uint32]$msg, $el, [double]$fx = 0.5, [double]$fy = 0.5) {
    $r = $el.Current.BoundingRectangle
    $pt = New-Object WSV.W+POINT; $pt.X = [int]($r.Left + $r.Width * $fx); $pt.Y = [int]($r.Top + $r.Height * $fy)
    [void][WSV.W]::ScreenToClient($h, [ref]$pt)
    $wp = if ($msg -eq 0x0201 -or $msg -eq 0x0200) { [IntPtr]1 } else { [IntPtr]0 }
    [void][WSV.W]::PostMessageW($h, $msg, $wp, [IntPtr](($pt.Y -shl 16) -bor ($pt.X -band 0xFFFF)))
    Start-Sleep -Milliseconds 120
}

# The bar can be translucent: keep a test backdrop behind it and capture
# the bar only when that backdrop is verifiably in place.
$bdProc = Start-Process powershell.exe -ArgumentList @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', (Join-Path $ProjectRoot 'scripts\backdrop-window.ps1'), '-Kind', 'checker', '-Seconds', '600', '-Height', '240') -PassThru -WindowStyle Hidden
for ($i = 0; $i -lt 100; $i++) { if ([WSV.W]::FindWindowW([NullString]::Value, 'windowsRICE test backdrop') -ne [IntPtr]::Zero) { break }; Start-Sleep -Milliseconds 100 }
function Bar-Shot([string]$name) {
    $bar = [WSV.W]::FindWindowW('windowsRICE.Panel', [NullString]::Value)
    $bdh = [WSV.W]::FindWindowW([NullString]::Value, 'windowsRICE test backdrop')
    $br = New-Object WSV.W+RECT; [void][WSV.W]::GetWindowRect($bdh, [ref]$br)
    $above = $false; $w = $bdh; for ($k = 0; $k -lt 500; $k++) { $w = [WSV.W]::GetWindow($w, 3); if ($w -eq [IntPtr]::Zero) { break }; if ($w -eq $bar) { $above = $true; break } }
    $r = New-Object WSV.W+RECT; [void][WSV.W]::GetWindowRect($bar, [ref]$r)
    if ($bdh -eq [IntPtr]::Zero -or $br.T -ne 0 -or $br.B -lt $r.B -or -not $above) { throw "backdrop not under the bar; refusing to capture ($name)" }
    $p = Join-Path $Out $name
    & "$ProjectRoot\experiments\ui-spike\target\release\shot.exe" $p $r.L $r.T ($r.R - $r.L) ($r.B - $r.T) 1 | Out-Null
    $p
}
# Scrolls the Studio section until an element is in the tree.
function Reveal([string]$id) {
    $sh2 = [WSV.W]::FindWindowW('windowsRICE.Studio', [NullString]::Value)
    for ($k = 0; $k -lt 30; $k++) {
        $e = ById (Root $sh2) $id; if ($e) { return $e }
        $r = New-Object WSV.W+RECT; [void][WSV.W]::GetWindowRect($sh2, [ref]$r)
        $x = [int](($r.L + $r.R) / 2) - 200; $y = [int](($r.T + $r.B) / 2) + 200
        [void][WSV.W]::PostMessageW($sh2, 0x020A, [IntPtr]([int](-120 -band 0xFFFF) -shl 16), [IntPtr](($y -shl 16) -bor ($x -band 0xFFFF)))
        Start-Sleep -Milliseconds 250
    }
    throw "could not reveal $id"
}
function Select-El($e) { $e.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Select(); Start-Sleep -Milliseconds 450 }
function Toggle-El($e) { $e.GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Toggle(); Start-Sleep -Milliseconds 450 }
# ================================================================ start with --studio
$stats = Join-Path $Out 'run.json'
$p = Start-Process $Exe -ArgumentList @('--studio', '--config', $cfg, '--stats', $stats, '--no-hotkey', '--lang', 'en', '--exit-after', '400', '--title', '"Documents - File Explorer"') -PassThru
$sh = Wait-Win 'windowsRICE.Studio'
Check 'Design Studio window opens (--studio)' ($sh -ne [IntPtr]::Zero)
Start-Sleep -Seconds 3
$s = Root $sh
Check 'Studio root is a Window named "windowsRICE Design Studio"' (($s.Current.ControlType -eq $CT::Window) -and ($s.Current.Name -eq 'windowsRICE Design Studio')) $s.Current.Name
$navs = @($s.FindAll($TS::Descendants, (New-Object System.Windows.Automation.PropertyCondition($A::ControlTypeProperty, $CT::ListItem))) | Where-Object { $_.Current.AutomationId -like 'group:*' })
Check 'six navigation groups, Templates first' (($navs.Count -eq 6) -and ($navs[0].Current.Name -eq 'Templates')) (($navs | ForEach-Object { $_.Current.Name }) -join ', ')
Check 'preview summary names the shown modules' ((ById $s 'preview:summary').Current.Name -like 'Preview shows * modules:*') (ById $s 'preview:summary').Current.Name
$all = $s.FindAll($TS::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
Check 'every Studio element is named' (@($all | Where-Object { [string]::IsNullOrWhiteSpace($_.Current.Name) }).Count -eq 0)
foreach ($sec in $StudioPages) {
    Select-Id $s "nav:$sec"
    $s = Root $sh
    $ok = Test-StudioPageSelected $sec { param($x) ById $s $x }
    Shot $sh "section-$sec.png"
    Check "section $sec opens" $ok
}

# ---- Colours: swatch, typed colour, contrast block, undo/redo, apply
Select-Id $s 'nav:background'; $s = Root $sh
Select-El (Reveal 'card:background:0'); $s = Root $sh    # solid, so the bar pixel is exact
Select-Id $s 'nav:colors'; $s = Root $sh
Select-El (Reveal 'role:0'); $s = Root $sh         # background role
Select-Id $s 'color:sw:1'; $s = Root $sh             # Midnight #121826
Check 'background swatch changes the draft (Apply enabled)' ((ById $s 'apply').Current.IsEnabled -and (Status $s) -match 'not applied') (Status $s)
Select-El (Reveal 'role:2'); $s = Root $sh                 # accent role
Set-Id $s 'field:color-hex' '#E3A33B'; $s = Root $sh
Check 'typed accent colour accepted (Value pattern)' ((ById $s 'field:color-hex').GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).Current.Value -eq '#E3A33B')
Set-Id $s 'field:color-hex' 'orange'; $s = Root $sh
Check 'invalid colour explained' ((Status $s) -match '#RRGGBB') (Status $s)
Select-El (Reveal 'role:3'); $s = Root $sh                 # text role
Set-Id $s 'field:color-hex' '#141A26'; $s = Root $sh
# Phase 12: an unreadable text colour is drawn as the nearest readable
# colour, and the correction is reported next to the colour editor.
$rd = ById $s 'color:readability'
Check 'unreadable text colour is corrected and the correction is reported' ($rd -and ($rd.Current.Name -match '^Readability: text #141A26 is shown as #')) $(if ($rd) { $rd.Current.Name } else { 'no notice' })
Invoke-Id $s 'undo'; $s = Root $sh
Check 'undo removes the unreadable colour' ((ById $s 'apply').Current.IsEnabled)
Invoke-Id $s 'redo'; $s = Root $sh; Invoke-Id $s 'undo'; $s = Root $sh
Check 'redo and undo again' ((ById $s 'apply').Current.IsEnabled)
Select-Id $s 'nav:background'; $s = Root $sh
Toggle-El (Reveal 'bg:border'); $s = Root $sh
Shot $sh 'colours-edited.png'
Invoke-Id $s 'apply'; Start-Sleep -Milliseconds 900; $s = Root $sh
$c = Cfg
Check 'Apply saves to config.toml' (($c -match 'background = "#121826"') -and ($c -match 'accent = "#E3A33B"') -and ($c -match 'border_bottom = true')) (($c -split "`n" | Where-Object { $_ -match 'background|accent|border_bottom' }) -join ' ')
Check 'status reports the bar was verified' ((Status $s) -match 'Verified on') (Status $s)
Add-Type -AssemblyName System.Drawing
$bp = Bar-Shot 'bar-after-apply.png'
$bmp = [System.Drawing.Bitmap]::FromFile($bp); $px = $bmp.GetPixel([int]($bmp.Width * 0.38), [int]($bmp.Height / 2)); $bmp.Dispose()
Check 'the real bar shows the applied background' (('#{0:X2}{1:X2}{2:X2}' -f $px.R, $px.G, $px.B) -eq '#121826') ('#{0:X2}{1:X2}{2:X2}' -f $px.R, $px.G, $px.B)

# ---- Modules: drag and drop (mouse messages to the Studio window) and keyboard alternatives
Select-Id $s 'nav:modules'; $s = Root $sh
$card = Reveal 'card:cpu'; $s = Root $sh; $col = ById $s 'col:0'
Post-Mouse $sh 0x0201 $card
Post-Mouse $sh 0x0200 $card 0.6 0.9
Post-Mouse $sh 0x0200 $col 0.5 0.5
Post-Mouse $sh 0x0200 $col 0.5 0.9
Post-Mouse $sh 0x0202 $col 0.5 0.9
Start-Sleep -Milliseconds 400; $s = Root $sh
Check 'drag and drop moves CPU into the left section' ((ById $s 'card:cpu').Current.Name -like 'CPU usage, Left section*') (ById $s 'card:cpu').Current.Name
Select-Id $s 'card:cpu'; $s = Root $sh
Invoke-Id $s 'props:earlier'; $s = Root $sh
Check 'keyboard alternative: Earlier moves the selected card' ((ById $s 'card:cpu').Current.Name -like '*position 2 of 3*') (ById $s 'card:cpu').Current.Name
Select-Id $s 'props:region:2'; $s = Root $sh
Check 'keyboard alternative: region choice moves it to the right' ((Reveal 'card:cpu').Current.Name -like '*Right section*')
(Reveal 'add:CustomText').GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke(); Start-Sleep -Milliseconds 500; $s = Root $sh
Check 'add a custom text module' ($null -ne (Reveal 'card:text-2'))
Set-Id $s 'field:module-text' '안녕 windowsRICE'; $s = Root $sh
Invoke-Id $s 'props:remove'; $s = Root $sh
Check 'remove the module again' ($null -eq (ById $s 'card:text-2'))
Shot $sh 'modules-edited.png'

# ---- Themes: duplicate, edit, save, delete (confirmation)
Select-Id $s 'nav:themes'; $s = Root $sh
$tc = @((Root $sh).FindAll($TS::Descendants, (New-Object System.Windows.Automation.PropertyCondition($A::ControlTypeProperty, $CT::ListItem))) | Where-Object { $_.Current.AutomationId -like 'theme:*' -and $_.Current.Name -like 'windowsRICE Default.*' })[0]
$tc.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Select(); Start-Sleep -Milliseconds 500; $s = Root $sh
Invoke-Id $s 'props:dup'; Start-Sleep -Milliseconds 600; $s = Root $sh
$themeFile = @(Get-ChildItem (Join-Path $Out 'themes') -Filter 'custom-*.toml' -ErrorAction SilentlyContinue)
Check 'duplicate writes a custom theme file' ($themeFile.Count -eq 1) ($themeFile.Name -join ',')
Set-Id $s 'field:theme-name' 'Studio Teal'; $s = Root $sh
Set-Id $s 'field:theme-accent' '#1FA39A'; $s = Root $sh
Invoke-Id $s 'props:tsave'; Start-Sleep -Milliseconds 600; $s = Root $sh
$tt = if ($themeFile.Count) { Get-Content $themeFile[0].FullName -Raw } else { '' }
Check 'edited theme saved and validated' (($tt -match 'name = "Studio Teal"') -and ($tt -match '#1FA39A')) (Status $s)
Shot $sh 'theme-studio.png'
Invoke-Id $s 'props:tdelete'
$answered = Answer-Dialog $p.Id '6'   # IDYES
Start-Sleep -Milliseconds 600; $s = Root $sh
Check 'delete asks first and removes the theme file' ($answered -and -not (Test-Path $themeFile[0].FullName)) (Status $s)

# ---- Presets: save, load, export, import (valid and malformed)
Select-Id $s 'nav:presets'; $s = Root $sh
Set-Id $s 'field:preset-name' 'Evening Test'; $s = Root $sh
Invoke-Id $s 'props:psave'; Start-Sleep -Milliseconds 400; $s = Root $sh
$pf = Join-Path $Out 'presets\evening-test.toml'
Check 'save current design as a preset' (Test-Path $pf) (Status $s)
Select-Id $s 'pre:0'; $s = Root $sh
Invoke-Id $s 'props:pload'; $s = Root $sh
Check 'load preset into the draft' ((Status $s) -match 'Loaded') (Status $s)
$exp = Join-Path $Out 'exported.toml'
Invoke-Id $s 'props:pexport'
$okExp = File-Dialog $p.Id $exp
Start-Sleep -Milliseconds 500
Check 'export through the Save dialog' ($okExp -and (Test-Path $exp) -and ((Get-Content $exp -Raw) -match 'format = "windowsrice-preset"')) "dialog=$okExp status=$expStatus"
$bad = Join-Path $Out 'malformed.toml'; Set-Content $bad 'format = "windowsrice-preset"`nversion = 1`nname = 7' -Encoding utf8
Invoke-Id (Root $sh) 'props:pimport'
$okBad = File-Dialog $p.Id $bad
Start-Sleep -Milliseconds 500; $s = Root $sh
Check 'malformed preset rejected with a reason' ($okBad -and (Status $s) -match 'Not imported') (Status $s)
$good = Join-Path $Out 'to-import.toml'; $src = if (Test-Path $exp) { $exp } else { $pf }; (Get-Content $src -Raw).Replace('name = "Evening Test"', 'name = "Imported One"') | Set-Content $good -Encoding utf8
Invoke-Id $s 'props:pimport'
$okGood = File-Dialog $p.Id $good
Start-Sleep -Milliseconds 500; $s = Root $sh
Check 'valid preset imported into the presets folder' ($okGood -and (Test-Path (Join-Path $Out 'presets\imported-one.toml'))) (Status $s)

# ---- Keyboard (keys posted to the Studio's own window): grid arrows,
# slider arrows, and caret editing in a text field.
function Key([int]$vk) { [void][WSV.W]::PostMessageW($sh, 0x0100, [IntPtr]$vk, [IntPtr]0); Start-Sleep -Milliseconds 350 }
function Chr([char]$c) { [void][WSV.W]::PostMessageW($sh, 0x0102, [IntPtr][int]$c, [IntPtr]0); Start-Sleep -Milliseconds 200 }
Select-Id (Root $sh) 'nav:styles'; $s = Root $sh
(ById $s 'card:style:0').SetFocus(); Start-Sleep -Milliseconds 400
Key 0x27
Check 'Right arrow moves to the next style card' ((ById (Root $sh) 'card:style:1').Current.HasKeyboardFocus)
Key 0x28
$down = @((Root $sh).FindAll($TS::Descendants, [System.Windows.Automation.Condition]::TrueCondition) | Where-Object { $_.Current.HasKeyboardFocus -and $_.Current.AutomationId -like 'card:style:*' })
Check 'Down arrow moves one row down the grid' ($down.Count -eq 1 -and $down[0].Current.AutomationId -ne 'card:style:1') ($down | ForEach-Object { $_.Current.AutomationId })
Key 0x20
Check 'Space chooses the focused card' (@((Root $sh).FindAll($TS::Descendants, [System.Windows.Automation.Condition]::TrueCondition) | Where-Object { $_.Current.AutomationId -like 'card:style:*' -and $_.Current.HasKeyboardFocus -and $_.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Current.IsSelected }).Count -eq 1)
Invoke-Id (Root $sh) 'undo'
Select-Id (Root $sh) 'nav:layout'; $s = Root $sh
$sl = Reveal 'slider:Height'; $v0 = $sl.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).Current.Value
$sl.SetFocus(); Start-Sleep -Milliseconds 300
Key 0x27
$v1 = (Reveal 'slider:Height').GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).Current.Value
Check 'Right arrow raises a slider by one step' ([int]$v1 -eq [int]$v0 + 1) "$v0 -> $v1"
Invoke-Id (Root $sh) 'undo'
Select-Id (Root $sh) 'nav:presets'; $s = Root $sh
(ById $s 'field:preset-name').SetFocus(); Start-Sleep -Milliseconds 300
Key 0x0D; Chr 'A'; Chr 'b'; Key 0x24; Chr 'X'; Key 0x23; Key 0x25; Key 0x2E; Key 0x0D
$pv = (ById (Root $sh) 'field:preset-name').GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).Current.Value
Check 'text field: typing, Home, End, Left and Delete edit at the caret' ($pv -eq 'XA') $pv

# ---- Reset (confirmation), unsaved close (cancel, then discard)
Select-Id $s 'nav:advanced'; $s = Root $sh
Invoke-Id $s 'adv:reset'
$okReset = Answer-Dialog $p.Id '6'
$s = Root $sh
Check 'reset asks first and resets the draft' ($okReset -and (Status $s) -match 'reset') (Status $s)
[void][WSV.W]::PostMessageW($sh, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)
$okCancel = Answer-Dialog $p.Id '2'   # IDCANCEL
Check 'closing with unapplied changes asks; Cancel keeps the Studio open' ($okCancel -and ([WSV.W]::FindWindowW('windowsRICE.Studio', [NullString]::Value) -ne [IntPtr]::Zero))
[void][WSV.W]::PostMessageW($sh, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)
$okNo = Answer-Dialog $p.Id '7'   # IDNO
Start-Sleep -Milliseconds 600
Check 'No discards and closes; the bar keeps running' ($okNo -and ([WSV.W]::FindWindowW('windowsRICE.Studio', [NullString]::Value) -eq [IntPtr]::Zero) -and -not $p.HasExited)
$st = Join-Path $Out 'studio.toml'
Check 'Studio remembers its section and window' ((Test-Path $st) -and ((Get-Content $st -Raw) -match 'section = "advanced"'))
Check 'reset was not applied (config keeps the applied design)' ((Cfg) -match 'background = "#121826"')

# ---- second launch opens the Studio in the running bar
$p2 = Start-Process $Exe -ArgumentList @('--studio', '--config', $cfg) -PassThru
$null = $p2.WaitForExit(10000)
$sh2 = Wait-Win 'windowsRICE.Studio'
Check 'a second --studio launch opens the Studio in the running bar' (($p2.ExitCode -eq 0) -and ($sh2 -ne [IntPtr]::Zero))

$bar = [WSV.W]::FindWindowW('windowsRICE.Panel', [NullString]::Value)
[void][WSV.W]::PostMessageW($bar, 0x0104, [IntPtr]0x73, [IntPtr]([long]1 -bor (0x3E -shl 16) -bor (1L -shl 29)))
$null = $p.WaitForExit(15000)
$j = if (Test-Path $stats) { Get-Content $stats -Raw | ConvertFrom-Json } else { [pscustomobject]@{ errors = @("no stats file (exited=$($p.HasExited))"); a11y_problems = @() } }
Check 'clean exit, no errors, no accessibility lint problems' (($p.ExitCode -eq 0) -and (@($j.errors).Count -eq 0) -and (@($j.a11y_problems).Count -eq 0)) "errors=$($j.errors -join '; ') a11y=$($j.a11y_problems -join '; ')"

$checks | Format-Table -AutoSize -Wrap | Out-String -Width 230
$checks | ConvertTo-Json | Set-Content (Join-Path $Out 'checks.json') -Encoding utf8
$failed = @($checks | Where-Object { -not $_.pass }).Count
"{0} checks, {1} failed" -f $checks.Count, $failed
Stop-Process -Id $bdProc.Id -Force -ErrorAction SilentlyContinue
exit [int]($failed -gt 0)
