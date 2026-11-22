# Phase 14: a background image from the design library is drawn on the
# running bar (isolated configuration). A generated test image (left half
# red, right half blue) is placed in the test library; the bar is captured
# only over the pinned test backdrop. Results: target\phase14\media\checks.json.
param(
    [string]$ProjectRoot = (Split-Path $PSScriptRoot -Parent),
    [string]$Exe = 'target\release\windowsrice-panel.exe'
)
$ErrorActionPreference = 'Stop'
Set-Location $ProjectRoot
if (@(Get-Process windowsrice-panel -ErrorAction SilentlyContinue).Count -gt 0) { throw 'a windowsRICE bar is running; close it first' }
$Out = Join-Path $ProjectRoot 'target\phase14\media'
if (Test-Path $Out) { Remove-Item -Recurse -Force $Out }
New-Item -ItemType Directory -Force (Join-Path $Out 'library') | Out-Null
Add-Type -AssemblyName System.Drawing
$bmp = New-Object System.Drawing.Bitmap 400, 40
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.FillRectangle([System.Drawing.Brushes]::Red, 0, 0, 200, 40)
$g.FillRectangle([System.Drawing.Brushes]::Blue, 200, 0, 200, 40)
$g.Dispose()
$bmp.Save((Join-Path $Out 'library\img-test.png'), [System.Drawing.Imaging.ImageFormat]::Png); $bmp.Dispose()
$cfg = Join-Path $Out 'config.toml'
@'
schema_version = 3

[bar]
modules = [{ id = "menu", kind = "menu", region = "left" }]

[bar.style]
material = "solid"
background_asset = "img-test"
'@ | Set-Content $cfg -Encoding utf8
$checks = [System.Collections.Generic.List[object]]::new()
function Check([string]$what, [bool]$ok, [string]$detail = '') { $checks.Add([pscustomobject]@{ check = $what; pass = $ok; detail = $detail }); '{0,-56} {1} {2}' -f $what, $ok, $detail }
Add-Type -Namespace MV -Name W -MemberDefinition @'
[DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindowW(string c, string t);
[DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
[DllImport("user32.dll")] public static extern bool SetProcessDpiAwarenessContext(IntPtr v);
[StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
'@
[void][MV.W]::SetProcessDpiAwarenessContext([IntPtr](-4))
$bd = Start-Process powershell.exe -ArgumentList @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', (Join-Path $PSScriptRoot 'backdrop-window.ps1'), '-Kind', 'checker', '-Seconds', '60', '-Height', '200') -PassThru -WindowStyle Hidden
for ($i = 0; $i -lt 60 -and [MV.W]::FindWindowW([NullString]::Value, 'windowsRICE test backdrop') -eq [IntPtr]::Zero; $i++) { Start-Sleep -Milliseconds 150 }
$stats = Join-Path $Out 'run.json'
$p = Start-Process $Exe -ArgumentList @('--config', $cfg, '--stats', $stats, '--no-hotkey', '--exit-after', '12', '--title', '"Media test"', '--monitor', '0') -PassThru
Start-Sleep -Seconds 5
$bar = [MV.W]::FindWindowW('windowsRICE.Panel', [NullString]::Value)
$r = New-Object MV.W+RECT; [void][MV.W]::GetWindowRect($bar, [ref]$r)
$bdh = [MV.W]::FindWindowW([NullString]::Value, 'windowsRICE test backdrop')
if ($bdh -eq [IntPtr]::Zero) { throw 'test backdrop not in place; refusing to capture' }
$png = Join-Path $Out 'bar.png'
& (Join-Path $ProjectRoot 'experiments\ui-spike\target\release\shot.exe') $png $r.L $r.T ($r.R - $r.L) ($r.B - $r.T) 1 | Out-Null
$img = [System.Drawing.Bitmap]::FromFile($png)
$w = $img.Width; $h = $img.Height
$left = $img.GetPixel([int]($w * 0.30), [int]($h * 0.85)); $right = $img.GetPixel([int]($w * 0.75), [int]($h * 0.85))
$img.Dispose()
Check 'the left half of the bar shows the image (red)' (($left.R -gt 180) -and ($left.G -lt 80) -and ($left.B -lt 80)) ('#{0:X2}{1:X2}{2:X2}' -f $left.R, $left.G, $left.B)
Check 'the right half of the bar shows the image (blue)' (($right.B -gt 180) -and ($right.R -lt 80)) ('#{0:X2}{1:X2}{2:X2}' -f $right.R, $right.G, $right.B)
$null = $p.WaitForExit(20000)
Stop-Process -Id $bd.Id -Force -ErrorAction SilentlyContinue
$j = Get-Content $stats -Raw | ConvertFrom-Json
Check 'no errors during the run' (@($j.errors).Count -eq 0) (@($j.errors) -join '; ')
$checks | ConvertTo-Json | Set-Content (Join-Path $Out 'checks.json') -Encoding utf8
$failed = @($checks | Where-Object { -not $_.pass }).Count
"{0} checks, {1} failed" -f $checks.Count, $failed
exit [int]($failed -gt 0)
