# windowsRICE: remaining Phase 4 / Phase 5 checks that this machine can run
# without changing Windows settings or injecting global input.
#
#   1. Hide/resume (Gaming Mode path): no frames while hidden, frames after.
#   2. Alt+F4 delivered to the bar's own window (WM_SYSKEYDOWN VK_F4 with
#      the Alt context bit, as Windows sends it): the bar closes, writes its
#      diagnostics and releases the reserved work area. SIMULATED: the key
#      press is posted, not typed.
#   3. Real overflow: a configuration with long custom-text modules on the
#      real monitor (full width); modules must be hidden and named.
#   4. Light, Dark and Glass over black, white and colourful backdrops: the
#      backdrop is a topmost window of this script under the bar (the
#      wallpaper is never changed); text contrast is measured on the
#      composited screen pixels.
# Uses project-local config files only. Run with PowerShell 7 (pwsh).

param(
    [string]$ProjectRoot = (Split-Path $PSScriptRoot -Parent),
    [string]$Out = (Join-Path (Split-Path $PSScriptRoot -Parent) 'target\phase45-verify'),
    [string]$Exe = 'target\release\windowsrice-panel.exe'
)
$ErrorActionPreference = 'Stop'
Set-Location $ProjectRoot
New-Item -ItemType Directory -Force $Out | Out-Null
Add-Type -AssemblyName System.Drawing

# Primary monitor geometry (physical pixels) and scale, so the checks work on
# any screen (CI runners included), not only the 3840 px / 150 % original.
Add-Type -Namespace WScr -Name S -MemberDefinition '[DllImport("user32.dll")] public static extern int GetSystemMetrics(int i); [DllImport("user32.dll")] public static extern uint GetDpiForSystem();'
$ScreenW = [WScr.S]::GetSystemMetrics(0)
$Scale = [WScr.S]::GetDpiForSystem() / 96.0
function Px([double]$dip) { [int][math]::Round($dip * $Scale) }
Add-Type -Namespace WV -Name Win -MemberDefinition @'
[DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr hWnd, uint msg, IntPtr wParam, IntPtr lParam);
[DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindowW(string cls, string title);
[DllImport("user32.dll")] public static extern bool SetProcessDpiAwarenessContext(IntPtr v);
[DllImport("user32.dll")] public static extern IntPtr MonitorFromPoint(POINT pt, uint flags);
[DllImport("user32.dll")] public static extern bool GetMonitorInfoW(IntPtr h, ref MONITORINFO mi);
[StructLayout(LayoutKind.Sequential)] public struct POINT { public int X; public int Y; }
[StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
[StructLayout(LayoutKind.Sequential)] public struct MONITORINFO { public int cbSize; public RECT rcMonitor; public RECT rcWork; public uint dwFlags; }
public static int WorkTop() { var mi = new MONITORINFO(); mi.cbSize = Marshal.SizeOf(mi); GetMonitorInfoW(MonitorFromPoint(new POINT(), 1), ref mi); return mi.rcWork.Top; }
'@
[void][WV.Win]::SetProcessDpiAwarenessContext([IntPtr](-4))
# The bar can be translucent, so whatever is behind it shows through. A
# pinned test backdrop (scripts/backdrop-window.ps1) stays behind it for the
# whole run, and no screenshot is taken unless that backdrop is in place.
Add-Type -Namespace WGuard -Name G -MemberDefinition '[DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindowW(string c, string t); [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r); [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }'
$guardProc = Start-Process powershell.exe -ArgumentList @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', (Join-Path $ProjectRoot 'scripts\backdrop-window.ps1'), '-Kind', 'checker', '-Seconds', '1200', '-Height', '240') -PassThru -WindowStyle Hidden
for ($gi = 0; $gi -lt 100; $gi++) { if ([WGuard.G]::FindWindowW([NullString]::Value, 'windowsRICE test backdrop') -ne [IntPtr]::Zero) { break }; Start-Sleep -Milliseconds 100 }
function Assert-Backdrop { $gh = [WGuard.G]::FindWindowW([NullString]::Value, 'windowsRICE test backdrop'); $gr = New-Object WGuard.G+RECT; [void][WGuard.G]::GetWindowRect($gh, [ref]$gr); if ($gh -eq [IntPtr]::Zero -or $gr.T -ne 0) { throw 'test backdrop not in place; refusing to capture' } }

$shot = Join-Path $ProjectRoot 'experiments\ui-spike\target\release\shot.exe'
$checks = [System.Collections.Generic.List[object]]::new()
function Check([string]$what, [bool]$ok, [string]$detail = '') { $checks.Add([pscustomobject]@{ check = $what; pass = $ok; detail = $detail }) }
function Wait-Bar { for ($i = 0; $i -lt 60; $i++) { $h = [WV.Win]::FindWindowW('windowsRICE.Panel', [NullString]::Value); if ($h -ne [IntPtr]::Zero) { return $h }; Start-Sleep -Milliseconds 100 }; [IntPtr]::Zero }
function Bar([string]$tag, [string[]]$extra, [string]$configText = $null) {
    $cfg = Join-Path $Out "cfg-$tag.toml"
    Remove-Item "$cfg*" -ErrorAction SilentlyContinue
    if ($configText) { [IO.File]::WriteAllText($cfg, $configText) }
    Start-Process $Exe -ArgumentList (@('--config', $cfg, '--no-hotkey', '--lang', 'en', '--stats', (Join-Path $Out "$tag.json"), '--title', '"Documents - File Explorer"') + $extra) -PassThru
}
function Stats([string]$tag) { Get-Content (Join-Path $Out "$tag.json") -Raw | ConvertFrom-Json }
$work0 = [WV.Win]::WorkTop()

# ---- 1. hide / resume
$p = Bar 'suspend' @('--exit-after', '12', '--suspend-test', '3', '8')
$null = $p.WaitForExit(30000)
$j = Stats 'suspend'
$hidden = @($j.frames | Where-Object { $_ -gt 3.2 -and $_ -lt 7.9 }).Count
$after = @($j.frames | Where-Object { $_ -ge 8 }).Count
Check 'hide/resume: no frames while hidden, frames after' (($hidden -eq 0) -and ($after -ge 1) -and (@($j.suspend_log).Count -eq 2)) "hidden=$hidden after=$after log=$($j.suspend_log -join ' | ')"

# ---- 2. Alt+F4 (posted to the bar's own window)
$p = Bar 'altf4' @('--exit-after', '30')
$h = Wait-Bar; Start-Sleep -Seconds 2
$wt = [WV.Win]::WorkTop()
$lp = [IntPtr]([long]1 -bor (0x3E -shl 16) -bor (1L -shl 29))
[void][WV.Win]::PostMessageW($h, 0x0104, [IntPtr]0x73, $lp)   # WM_SYSKEYDOWN VK_F4, Alt held
$exited = $p.WaitForExit(5000)
Start-Sleep -Milliseconds 500
$j = Stats 'altf4'
Check 'Alt+F4 (posted): bar closes cleanly and releases the work area' ($exited -and $p.ExitCode -eq 0 -and $j.exit_reason -eq 'Alt+F4' -and $wt -gt $work0 -and ([WV.Win]::WorkTop()) -eq $work0) "exited=$exited reason=$($j.exit_reason) work top while running=$wt after=$([WV.Win]::WorkTop())"

# ---- 3. real overflow
$mods = "schema_version = 3`n[bar]`n[[bar.modules]]`nid = `"menu`"`nkind = `"menu`"`n[[bar.modules]]`nid = `"active-window`"`nkind = `"active_window`"`n[[bar.modules]]`nid = `"clock`"`nkind = `"clock`"`nregion = `"center`"`n"
foreach ($n in 1..14) { $mods += "[[bar.modules]]`nid = `"note-$n`"`nkind = `"custom_text`"`nregion = `"right`"`ntext = `"Overflow sample module number $n here`"`n" }
$p = Bar 'overflow' @('--exit-after', '7') $mods
$null = Wait-Bar; Start-Sleep -Seconds 3
Assert-Backdrop; & $shot (Join-Path $Out 'overflow-real.png') 0 0 $ScreenW (Px 36.7) 1
$null = $p.WaitForExit(20000)
$j = Stats 'overflow'
Check 'overflow on the real monitor: modules hidden and named, tree lint clean' ((@($j.hidden_modules).Count -gt 0) -and (@($j.a11y_problems).Count -eq 0) -and (@($j.errors).Count -eq 0)) "hidden=$($j.hidden_modules -join ',')"

# ---- 4. themes over backdrops
# Shared pinned backdrop (stays at the top edge under the bar).
$backdropScript = Join-Path $ProjectRoot 'scripts\backdrop-window.ps1'
function Lum($c) { $f = { param($v) $v = $v / 255.0; if ($v -le 0.04045) { $v / 12.92 } else { [math]::Pow(($v + 0.055) / 1.055, 2.4) } }; 0.2126 * (& $f $c.R) + 0.7152 * (& $f $c.G) + 0.0722 * (& $f $c.B) }
function Contrast($a, $b) { $la = Lum $a; $lb = Lum $b; ([math]::Max($la, $lb) + 0.05) / ([math]::Min($la, $lb) + 0.05) }
$themes = @(
    @{ tag = 'dark'; args = @('--mode', 'dark'); text = '#E8EAEE'; h = 34 },
    @{ tag = 'light'; args = @('--mode', 'light'); text = '#1E2128'; h = 34 },
    @{ tag = 'glass-dark'; args = @('--theme', 'windowsrice-glass', '--mode', 'dark'); text = '#F0F3F7'; h = 36 },
    @{ tag = 'glass-light'; args = @('--theme', 'windowsrice-glass', '--mode', 'light'); text = '#161A21'; h = 36 }
)
$rows = @()
foreach ($bd in 'black', 'white', 'colour') {
    foreach ($t in $themes) {
        $tag = "bg-$($t.tag)-$bd"
        $bp = Start-Process powershell.exe -ArgumentList @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $backdropScript, '-Kind', $bd, '-Seconds', '9') -PassThru -WindowStyle Hidden
        Start-Sleep -Milliseconds 1500
        $p = Bar $tag (@('--exit-after', '6') + $t.args)
        $null = Wait-Bar; Start-Sleep -Seconds 3
        $png = Join-Path $Out "$tag.png"
        $hpx = Px $t.h
        Assert-Backdrop; & $shot $png 0 0 $ScreenW ($hpx + 30) 1
        $null = $p.WaitForExit(20000)
        $j = Stats $tag
        $bmp = [System.Drawing.Bitmap]::FromFile($png)
        $y = [int]($hpx / 2)
        # Sample inside the title module above its text (since Phase 7 the gaps
        # between modules are see-through) and in the empty
        # middle of the bar, plus the backdrop below the bar.
        $chip = $bmp.GetPixel((Px 153), (Px 6))
        $midX = [int]($ScreenW * 0.3125)   # empty middle of the bar (1200 px of 3840)
        $gap = $bmp.GetPixel($midX, $y)
        $below = $bmp.GetPixel($midX, $hpx + 15)
        $bmp.Dispose()
        $textHex = if ($t.text) { $t.text } elseif ($j.theme.mode -eq 'Light') { '#1A1C20' } else { '#E8EAED' }
        $text = [System.Drawing.ColorTranslator]::FromHtml($textHex)
        $rows += [pscustomobject]@{
            theme = $t.tag; backdrop = $bd; backplates = $j.surface.backplates; opacity = [math]::Round($j.surface.surface_opacity, 2)
            backdrop_px = '#{0:X2}{1:X2}{2:X2}' -f $below.R, $below.G, $below.B
            chip_px = '#{0:X2}{1:X2}{2:X2}' -f $chip.R, $chip.G, $chip.B; chip_contrast = [math]::Round((Contrast $text $chip), 2)
            surface_px = '#{0:X2}{1:X2}{2:X2}' -f $gap.R, $gap.G, $gap.B; surface_contrast = [math]::Round((Contrast $text $gap), 2)
        }
        $null = $bp.WaitForExit(15000)
    }
}
$rows | Format-Table -AutoSize | Out-String -Width 220
$rows | ConvertTo-Json | Set-Content (Join-Path $Out 'backdrops.json') -Encoding utf8
foreach ($r in $rows) {
    Check "contrast $($r.theme) over $($r.backdrop)" ($r.chip_contrast -ge 4.5) "chip $($r.chip_contrast):1 ($($r.chip_px)), surface $($r.surface_contrast):1, backplates=$($r.backplates)"
}
$checks | Format-Table -AutoSize -Wrap | Out-String -Width 220
$checks | ConvertTo-Json | Set-Content (Join-Path $Out 'checks.json') -Encoding utf8
$failed = @($checks | Where-Object { -not $_.pass }).Count
"{0} checks, {1} failed" -f $checks.Count, $failed

Stop-Process -Id $guardProc.Id -Force -ErrorAction SilentlyContinue
exit [int]($failed -gt 0)
