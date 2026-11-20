# windowsRICE Phase 7 diagnostic (kept as evidence): does each Design Studio
# change visibly reach the real bar? Written against the Phase 6 Studio and
# its element ids (bd:*, font:N); superseded by scripts/design-e2e.ps1.
# It captures the bar without a test backdrop, which was safe only while
# the bar was opaque; do not run it against Phase 7 or later builds.
#
#
# For every probe step, the script changes one property in the running
# Studio through UI Automation, presses Apply, captures the real bar from
# the screen and compares the capture with the one before. A step whose
# configuration changed but whose bar pixels did not is reported as
# NOT VISIBLE. Uses a project-local configuration. Windows PowerShell 5.1.

param(
    [string]$ProjectRoot = (Split-Path $PSScriptRoot -Parent),
    [string]$Out = (Join-Path (Split-Path $PSScriptRoot -Parent) 'target\apply-probe'),
    [string]$Exe = 'target\release\windowsrice-panel.exe',
    [string]$Lang = 'en'
)
$ErrorActionPreference = 'Stop'
Set-Location $ProjectRoot
if (Test-Path $Out) { Remove-Item -Recurse -Force $Out }
New-Item -ItemType Directory -Force $Out | Out-Null
$cfg = Join-Path $Out 'config.toml'
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes, System.Drawing
Add-Type -Namespace WAP -Name W -MemberDefinition @'
[DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindowW(string cls, string title);
[DllImport("user32.dll")] public static extern bool SetProcessDpiAwarenessContext(IntPtr v);
[DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
[DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr hWnd, uint msg, IntPtr wParam, IntPtr lParam);
[StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
'@
[void][WAP.W]::SetProcessDpiAwarenessContext([IntPtr](-4))
$A = [System.Windows.Automation.AutomationElement]; $TS = [System.Windows.Automation.TreeScope]
function Wait-Win([string]$cls) { for ($i = 0; $i -lt 80; $i++) { $h = [WAP.W]::FindWindowW($cls, [NullString]::Value); if ($h -ne [IntPtr]::Zero) { return $h }; Start-Sleep -Milliseconds 100 }; [IntPtr]::Zero }
function ById([string]$id) { $script:root = $A::FromHandle($script:sh); $script:root.FindFirst($TS::Descendants, (New-Object System.Windows.Automation.PropertyCondition($A::AutomationIdProperty, $id))) }
function Do-Invoke([string]$id) { $e = ById $id; if (-not $e) { throw "no element $id" }; $e.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke(); Start-Sleep -Milliseconds 350 }
function Do-Select([string]$id) { $e = ById $id; if (-not $e) { throw "no element $id" }; $e.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Select(); Start-Sleep -Milliseconds 350 }
function Do-Toggle([string]$id) { (ById $id).GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Toggle(); Start-Sleep -Milliseconds 350 }
function Do-Set([string]$id, [string]$v) { (ById $id).GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue($v); Start-Sleep -Milliseconds 350 }
function Bar-Shot([string]$name) {
    $b = [WAP.W]::FindWindowW('windowsRICE.Panel', [NullString]::Value)
    $r = New-Object WAP.W+RECT; [void][WAP.W]::GetWindowRect($b, [ref]$r)
    $p = Join-Path $Out "$name.png"
    & "$ProjectRoot\experiments\ui-spike\target\release\shot.exe" $p $r.L $r.T ($r.R - $r.L) ($r.B - $r.T) 1 | Out-Null
    $bmp = [System.Drawing.Bitmap]::FromFile($p)
    # Signature: background sampled in empty areas + a coarse hash of the whole strip.
    $sig = New-Object System.Text.StringBuilder
    for ($x = 0; $x -lt $bmp.Width; $x += 7) { for ($y = 1; $y -lt $bmp.Height; $y += 5) { $c = $bmp.GetPixel($x, $y); [void]$sig.Append($c.R).Append(',').Append($c.G).Append(',').Append($c.B).Append(';') } }
    $bg = $bmp.GetPixel([int]($bmp.Width * 0.30), [int]($bmp.Height / 2))
    $h = $bmp.Height; $bmp.Dispose()
    $md5 = [System.BitConverter]::ToString([System.Security.Cryptography.MD5]::Create().ComputeHash([System.Text.Encoding]::ASCII.GetBytes($sig.ToString()))).Replace('-', '').Substring(0, 12)
    [pscustomobject]@{ hash = $md5; bg = ('#{0:X2}{1:X2}{2:X2}' -f $bg.R, $bg.G, $bg.B); height = $h }
}
$results = [System.Collections.Generic.List[object]]::new()
function Probe([string]$name, [scriptblock]$change) {
    $before = Get-Content $cfg -Raw -ErrorAction SilentlyContinue
    $shotBefore = Bar-Shot "$($results.Count)-a-$name"
    try { & $change } catch { $results.Add([pscustomobject]@{ step = $name; config = '?'; bar = 'ERROR'; detail = "$_" }); return }
    if ((ById 'apply').Current.IsEnabled) { Do-Invoke 'apply' } else { $results.Add([pscustomobject]@{ step = $name; config = 'same'; bar = 'no draft change'; detail = (ById 'status').Current.Name }); return }
    Start-Sleep -Milliseconds 900
    $status = (ById 'status').Current.Name
    $after = Get-Content $cfg -Raw -ErrorAction SilentlyContinue
    $shotAfter = Bar-Shot "$($results.Count)-b-$name"
    $cfgChanged = $before -ne $after
    $visible = $shotBefore.hash -ne $shotAfter.hash
    $verdict = if ($visible) { 'visible' } elseif ($cfgChanged) { 'NOT VISIBLE' } else { 'no change' }
    $results.Add([pscustomobject]@{ step = $name; config = $(if ($cfgChanged) { 'changed' } else { 'same' }); bar = $verdict; detail = "bg $($shotBefore.bg)->$($shotAfter.bg) h $($shotBefore.height)->$($shotAfter.height) | $status" })
}

$p = Start-Process $Exe -ArgumentList @('--studio', '--config', $cfg, '--no-hotkey', '--lang', $Lang, '--exit-after', '600', '--title', '"Documents - File Explorer"') -PassThru
$script:sh = Wait-Win 'windowsRICE.Studio'
Start-Sleep -Seconds 3

Do-Select 'nav:themes'
foreach ($t in 'windowsrice-glass', 'windowsrice-ambient', 'windowsrice-light', 'windowsrice-amoled', 'windowsrice-hypr', 'windowsrice-default') {
    Probe "theme $t" { Do-Select "theme:$t"; Do-Invoke 'props:use' }
}
Do-Select 'nav:bar-design'
Probe 'mode light' { Do-Select 'bd:mode:2' }
Probe 'mode dark' { Do-Select 'bd:mode:1' }
Probe 'background swatch' { Do-Select 'bd:bg:3' }
Probe 'accent swatch' { Do-Select 'bd:accent:2' }
Probe 'text colour' { Do-Set 'field:text' '#F5D78E' }
Probe 'opacity down x3' { Do-Invoke 'bd:opacity:dec'; Do-Invoke 'bd:opacity:dec'; Do-Invoke 'bd:opacity:dec' }
Probe 'bottom border' { Do-Toggle 'bd:border' }
Probe 'separators' { Do-Toggle 'bd:separators' }
Do-Select 'nav:typography'
Probe 'font family' { $f = ById 'font:3'; if (-not $f) { throw 'no font:3' }; $f.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Select(); Start-Sleep -Milliseconds 350 }
Probe 'font size up x3' { Do-Invoke 'font:size:inc'; Do-Invoke 'font:size:inc'; Do-Invoke 'font:size:inc' }
Probe 'font weight 500' { Do-Select 'font:weight:1' }
Probe 'labels off' { Do-Toggle 'font:labels' }
Do-Select 'nav:layout'
Probe 'height up x4' { 1..4 | ForEach-Object { Do-Invoke 'lay:height:inc' } }
Probe 'padding up x4' { 1..4 | ForEach-Object { Do-Invoke 'lay:padding:inc' } }
Probe 'spacing up x4' { 1..4 | ForEach-Object { Do-Invoke 'lay:spacing:inc' } }
Probe 'valign top' { Do-Select 'lay:valign:1' }

$results | Format-Table -AutoSize -Wrap | Out-String -Width 260
$results | ConvertTo-Json | Set-Content (Join-Path $Out 'probe.json') -Encoding utf8
Copy-Item $cfg (Join-Path $Out 'final-config.toml') -ErrorAction SilentlyContinue
Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
