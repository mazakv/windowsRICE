# Generates packaging\THIRD-PARTY-NOTICES.txt: the license texts of every
# third-party crate that windowsrice-panel depends on for the Windows target
# (normal dependencies, including build-time procedural macros, so the list
# errs on the side of including too much). Texts are copied from each
# crate's own license files in the local Cargo registry; nothing is
# downloaded. For crates offered under a choice of licenses, windowsRICE
# uses them under the MIT license and the MIT text is reproduced; other
# license files a crate ships (for example Unicode-3.0) are included too.
#
#   pwsh -File scripts/third-party-notices.ps1          # write the file
#   pwsh -File scripts/third-party-notices.ps1 -Check   # fail if it is out of date
param([string]$ProjectRoot = (Split-Path $PSScriptRoot -Parent), [switch]$Check)
$ErrorActionPreference = 'Stop'
Set-Location $ProjectRoot
$out = Join-Path $ProjectRoot 'packaging\THIRD-PARTY-NOTICES.txt'

$tree = cargo tree -p windowsrice-panel -e normal --target x86_64-pc-windows-msvc --prefix none --format '{p}' --locked
if ($LASTEXITCODE -ne 0) { throw 'cargo tree failed' }
$wanted = @{}
foreach ($line in $tree) {
    if ($line -match '^(\S+) v(\S+)') {
        if ($Matches[1] -notlike 'windowsrice*') { $wanted["$($Matches[1]) $($Matches[2])"] = $true }
    }
}
$meta = cargo metadata --format-version 1 --locked --filter-platform x86_64-pc-windows-msvc | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) { throw 'cargo metadata failed' }
$pkgs = @($meta.packages | Where-Object { $wanted.ContainsKey("$($_.name) $($_.version)") } | Sort-Object name, version)
if ($pkgs.Count -ne $wanted.Count) { throw "found $($pkgs.Count) of $($wanted.Count) crates in cargo metadata" }

$sb = [Text.StringBuilder]::new()
[void]$sb.AppendLine('windowsRICE third-party notices')
[void]$sb.AppendLine('================================')
[void]$sb.AppendLine('')
[void]$sb.AppendLine('windowsRICE is licensed under the MIT License (see LICENSE). It is built')
[void]$sb.AppendLine('with the following third-party Rust crates, each under its own license.')
[void]$sb.AppendLine('Where a crate offers a choice of licenses, windowsRICE uses it under the')
[void]$sb.AppendLine('MIT License. The license texts below are copied from each crate.')
[void]$sb.AppendLine('')
foreach ($p in $pkgs) { [void]$sb.AppendLine(("  {0} {1}  ({2})" -f $p.name, $p.version, $p.license)) }
foreach ($p in $pkgs) {
    $dir = Split-Path $p.manifest_path
    $files = @(Get-ChildItem $dir -File | Where-Object { $_.Name -match '^(LICEN[CS]E|COPYING|UNLICENSE)' })
    # A choice that includes MIT: reproduce the MIT text (and any other
    # mandatory notice such as Unicode), not the alternatives.
    if ($p.license -match 'MIT' -and $p.license -match ' OR |/') {
        $mit = @($files | Where-Object { $_.Name -match 'MIT' })
        $extra = @($files | Where-Object { $_.Name -match 'UNICODE' })
        if ($mit.Count) { $files = $mit + $extra }
    }
    [void]$sb.AppendLine('')
    [void]$sb.AppendLine(('-' * 78))
    [void]$sb.AppendLine(("{0} {1}" -f $p.name, $p.version))
    [void]$sb.AppendLine(("License: {0}" -f $p.license))
    if ($p.repository) { [void]$sb.AppendLine(("Source: {0}" -f $p.repository)) }
    [void]$sb.AppendLine(('-' * 78))
    if ($files.Count -eq 0) {
        [void]$sb.AppendLine(("The crate ships no license file; its declared license is {0}." -f $p.license))
        continue
    }
    foreach ($f in $files) {
        [void]$sb.AppendLine('')
        [void]$sb.AppendLine((Get-Content $f.FullName -Raw).Replace("`r`n", "`n").TrimEnd())
    }
}
$text = $sb.ToString().Replace("`r`n", "`n")
if ($Check) {
    $have = if (Test-Path $out) { (Get-Content $out -Raw).Replace("`r`n", "`n") } else { '' }
    if ($have -ne $text) { Write-Output 'THIRD-PARTY-NOTICES.txt is out of date: run scripts/third-party-notices.ps1'; exit 1 }
    Write-Output "third-party notices up to date ($($pkgs.Count) crates)"
    exit 0
}
[IO.File]::WriteAllText($out, $text, [Text.UTF8Encoding]::new($false))
Write-Output "wrote $out ($($pkgs.Count) crates)"
