# windowsRICE verification helper. See docs/development.md for when to use each level.
#
#   pwsh -File scripts/check.ps1 quick -Package windowsrice-theme [-Filter name]
#   pwsh -File scripts/check.ps1 quick -Changed                 # packages touched since HEAD
#   pwsh -File scripts/check.ps1 integration -Package a,b [-WithDependents]
#   pwsh -File scripts/check.ps1 full [-ReleaseTests]
#
# The script only runs Cargo commands. It never cleans the build cache, never
# changes Windows settings and never edits files other than Cargo's own output.

param(
    [Parameter(Mandatory, Position = 0)][ValidateSet('quick', 'integration', 'full')][string]$Level,
    [string[]]$Package = @(),
    [switch]$Changed,
    [switch]$WithDependents,
    [string]$Filter,
    [switch]$ReleaseTests
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
# Keep Cargo and rustc temporary files inside the project.
$env:TMP = Join-Path $root 'target\tmp'; $env:TEMP = $env:TMP
New-Item -ItemType Directory -Force $env:TMP | Out-Null

# `pwsh -File` passes "a,b" as one string; accept both forms.
$Package = @($Package | ForEach-Object { $_ -split ',' } | ForEach-Object { $_.Trim() } | Where-Object { $_ })

$meta = cargo metadata --format-version 1 --no-deps | ConvertFrom-Json
$members = @($meta.packages | Where-Object { $meta.workspace_members -contains $_.id })

if ($Changed) {
    $files = @(git diff --name-only HEAD) + @(git ls-files --others --exclude-standard)
    foreach ($p in $members) {
        $dir = (Split-Path -Parent $p.manifest_path).Substring($root.Length + 1).Replace('\', '/') + '/'
        if ($files | Where-Object { $_.StartsWith($dir) }) { $Package += $p.name }
    }
    if ($files | Where-Object { $_ -in 'Cargo.toml', 'Cargo.lock' }) {
        Write-Host 'Workspace manifest or lock file changed: use the full level.' -ForegroundColor Yellow
    }
}
$Package = @($Package | Sort-Object -Unique)

if ($WithDependents -and $Package.Count) {
    # Add workspace packages that depend (directly or transitively) on a selected one.
    do {
        $before = $Package.Count
        foreach ($p in $members) {
            if ($Package -notcontains $p.name -and ($p.dependencies | Where-Object { $Package -contains $_.name })) { $Package += $p.name }
        }
    } while ($Package.Count -ne $before)
}

if ($Level -ne 'full' -and $Package.Count -eq 0) {
    Write-Host 'No packages selected. Pass -Package, or -Changed with modified package files.' -ForegroundColor Yellow
    exit 2
}
$sel = @($Package | ForEach-Object { '-p', $_ })

$steps = [System.Collections.Generic.List[object]]::new()
function Step([string]$name, [string[]]$a) { $steps.Add([pscustomobject]@{ Name = $name; Args = $a }) }
switch ($Level) {
    'quick' {
        Step 'check' (@('check') + $sel + '--all-targets')
        Step 'test' (@('test') + $sel + $(if ($Filter) { @($Filter) } else { @() }))
    }
    'integration' {
        Step 'fmt' (@('fmt') + $sel + @('--', '--check'))
        Step 'check' (@('check') + $sel + '--all-targets')
        Step 'clippy' (@('clippy') + $sel + @('--all-targets', '--', '-D', 'warnings'))
        Step 'test' (@('test') + $sel)
    }
    'full' {
        Step 'fmt' @('fmt', '--all', '--', '--check')
        Step 'check' @('check', '--workspace', '--all-targets')
        Step 'clippy' @('clippy', '--workspace', '--all-targets', '--', '-D', 'warnings')
        Step 'test' @('test', '--workspace')
        Step 'build-release' @('build', '--workspace', '--release')
        if ($ReleaseTests) { Step 'test-release' @('test', '--workspace', '--release') }
    }
}

$results = @(); $failed = $false
foreach ($s in $steps) {
    $name, $cargoArgs = $s.Name, $s.Args
    Write-Host "`n>> cargo $($cargoArgs -join ' ')" -ForegroundColor Cyan
    $sw = [Diagnostics.Stopwatch]::StartNew()
    & cargo @cargoArgs 2>&1 | ForEach-Object { "$_" } | Tee-Object -Variable lines | Out-Host
    $code = $LASTEXITCODE; $sw.Stop()
    $text = $lines -join "`n"
    $passed = [int]([regex]::Matches($text, 'test result: \w+\. (\d+) passed') | ForEach-Object { [int]$_.Groups[1].Value } | Measure-Object -Sum).Sum
    $fails = [int]([regex]::Matches($text, 'test result: \w+\. \d+ passed; (\d+) failed') | ForEach-Object { [int]$_.Groups[1].Value } | Measure-Object -Sum).Sum
    $filtered = [int]([regex]::Matches($text, '(\d+) filtered out') | ForEach-Object { [int]$_.Groups[1].Value } | Measure-Object -Sum).Sum
    $isTest = $name -like 'test*'
    $results += [pscustomobject]@{ Step = $name; Exit = $code; Seconds = [math]::Round($sw.Elapsed.TotalSeconds, 1); Passed = $(if ($isTest) { $passed } else { '' }); Failed = $(if ($isTest) { $fails } else { '' }); FilteredOut = $(if ($isTest) { $filtered } else { '' }) }
    if ($code -ne 0) { $failed = $true; break }
}

Write-Host "`n== $Level check$(if ($Package.Count) { ' for ' + ($Package -join ', ') }) ==" -ForegroundColor Cyan
$results | Format-Table -AutoSize | Out-String | Write-Host
$deferred = switch ($Level) {
    'quick' { 'fmt, clippy, other packages, release build (run integration or full before finishing)' }
    'integration' { 'packages outside the selection, release build (run full before finishing the phase)' }
    'full' { if ($ReleaseTests) { 'none' } else { 'release-mode tests (pass -ReleaseTests for release validation)' } }
}
Write-Host "Deferred: $deferred"
if ($failed) { Write-Host 'FAILED: stopped at the first failing step.' -ForegroundColor Red; exit 1 }
