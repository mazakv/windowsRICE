# Builds the release and creates the distributable zip:
#   target\dist\windowsRICE-<version>-win-x64.zip (+ .sha256)
# The zip holds windowsrice-panel.exe (with icon and version information),
# install.ps1, uninstall.ps1, windowsrice.ico and INSTALL.txt. Nothing is
# signed, uploaded or published.
#
# The package build uses its own target folder (target\package) and strips
# local paths from the executable: source paths are remapped
# (--remap-path-prefix) and only the PDB file name is recorded
# (/PDBALTPATH), so the builder's user folder never ships.
param([string]$ProjectRoot = (Split-Path $PSScriptRoot -Parent), [switch]$NoBuild)
$ErrorActionPreference = 'Stop'
Set-Location $ProjectRoot
$targetDir = Join-Path $ProjectRoot 'target\package'
if (-not $NoBuild) {
    $env:TMP = Join-Path $ProjectRoot 'target\tmp'; $env:TEMP = $env:TMP
    New-Item -ItemType Directory -Force $env:TMP | Out-Null
    $env:CARGO_TARGET_DIR = $targetDir
    $remap = @(
        "--remap-path-prefix=$(Join-Path $env:USERPROFILE '.cargo')=cargo",
        "--remap-path-prefix=$(Join-Path $env:USERPROFILE '.rustup')=rustup",
        "--remap-path-prefix=$ProjectRoot=windowsRICE",
        "--remap-path-prefix=$env:USERPROFILE=home",
        '-C', 'link-arg=/PDBALTPATH:%_PDB%'
    )
    $env:RUSTFLAGS = $remap -join ' '
    # Cargo reports progress on stderr; only its exit code decides.
    $prev = $ErrorActionPreference; $ErrorActionPreference = 'Continue'
    & cargo build --release -p windowsrice-panel 2>&1 | ForEach-Object { "$_" } | Select-Object -Last 2
    $code = $LASTEXITCODE
    $ErrorActionPreference = $prev
    Remove-Item Env:RUSTFLAGS, Env:CARGO_TARGET_DIR
    if ($code -ne 0) { throw 'release build failed' }
}
$exe = Join-Path $targetDir 'release\windowsrice-panel.exe'
if (-not (Test-Path $exe)) { throw "no package build at $exe (run without -NoBuild)" }
$version = (Get-Item $exe).VersionInfo.ProductVersion
if (-not $version) { throw 'the executable has no version information (is rc.exe available?)' }
# The builder's user name must not appear in what ships.
$user = Split-Path $env:USERPROFILE -Leaf
$text = [Text.Encoding]::ASCII.GetString([IO.File]::ReadAllBytes($exe))
if ($text -match [regex]::Escape("\Users\$user")) { throw "the executable still contains a local path (\Users\$user)" }
$name = "windowsRICE-$version"
$dist = Join-Path $ProjectRoot 'target\dist'
$stage = Join-Path $dist $name
if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
New-Item -ItemType Directory -Force $stage | Out-Null
Copy-Item $exe $stage
Copy-Item 'packaging\install.ps1', 'packaging\uninstall.ps1', 'packaging\INSTALL.txt' $stage
Copy-Item 'assets\windowsrice.ico' $stage
$zip = Join-Path $dist "$name-win-x64.zip"
if (Test-Path $zip) { Remove-Item -Force $zip }
Compress-Archive -Path (Join-Path $stage '*') -DestinationPath $zip
$hash = (Get-FileHash $zip -Algorithm SHA256).Hash
"$hash  $(Split-Path $zip -Leaf)" | Set-Content "$zip.sha256" -Encoding ascii
"package: $zip"
"size: $([math]::Round((Get-Item $zip).Length / 1KB)) KB; SHA-256 $hash"
