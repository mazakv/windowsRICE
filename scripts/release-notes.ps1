# Writes the GitHub Release notes for a version: the CHANGELOG.md section,
# installation steps, the SHA-256 of the zip, known limitations and the
# code-signing status. Fails if CHANGELOG.md has no section for the version.
#
#   pwsh -File scripts/release-notes.ps1 -Version 0.1.0-rc.2 -Zip <zip> -Out notes.md
param(
    [Parameter(Mandatory)][string]$Version,
    [Parameter(Mandatory)][string]$Zip,
    [Parameter(Mandatory)][string]$Out,
    [string]$ProjectRoot = (Split-Path $PSScriptRoot -Parent)
)
$ErrorActionPreference = 'Stop'
Set-Location $ProjectRoot
$log = (Get-Content 'CHANGELOG.md' -Raw).Replace("`r`n", "`n")
$m = [regex]::Match($log, "(?ms)^## \[$([regex]::Escape($Version))\][^\n]*\n(.*?)(?=^## \[|\z)")
if (-not $m.Success) { throw "CHANGELOG.md has no section for $Version" }
$changes = $m.Groups[1].Value.Trim()
$lim = [regex]::Match($log, '(?ms)^## Known limitations\s*\n(.*?)(?=^## |\z)')
$limits = if ($lim.Success) { $lim.Groups[1].Value.Trim() } else { throw 'CHANGELOG.md has no "Known limitations" section' }
$name = Split-Path $Zip -Leaf
$hash = (Get-FileHash $Zip -Algorithm SHA256).Hash
$pre = $Version -match '-'
$text = @"
windowsRICE $Version for Windows 10/11 x64$(if ($pre) { ' (pre-release: for testing, not a stable release)' }).

## Download and install

1. Download ``$name`` and ``$name.sha256``.
2. Check the download (PowerShell): ``(Get-FileHash .\$name -Algorithm SHA256).Hash`` must equal ``$hash``.
3. Extract the zip and run ``powershell -ExecutionPolicy Bypass -File .\install.ps1`` (per-user, no administrator rights). Add ``-StartWithWindows`` to start at sign-in.
4. To remove it, run ``uninstall.ps1`` from the install folder (settings are kept unless ``-RemoveSettings`` is given and confirmed).

Full instructions: ``INSTALL.txt`` in the zip.

**Code signing: not signed.** No code-signing certificate is used, so Windows SmartScreen may warn on first start. Verify the SHA-256 above before running.

## Changes

$changes

## Known limitations

$limits

## Licence

MIT (``LICENSE.txt``). Third-party licences: ``THIRD-PARTY-NOTICES.txt``.
"@
[IO.File]::WriteAllText($Out, $text.Replace("`r`n", "`n"), [Text.UTF8Encoding]::new($false))
Write-Output "notes for $Version written to $Out"
