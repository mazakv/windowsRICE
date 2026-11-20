# Creates and pushes a release tag after verifying everything locally. The
# GitHub "Release" workflow then rebuilds the tag, re-runs the checks and
# publishes the release (see docs/releasing.md).
#
#   pwsh -File scripts/release.ps1 -Version 0.1.0-rc.2 -Prerelease
#   pwsh -File scripts/release.ps1 -Version 0.1.0-rc.2 -Prerelease -DryRun
#
# Refuses to continue unless:
#   - the working tree is clean and on master, level with origin/master
#   - the workspace version in Cargo.toml equals -Version
#   - -Prerelease matches the version (a suffix such as -rc.2 means pre-release)
#   - CHANGELOG.md has a section for the version
#   - the tag does not exist locally or on GitHub
#   - the full check, the application checks, the package, its checksum and
#     the installer verification all pass
# Never force-pushes; pushes only the new tag.
param(
    [Parameter(Mandatory)][string]$Version,
    [switch]$Prerelease,
    [switch]$DryRun,
    [switch]$SkipAppTests,
    # Tagger identity when git has no user.name/user.email configured.
    [string]$TaggerName,
    [string]$TaggerEmail,
    [string]$ProjectRoot = (Split-Path $PSScriptRoot -Parent)
)
$ErrorActionPreference = 'Stop'
Set-Location $ProjectRoot
function Step($t) { Write-Host "`n== $t" -ForegroundColor Cyan }
function Native([scriptblock]$b, [string]$what) {
    $prev = $ErrorActionPreference; $ErrorActionPreference = 'Continue'
    & $b; $code = $LASTEXITCODE
    $ErrorActionPreference = $prev
    if ($code -ne 0) { throw "$what failed (exit $code)" }
}

if ($Version -notmatch '^\d+\.\d+\.\d+(-[0-9A-Za-z.]+)?$') { throw "not a version: $Version" }
$isPre = $Version -match '-'
if ($isPre -ne [bool]$Prerelease) {
    throw $(if ($isPre) { "$Version is a pre-release version: pass -Prerelease" } else { "$Version is a stable version: stable releases need the manual checks in docs/releasing.md; do not pass -Prerelease" })
}
$tag = "v$Version"

Step 'Repository state'
$branch = (git rev-parse --abbrev-ref HEAD).Trim()
if ($branch -ne 'master') { throw "on branch $branch; releases are tagged from master" }
$dirty = git status --porcelain
if ($dirty) { throw "the working tree has changes:`n$($dirty -join "`n")" }
Native { git fetch --quiet origin master --tags } 'git fetch'
$local = (git rev-parse HEAD).Trim(); $remote = (git rev-parse origin/master).Trim()
if ($local -ne $remote) { throw "master ($local) is not level with origin/master ($remote): push or pull first" }
if (git tag --list $tag) { throw "tag $tag already exists locally" }
if (git ls-remote --tags origin "refs/tags/$tag") { throw "tag $tag already exists on GitHub" }
$name = if ($TaggerName) { $TaggerName } else { git config user.name }
$mail = if ($TaggerEmail) { $TaggerEmail } else { git config user.email }
if (-not $name -or -not $mail) { throw 'no tagger identity: configure git user.name/user.email or pass -TaggerName and -TaggerEmail' }
Write-Host "master at $local, level with origin"

Step 'Version'
$cargo = (Select-String -Path Cargo.toml -Pattern '^version = "([^"]+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
if ($cargo -ne $Version) { throw "Cargo.toml has version $cargo, not $Version" }
if (-not (Select-String -Path CHANGELOG.md -Pattern "^## \[$([regex]::Escape($Version))\]" -Quiet)) { throw "CHANGELOG.md has no section for $Version" }
Write-Host "Cargo.toml and CHANGELOG.md: $Version"

Step 'Format, Clippy, tests, release build'
Native { & (Join-Path $PSScriptRoot 'check.ps1') full } 'check.ps1 full'
Native { cargo build --workspace --locked } 'debug build'
Native { & (Join-Path $PSScriptRoot 'third-party-notices.ps1') -Check } 'third-party notices'

if (-not $SkipAppTests) {
    Step 'Application checks'
    Native { & (Join-Path $PSScriptRoot 'app-tests.ps1') } 'application checks'
}

Step 'Package, checksum, installer'
$ps5 = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
Native { & $ps5 -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'package.ps1') } 'package'
$zip = "target\dist\windowsRICE-$Version-win-x64.zip"
if (-not (Test-Path $zip)) { throw "missing $zip" }
$line = (Get-Content "$zip.sha256" -Raw).Trim()
$hash = (Get-FileHash $zip -Algorithm SHA256).Hash
if ($line -ne "$hash  $(Split-Path $zip -Leaf)") { throw "checksum file does not match the zip: $line" }
Native { & $ps5 -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'package-verify.ps1') } 'package verification'
Write-Host "$zip  SHA-256 $hash"

if ((git status --porcelain)) { throw 'the checks changed tracked files; commit or revert them first' }

if ($DryRun) { Step "Dry run: would tag $tag at $local and push it"; exit 0 }
Step "Tag $tag"
$who = @()
if ($TaggerName) { $who += @('-c', "user.name=$TaggerName") }
if ($TaggerEmail) { $who += @('-c', "user.email=$TaggerEmail") }
Native { git @who tag -a $tag -m "windowsRICE $Version" } 'git tag'
Native { git push origin "refs/tags/$tag" } 'git push tag'
Write-Host "Pushed $tag. The Release workflow now builds and publishes it:"
Write-Host "  gh run list --workflow release.yml --limit 1"
Write-Host "  gh release view $tag"
