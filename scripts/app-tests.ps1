# Runs the real-application verification suites and writes one report:
# target\ci\application-check-results.json (and a GitHub job summary when
# run in GitHub Actions).
#
# Profiles:
#   FullLocal     the release acceptance profile (scripts/release.ps1 always
#                 uses it): every check of every suite must PASS. A check that
#                 did not run is a failure.
#   GitHubHosted  GitHub's hosted Windows runner (CI and the Release
#                 workflow). Every suite runs and every check must PASS,
#                 except the checks in the allowlist below, which need a real
#                 interactive desktop with a GPU compositor; on the runner
#                 those are reported NOT_RUN with their reason (never PASS).
#                 Any other failure, any check that did not run outside the
#                 allowlist, and any suite that aborts fail the run.
#
# Each suite uses its own isolated configuration under target\ and never
# touches %APPDATA%\windowsRICE. Requires an interactive desktop session
# and target\release\windowsrice-panel.exe.
param(
    [ValidateSet('FullLocal', 'GitHubHosted')][string]$Profile = 'FullLocal',
    [string]$ProjectRoot = (Split-Path $PSScriptRoot -Parent),
    [string[]]$Only = @()
)
$ErrorActionPreference = 'Stop'
Set-Location $ProjectRoot
if (-not (Test-Path 'target\release\windowsrice-panel.exe')) { throw 'build the release first (cargo build --release -p windowsrice-panel)' }

# Suites and where they write their checks.
$Suites = [ordered]@{
    'design-e2e'     = 'target\phase8\e2e\checks.json'
    'studio-verify'  = 'target\phase6\verify\checks.json'
    'bar-a11y'       = 'target\phase5\a11y\checks.json'
    'phase45-verify' = 'target\phase45-verify\checks.json'
    'panels-verify'  = 'target\phase13\panels\checks.json'
    'bottom-verify'  = 'target\phase14\bottom\checks.json'
    'media-verify'   = 'target\phase14\media\checks.json'
}

# GitHubHosted only: checks that need what GitHub's hosted runner does not
# provide. Keyed "suite|check"; the value is the reason shown for NOT_RUN.
$Focus = 'needs foreground keyboard focus on a real interactive desktop; the hosted runner does not give the bar window foreground focus'
$Compositor = 'needs real DirectComposition translucency over the desktop; the hosted runner renders with software (WARP) and its captures do not show it'
$Dialog = 'needs the native Windows Save/Open dialog driven by UI Automation; not automatable on the hosted runner'
$HostedAllowlist = @{
    'bar-a11y|hit testing returns the menu'                                     = $Focus
    'bar-a11y|UIA SetFocus (provider view)'                                      = $Focus
    'bar-a11y|keyboard: Tab, Left, Home, End'                                    = $Focus
    'bar-a11y|UIA focus-changed events'                                          = $Focus
    'bar-a11y|UIA Name-changed events'                                           = $Focus
    'bar-a11y|auto-hide: collapsed to a 2 px strip, work area released'          = 'needs the real mouse pointer away from the top edge; the hosted runner''s pointer position is not controlled'
    'design-e2e|translucent background shows the backdrop through the real bar' = $Compositor
    'design-e2e|Ocean Blue background on the real bar (#0E2A3F)'                = $Compositor
    'studio-verify|export through the Save dialog'                               = $Dialog
    'studio-verify|malformed preset rejected with a reason'                      = $Dialog
    'studio-verify|valid preset imported into the presets folder'                = $Dialog
    'studio-verify|Export .ricepack writes a package'                            = $Dialog
    'studio-verify|Open .ricepack checks the package and its design'             = $Dialog
    'panels-verify|Volume panel'                                                 = 'needs an audio output device; the hosted runner has none'
}

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

# `pwsh -File` passes "a,b" as one string; accept both forms.
$Only = @($Only | ForEach-Object { $_ -split ',' } | ForEach-Object { $_.Trim() } | Where-Object { $_ })
$names = @($Suites.Keys | Where-Object { $Only.Count -eq 0 -or $Only -contains $_ })
if ($Only.Count -and $names.Count -ne $Only.Count) { throw "unknown suite in -Only: $($Only -join ', ')" }

# Environment, recorded with the results.
Add-Type -AssemblyName System.Windows.Forms
$scr = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
$envInfo = [ordered]@{
    runner      = $(if ($env:GITHUB_ACTIONS -eq 'true') { "GitHub-hosted ($env:RUNNER_OS, $env:ImageOS)" } else { 'local machine' })
    os          = [Environment]::OSVersion.VersionString
    screen      = "$($scr.Width)x$($scr.Height)"
    commit      = (git rev-parse --short HEAD 2>$null)
    started_utc = (Get-Date).ToUniversalTime().ToString('s') + 'Z'
}

$host5 = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
$results = [System.Collections.Generic.List[object]]::new()
function Add-Result($suite, $name, $status, $reason, $detail) {
    $results.Add([pscustomobject][ordered]@{ suite = $suite; name = $name; status = $status; reason = $reason; detail = "$detail"; profile = $Profile })
}
foreach ($s in $names) {
    Write-Host "`n=== $s ($Profile)"
    $file = Join-Path $ProjectRoot $Suites[$s]
    if (Test-Path $file) { Remove-Item $file -Force }
    $sw = [Diagnostics.Stopwatch]::StartNew()
    $prev = $ErrorActionPreference; $ErrorActionPreference = 'Continue'
    $lines = @(& $host5 -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "$s.ps1") 2>&1 | ForEach-Object { "$_" })
    $code = $LASTEXITCODE
    $ErrorActionPreference = $prev
    $lines | Out-Host
    Get-Process windowsrice-panel -ErrorAction SilentlyContinue | Stop-Process -Force
    if (-not (Test-Path $file)) {
        $err = @($lines | Where-Object { $_ -match 'Exception|throw|error' }) | Select-Object -First 2
        Add-Result $s "$s finished" 'FAIL' $null "the suite aborted after $([math]::Round($sw.Elapsed.TotalSeconds)) s without results: $($err -join ' | ')"
        continue
    }
    foreach ($c in @(Get-Content $file -Raw | ConvertFrom-Json)) {
        $key = "$s|$($c.check)"
        $ran = $c.pass -is [bool]
        if ($ran -and $c.pass) {
            Add-Result $s $c.check 'PASS' $null $c.detail
        } elseif ($Profile -eq 'GitHubHosted' -and $HostedAllowlist.ContainsKey($key)) {
            Add-Result $s $c.check 'NOT_RUN' ('UNSUPPORTED HOSTED RUNNER ENVIRONMENT: ' + $HostedAllowlist[$key]) $c.detail
        } elseif ($ran) {
            Add-Result $s $c.check 'FAIL' $null $c.detail
        } else {
            Add-Result $s $c.check 'FAIL' $null "required check did not run ($($c.pass)): $($c.detail)"
        }
    }
    # A suite that exits with an error must have a non-passing check to show for it.
    if ($code -ne 0 -and -not (@($results | Where-Object { $_.suite -eq $s -and $_.status -ne 'PASS' }).Count)) {
        Add-Result $s "$s exit code" 'FAIL' $null "the suite exited with code $code"
    }
}

$pass = @($results | Where-Object status -eq 'PASS').Count
$fail = @($results | Where-Object status -eq 'FAIL').Count
$notRun = @($results | Where-Object status -eq 'NOT_RUN')
$report = [ordered]@{
    profile     = $Profile
    environment = $envInfo
    totals      = [ordered]@{ checks = $results.Count; passed = $pass; failed = $fail; not_run = $notRun.Count }
    checks      = $results
}
New-Item -ItemType Directory -Force (Join-Path $ProjectRoot 'target\ci') | Out-Null
$out = Join-Path $ProjectRoot 'target\ci\application-check-results.json'
$report | ConvertTo-Json -Depth 6 | Set-Content $out -Encoding utf8

$md = [System.Collections.Generic.List[string]]::new()
$md.Add("## Application checks ($Profile)")
$md.Add('')
$md.Add("Total **$($results.Count)** · passed **$pass** · failed **$fail** · not run **$($notRun.Count)**")
$md.Add('')
foreach ($s in $names) {
    $r = @($results | Where-Object suite -eq $s)
    $md.Add("- ``$s``: $(@($r | Where-Object status -eq 'PASS').Count) passed, $(@($r | Where-Object status -eq 'FAIL').Count) failed, $(@($r | Where-Object status -eq 'NOT_RUN').Count) not run")
}
if ($fail) {
    $md.Add(''); $md.Add('### Failed')
    foreach ($r in $results | Where-Object status -eq 'FAIL') { $md.Add("- ``$($r.suite)`` $($r.name): $($r.detail)") }
}
if ($notRun.Count) {
    $md.Add(''); $md.Add('### Not run (not passed)')
    foreach ($r in $notRun) { $md.Add("- ``$($r.suite)`` $($r.name) — $($r.reason)") }
}
$md -join "`n" | Out-Host
if ($env:GITHUB_STEP_SUMMARY) { $md -join "`n" | Add-Content $env:GITHUB_STEP_SUMMARY -Encoding utf8 }
Write-Host "report: $out"
if ($fail) { Write-Host "FAILED: $fail check(s)"; exit 1 }
Write-Host "all required application checks passed ($Profile)"
