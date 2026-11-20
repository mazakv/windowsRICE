# windowsRICE Bar (Phase 5): visual captures on this machine.
# Forced scales (--scale) are simulated DPI on the 150 % monitor, not Windows
# DPI changes. Uses fresh project-local config files (defaults).

param(
    [string]$ProjectRoot = (Split-Path $PSScriptRoot -Parent),
    [string]$Out = (Join-Path (Split-Path $PSScriptRoot -Parent) 'target\phase5\visual'),
    [string]$Exe = 'target\release\windowsrice-panel.exe'
)
$ErrorActionPreference = 'Stop'
Set-Location $ProjectRoot
New-Item -ItemType Directory -Force $Out | Out-Null
$shot = Join-Path $ProjectRoot 'experiments\ui-spike\target\release\shot.exe'
$configs = @(
    @{ tag = 'default-dark-100'; scale = 1.0; args = @('--scale', '1.0') },
    @{ tag = 'default-dark-125'; scale = 1.25; args = @('--scale', '1.25') },
    @{ tag = 'default-dark-150-native'; scale = 1.5; args = @() },
    @{ tag = 'default-dark-200'; scale = 2.0; args = @('--scale', '2.0') },
    @{ tag = 'overflow-300'; scale = 3.0; args = @('--scale', '3.0') },
    @{ tag = 'default-light'; scale = 1.5; args = @('--mode', 'light') },
    @{ tag = 'glass-dark'; scale = 1.5; h = 36; args = @('--theme', 'windowsrice-glass', '--mode', 'dark') },
    @{ tag = 'glass-reduced'; scale = 1.5; h = 36; args = @('--theme', 'windowsrice-glass', '--mode', 'dark', '--reduced-transparency') },
    @{ tag = 'korean'; scale = 1.5; args = @('--lang', 'ko'); popup = $true }
)
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes -ErrorAction SilentlyContinue
$rows = @()
foreach ($c in $configs) {
    $cfg = Join-Path $Out "config-$($c.tag).toml"
    Remove-Item $cfg -ErrorAction SilentlyContinue
    $stats = Join-Path $Out "$($c.tag).json"
    $lang = if ($c.args -contains '--lang') { @() } else { @('--lang', 'ko') }
    $p = Start-Process $Exe -ArgumentList (@('--exit-after', '7', '--stats', $stats, '--config', $cfg, '--no-hotkey', '--title', '"문서 — 파일 탐색기"') + $lang + $c.args) -PassThru
    Start-Sleep -Milliseconds 4000
    $hd = if ($c.h) { $c.h } else { 34 }
    $h = [int][math]::Round($hd * $c.scale)
    & $shot (Join-Path $Out "$($c.tag)-full.png") 0 0 3840 ($h + 4) 1
    & $shot (Join-Path $Out "$($c.tag)-left.png") 0 0 ([int](480 * $c.scale)) $h 2
    & $shot (Join-Path $Out "$($c.tag)-right.png") ([int](3840 - 640 * $c.scale)) 0 ([int](640 * $c.scale)) $h 1
    $null = $p.WaitForExit(20000)
    $j = Get-Content $stats -Raw | ConvertFrom-Json
    $rows += [pscustomobject]@{
        config = $c.tag; exit = $p.ExitCode; window = ($j.bars[-1].window_rect -join ','); dpi = $j.bars[-1].dpi
        appbar = $j.bars[-1].appbar; work_after = ($j.bars[-1].work_area_after -join ','); hidden = ($j.hidden_modules -join ',')
        theme = $j.theme.id; backplates = $j.surface.backplates; reason = $j.surface.reason
        a11y_problems = @($j.a11y_problems).Count; errors = ($j.errors -join '; ')
    }
}
$rows | Format-Table -AutoSize -Wrap | Out-String -Width 260
$rows | ConvertTo-Json | Set-Content (Join-Path $Out 'summary.json') -Encoding utf8
