# windowsRICE panel: visual, DPI and legibility captures (real Windows).
#
# For each configuration the panel runs for a few seconds, its window region
# is captured from the screen, and its diagnostics are saved. For the
# translucent configurations the script measures the contrast between the
# theme's primary text colour and the pixels actually composited behind the
# text area (surface over the live Acrylic backdrop), so legibility claims do
# not rest on palette arithmetic alone.
#
# Forced scales (--scale) exercise layout and rendering at 100/125/200 %
# on this 150 % monitor; they are simulated DPI, not Windows DPI changes.

param(
    [string]$ProjectRoot = (Split-Path $PSScriptRoot -Parent),
    [string]$Out = (Join-Path (Split-Path $PSScriptRoot -Parent) 'target\phase4\visual'),
    [string]$Exe = 'target\release\windowsrice-panel.exe'
)
$ErrorActionPreference = 'Stop'
Set-Location $ProjectRoot
New-Item -ItemType Directory -Force $Out | Out-Null
Add-Type -AssemblyName System.Drawing
$shot = Join-Path $ProjectRoot 'experiments\ui-spike\target\release\shot.exe'
$title = '"문서 — 파일 탐색기"'

function Lum([System.Drawing.Color]$c) {
    $f = { param($v) $v = $v / 255.0; if ($v -le 0.04045) { $v / 12.92 } else { [math]::Pow(($v + 0.055) / 1.055, 2.4) } }
    0.2126 * (& $f $c.R) + 0.7152 * (& $f $c.G) + 0.0722 * (& $f $c.B)
}
function Contrast($a, $b) { $la = Lum $a; $lb = Lum $b; ([math]::Max($la, $lb) + 0.05) / ([math]::Min($la, $lb) + 0.05) }

$configs = @(
    @{ tag = 'default-dark-100'; args = @('--scale', '1.0') },
    @{ tag = 'default-dark-125'; args = @('--scale', '1.25') },
    @{ tag = 'default-dark-150-native'; args = @() },
    @{ tag = 'default-dark-200'; args = @('--scale', '2.0') },
    @{ tag = 'default-light-150'; args = @('--mode', 'light') },
    @{ tag = 'glass-dark-auto'; bar = @(36, 10, 14); args = @('--theme', 'windowsrice-glass', '--mode', 'dark'); text = '#F0F3F7' },
    @{ tag = 'glass-dark-never'; bar = @(36, 10, 14); args = @('--theme', 'windowsrice-glass', '--mode', 'dark', '--backplates', 'never'); text = '#F0F3F7' },
    @{ tag = 'glass-dark-opacity90'; bar = @(36, 10, 14); args = @('--theme', 'windowsrice-glass', '--mode', 'dark', '--opacity', '0.9', '--backplates', 'never'); text = '#F0F3F7' },
    @{ tag = 'glass-dark-reduced'; bar = @(36, 10, 14); args = @('--theme', 'windowsrice-glass', '--mode', 'dark', '--reduced-transparency'); text = '#F0F3F7' },
    @{ tag = 'glass-light-auto'; bar = @(36, 10, 14); args = @('--theme', 'windowsrice-glass', '--mode', 'light'); text = '#161A21' },
    @{ tag = 'glass-light-never'; bar = @(36, 10, 14); args = @('--theme', 'windowsrice-glass', '--mode', 'light', '--backplates', 'never'); text = '#161A21' }
)
$rows = @()
foreach ($c in $configs) {
    $stats = Join-Path $Out "$($c.tag).json"
    $p = Start-Process $Exe -ArgumentList (@('--exit-after', '6', '--stats', $stats, '--no-hotkey', '--lang', 'ko', '--title', $title) + $c.args) -PassThru
    Start-Sleep -Milliseconds 3500
    $scale = 1.5; for ($i = 0; $i -lt $c.args.Count; $i++) { if ($c.args[$i] -eq '--scale') { $scale = [double]$c.args[$i + 1] } }
    # Bar tokens (height, margin, padding_x) in DIPs: Default 34/8/12, Glass 36/10/14.
    $bar = if ($c.bar) { $c.bar } else { @(34, 8, 12) }
    $m = [int][math]::Round($bar[1] * $scale); $h = [int][math]::Round($bar[0] * $scale); $w = 3840 - 2 * $m
    $full = Join-Path $Out "$($c.tag)-full.png"
    & $shot $full $m $m $w $h 1
    & $shot (Join-Path $Out "$($c.tag)-left.png") $m $m ([int](460 * $scale)) $h 2
    & $shot (Join-Path $Out "$($c.tag)-center.png") ([int](1920 - 180 * $scale)) $m ([int](360 * $scale)) $h 2
    & $shot (Join-Path $Out "$($c.tag)-right.png") ([int](3840 - $m - 400 * $scale)) $m ([int](400 * $scale)) $h 2
    $null = $p.WaitForExit(15000)
    $j = Get-Content $stats -Raw | ConvertFrom-Json
    $row = [ordered]@{
        config = $c.tag; exit = $p.ExitCode; window = ($j.monitor.window_rect -join ','); scale = $j.monitor.scale_used
        dpi = $j.render.dpi; backdrop = $j.surface.backdrop; reason = $j.surface.reason; opacity = [math]::Round($j.surface.surface_opacity, 2)
        backplates = $j.surface.backplates; worst_case = [math]::Round($j.surface.worst_case_text_contrast, 2); meets_aa = $j.surface.meets_aa_worst_case
        warnings = ($j.warnings -join '; '); errors = ($j.errors -join '; ')
    }
    if ($c.text) {
        # Real composited background under the active-window text area: the
        # chip padding left of the text (inside the backplate when present)
        # and the plain surface between the title and the clock.
        $bmp = [System.Drawing.Bitmap]::FromFile($full)
        $text = [System.Drawing.ColorTranslator]::FromHtml($c.text)
        $y = [int]($h / 2)
        # Active-window chip: padding_x + menu chip (height - 8) + gap 4, then 3 DIPs in.
        $chipX = [int](($bar[2] + ($bar[0] - 8) + 4 + 3) * $scale)
        $samples = @($bmp.GetPixel($chipX, $y), $bmp.GetPixel([int](900 * $scale), $y), $bmp.GetPixel([int](1500 * $scale), 3))
        $bmp.Dispose()
        $row.chip_bg = '#{0:X2}{1:X2}{2:X2}' -f $samples[0].R, $samples[0].G, $samples[0].B
        $row.measured_text_contrast_chip = [math]::Round((Contrast $text $samples[0]), 2)
        $row.surface_bg = '#{0:X2}{1:X2}{2:X2}' -f $samples[1].R, $samples[1].G, $samples[1].B
        $row.measured_text_contrast_surface = [math]::Round((Contrast $text $samples[1]), 2)
    }
    $rows += [pscustomobject]$row
}
$rows | Format-Table -AutoSize -Wrap | Out-String -Width 260
$rows | ConvertTo-Json | Set-Content (Join-Path $Out 'summary.json') -Encoding utf8
