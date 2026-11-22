# Runs the real-application verification scripts against the release build,
# one after another, and fails if any of them fails. Used by CI and by
# scripts/release.ps1. Each script uses its own isolated configuration under
# target\ and never touches %APPDATA%\windowsRICE.
#
#   design-e2e      end-to-end Apply on the running bar (Phase 7-9)
#   studio-verify   Design Studio window: keyboard, UI Automation, editing
#   bar-a11y        bar and settings popup through UI Automation
#   phase45-verify  Phase 4/5 checks: overflow, legibility over backdrops
#
# Requires an interactive desktop session (a GitHub-hosted Windows runner
# provides one) and target\release\windowsrice-panel.exe.
param(
    [string]$ProjectRoot = (Split-Path $PSScriptRoot -Parent),
    [string[]]$Only = @()
)
$ErrorActionPreference = 'Stop'
Set-Location $ProjectRoot
if (-not (Test-Path 'target\release\windowsrice-panel.exe')) { throw 'build the release first (cargo build --release -p windowsrice-panel)' }

# The scripts capture screenshots (only over a pinned test backdrop) with the
# small `shot` tool of the Phase 3.5 spike workspace.
$shot = Join-Path $ProjectRoot 'experiments\ui-spike\target\release\shot.exe'
if (-not (Test-Path $shot)) {
    Write-Host 'building the screenshot tool (experiments/ui-spike, package shot)'
    $prev = $ErrorActionPreference; $ErrorActionPreference = 'Continue'
    & cargo build --release --locked --manifest-path (Join-Path $ProjectRoot 'experiments\ui-spike\Cargo.toml') -p shot 2>&1 | ForEach-Object { "$_" } | Select-Object -Last 3 | Out-Host
    $code = $LASTEXITCODE; $ErrorActionPreference = $prev
    if ($code -ne 0 -or -not (Test-Path $shot)) { throw 'could not build the screenshot tool' }
}

$suite = @('design-e2e', 'studio-verify', 'bar-a11y', 'phase45-verify', 'panels-verify', 'bottom-verify', 'media-verify')
# `pwsh -File` passes "a,b" as one string; accept both forms.
$Only = @($Only | ForEach-Object { $_ -split ',' } | ForEach-Object { $_.Trim() } | Where-Object { $_ })
if ($Only.Count) { $suite = @($suite | Where-Object { $Only -contains $_ }) }
$host5 = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
$results = @()
foreach ($s in $suite) {
    Write-Host "`n=== $s"
    $sw = [Diagnostics.Stopwatch]::StartNew()
    # Windows PowerShell 5.1: the scripts use its UI Automation assemblies.
    $prev = $ErrorActionPreference; $ErrorActionPreference = 'Continue'
    $lines = @(& $host5 -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "$s.ps1") 2>&1 | ForEach-Object { "$_" })
    $code = $LASTEXITCODE
    $ErrorActionPreference = $prev
    $lines | Out-Host
    Get-Process windowsrice-panel -ErrorAction SilentlyContinue | Stop-Process -Force
    $summary = @($lines | Where-Object { $_ -match '^\d+ checks, ' }) | Select-Object -Last 1
    $results += [pscustomobject]@{ Script = $s; Exit = $code; Seconds = [math]::Round($sw.Elapsed.TotalSeconds); Summary = $summary }
}
$results | Format-Table -AutoSize | Out-String -Width 200 | Out-Host
$bad = @($results | Where-Object { $_.Exit -ne 0 -or -not $_.Summary })
if ($bad.Count) { Write-Host ('FAILED: ' + (($bad | ForEach-Object { $_.Script }) -join ', ')); exit 1 }
Write-Host 'all application checks passed'
