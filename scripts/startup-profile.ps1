# Startup profile of the release bar (Phase 11). Runs the bar with an
# isolated configuration and reads the startup milestones it records with
# --stats (milliseconds since process creation, precise clock).
#
#   warm:  the same executable launched -Runs times in a row
#   fresh: -Fresh copies of the executable, each launched once right after
#          being written to a new path (first launch of a new file, so
#          image loading and on-access antivirus scanning are included).
#          A true cold start (empty file cache after a reboot) is not
#          measured.
#
# Results: target\startup-profile\results.json and a table on stdout.
param(
    [string]$ProjectRoot = (Split-Path $PSScriptRoot -Parent),
    [string]$Exe = 'target\release\windowsrice-panel.exe',
    [int]$Runs = 7,
    [int]$Fresh = 3,
    # Seconds to wait before each launch. The previous instance's exit
    # removes its AppBar, and Explorer then rearranges the work area; a
    # launch during that work is measurably slower (docs/phase11-15).
    [int]$GapSeconds = 4,
    # Fails (exit 1) when the warm median to first frame exceeds this. The
    # Phase 11 reference on the development machine is ~165 ms.
    [double]$BudgetMs = 250,
    [string[]]$Extra = @()
)
$ErrorActionPreference = 'Stop'
Set-Location $ProjectRoot
if (@(Get-Process windowsrice-panel -ErrorAction SilentlyContinue).Count -gt 0) { throw 'a windowsRICE bar is running; close it first' }
$Out = Join-Path $ProjectRoot 'target\startup-profile'
if (Test-Path $Out) { Remove-Item -Recurse -Force $Out }
New-Item -ItemType Directory -Force $Out | Out-Null
$cfg = Join-Path $Out 'config.toml'

function Run-Once([string]$exe, [string]$tag) {
    if ($GapSeconds -gt 0) { Start-Sleep -Seconds $GapSeconds }
    $stats = Join-Path $Out "$tag.json"
    $args = @('--config', $cfg, '--stats', $stats, '--no-hotkey', '--exit-after', '3', '--title', '"Startup test"') + $Extra
    $p = Start-Process $exe -ArgumentList $args -PassThru
    if (-not $p.WaitForExit(30000)) { $p.Kill(); throw "$tag did not exit" }
    $j = Get-Content $stats -Raw | ConvertFrom-Json
    $row = [ordered]@{ run = $tag; first_frame = [math]::Round($j.ms_process_to_first_frame, 1) }
    foreach ($m in $j.startup_milestones) { $row[$m[0]] = [math]::Round([double]$m[1], 1) }
    [pscustomobject]$row
}

# One launch creates the configuration, so every measured run reads an
# existing file (as at a normal sign-in).
$null = Run-Once $Exe 'setup'
$warmRuns = foreach ($i in 1..$Runs) { Run-Once $Exe "warm$i" }
$freshRuns = foreach ($i in 1..$Fresh) {
    $copy = Join-Path $Out "fresh$i\windowsrice-panel.exe"
    New-Item -ItemType Directory -Force (Split-Path $copy) | Out-Null
    Copy-Item $Exe $copy
    Run-Once $copy "fresh$i"
}

function Median($xs) { $s = @($xs | Where-Object { $_ -ne $null } | Sort-Object); if ($s.Count -eq 0) { return $null }; $s[[int][math]::Floor($s.Count / 2)] }
$names = @($warmRuns[0].PSObject.Properties.Name | Where-Object { $_ -ne 'run' })
$summary = foreach ($set in @(@{ n = 'warm'; r = $warmRuns }, @{ n = 'fresh'; r = $freshRuns })) {
    $row = [ordered]@{ set = $set.n }
    foreach ($k in $names) { $row[$k] = Median ($set.r | ForEach-Object { $_.$k }) }
    [pscustomobject]$row
}
[pscustomobject]@{ exe = $Exe; extra = $Extra; warm = $warmRuns; fresh = $freshRuns; median = $summary } |
    ConvertTo-Json -Depth 5 | Set-Content (Join-Path $Out 'results.json') -Encoding utf8
@($warmRuns) + @($freshRuns) | Format-Table -AutoSize | Out-String -Width 400
'median'
$summary | Format-List | Out-String -Width 400
# Stage durations (warm median) to point at the cause of a regression.
$w = $summary | Where-Object set -eq 'warm'
$prev = 0.0
foreach ($k in $names | Where-Object { $_ -ne 'first_frame' }) {
    if ($null -ne $w.$k) { '{0,-28} +{1,6:N1} ms' -f $k, ($w.$k - $prev); $prev = $w.$k }
}
if ($w.first_frame -gt $BudgetMs) {
    "FAIL: warm median to first frame {0} ms exceeds the budget of {1} ms" -f $w.first_frame, $BudgetMs
    exit 1
}
"PASS: warm median to first frame {0} ms (budget {1} ms)" -f $w.first_frame, $BudgetMs
