# windowsRICE per-user installer (no administrator rights).
#
#   powershell -ExecutionPolicy Bypass -File install.ps1 [-StartWithWindows | -NoStartWithWindows] [-Launch]
#
# Installs the files next to this script into %LOCALAPPDATA%\Programs\
# windowsRICE, adds Start menu shortcuts and an "Apps & features" entry for
# the current user, and optionally a shortcut in the user's Startup folder.
#
# Safety:
# - Settings in %APPDATA%\windowsRICE are never touched.
# - A running windowsRICE from this install is closed first (`--quit`).
# - New files are staged, then swapped in with renames. If an earlier run
#   was interrupted between the renames, the previous install is restored
#   before anything else happens.
# - Only windowsRICE's own files are replaced; other files a user put in
#   the install folder are carried over.
# - The installer refuses folders that are not a windowsRICE install and
#   are not empty, and drive or profile roots.
#
# Every folder can be overridden (scripts/package-verify.ps1 tests in a
# sandbox). -NoRegister skips the Apps & features entry; -RegistryPlan
# writes the values that would be registered to a JSON file instead.
param(
    [string]$SourceDir = $PSScriptRoot,
    [string]$InstallDir = (Join-Path $env:LOCALAPPDATA 'Programs\windowsRICE'),
    [string]$StartMenuDir = (Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs'),
    [string]$StartupDir = (Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\Startup'),
    [switch]$StartWithWindows,
    [switch]$NoStartWithWindows,
    [switch]$NoRegister,
    [string]$RegistryPlan,
    [switch]$Launch
)
$ErrorActionPreference = 'Stop'
$exeName = 'windowsrice-panel.exe'
$own = @($exeName, 'uninstall.ps1', 'windowsrice.ico', 'INSTALL.txt', 'install.json')
if ($StartWithWindows -and $NoStartWithWindows) { throw 'Choose either -StartWithWindows or -NoStartWithWindows.' }
$src = Join-Path $SourceDir $exeName
if (-not (Test-Path $src)) { throw "windowsRICE files not found next to the installer ($src)." }
$version = (Get-Item $src).VersionInfo.ProductVersion
if (-not $version) { $version = '0.0.0' }

# Refuse dangerous targets.
$InstallDir = [IO.Path]::GetFullPath($InstallDir).TrimEnd('\')
$forbidden = @([IO.Path]::GetPathRoot($InstallDir).TrimEnd('\'), $env:USERPROFILE, $env:LOCALAPPDATA, $env:APPDATA, (Join-Path $env:LOCALAPPDATA 'Programs')) |
    Where-Object { $_ } | ForEach-Object { [IO.Path]::GetFullPath($_).TrimEnd('\') }
if ($forbidden -contains $InstallDir) { throw "Refusing to install directly into $InstallDir; choose a windowsRICE folder." }
$stage = "$InstallDir.new"; $old = "$InstallDir.old"

# 0. Recover from an interrupted earlier run.
if (-not (Test-Path $InstallDir) -and (Test-Path (Join-Path $old $exeName))) {
    Rename-Item $old (Split-Path $InstallDir -Leaf)
    "Restored the previous installation after an interrupted install."
}
if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
if (Test-Path $old) { Remove-Item -Recurse -Force $old }
if (Test-Path $InstallDir) {
    $present = @(Get-ChildItem -Force $InstallDir)
    if ($present.Count -gt 0 -and -not (Test-Path (Join-Path $InstallDir $exeName))) {
        throw "$InstallDir is not a windowsRICE installation and is not empty; nothing was changed."
    }
}

# 1. Close a running windowsRICE from this install (it releases its screen strip).
$installed = Join-Path $InstallDir $exeName
if (Test-Path $installed) {
    $full = (Resolve-Path $installed).Path
    & $installed --quit | Out-Null
    for ($i = 0; $i -lt 50; $i++) {
        $running = @(Get-Process windowsrice-panel -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $full })
        if ($running.Count -eq 0) { break }
        Start-Sleep -Milliseconds 100
    }
    if ($running.Count -gt 0) { throw "windowsRICE is still running from $InstallDir; close it and run the installer again. Nothing was changed." }
}

# 2. Stage the new files (plus any files of the user's in the old folder), then swap.
New-Item -ItemType Directory -Force (Split-Path $InstallDir -Parent) | Out-Null
New-Item -ItemType Directory $stage | Out-Null
foreach ($f in $own) {
    $p = Join-Path $SourceDir $f
    if ($f -ne 'install.json' -and (Test-Path $p)) { Copy-Item $p (Join-Path $stage $f) }
}
if (Test-Path $InstallDir) {
    Get-ChildItem -Force $InstallDir | Where-Object { $own -notcontains $_.Name } | ForEach-Object { Copy-Item -Recurse $_.FullName (Join-Path $stage $_.Name) }
}
$startupLink = Join-Path $StartupDir 'windowsRICE.lnk'
$startup = if ($StartWithWindows) { $true } elseif ($NoStartWithWindows) { $false } else { Test-Path $startupLink }
@{ version = $version; start_menu = $StartMenuDir; startup = $StartupDir; start_with_windows = $startup; registered = (-not $NoRegister.IsPresent) } |
    ConvertTo-Json | Set-Content (Join-Path $stage 'install.json') -Encoding utf8
if (Test-Path $InstallDir) { Rename-Item $InstallDir (Split-Path $old -Leaf) }
Rename-Item $stage (Split-Path $InstallDir -Leaf)
if (Test-Path $old) { Remove-Item -Recurse -Force $old }
$exe = Join-Path $InstallDir $exeName

# 3. Shortcuts.
$shell = New-Object -ComObject WScript.Shell
function New-Shortcut([string]$path, [string]$arguments, [string]$description) {
    New-Item -ItemType Directory -Force (Split-Path $path) | Out-Null
    $s = $shell.CreateShortcut($path)
    $s.TargetPath = $exe; $s.Arguments = $arguments; $s.WorkingDirectory = $InstallDir
    $s.Description = $description
    $icon = Join-Path $InstallDir 'windowsrice.ico'; if (Test-Path $icon) { $s.IconLocation = $icon }
    $s.Save()
}
New-Shortcut (Join-Path $StartMenuDir 'windowsRICE.lnk') '' 'windowsRICE bar'
New-Shortcut (Join-Path $StartMenuDir 'windowsRICE Design Studio.lnk') '--studio' 'windowsRICE Design Studio'
if ($startup) {
    New-Shortcut $startupLink '' 'windowsRICE bar'
} elseif (Test-Path $startupLink) {
    Remove-Item -Force $startupLink
}

# 4. Apps & features entry for the current user.
$size = [int]((Get-ChildItem $InstallDir -Recurse | Measure-Object Length -Sum).Sum / 1KB)
$values = [ordered]@{
    DisplayName = 'windowsRICE'; DisplayVersion = $version; Publisher = 'windowsRICE'
    InstallLocation = $InstallDir; DisplayIcon = (Join-Path $InstallDir 'windowsrice.ico')
    UninstallString = "powershell.exe -NoProfile -ExecutionPolicy Bypass -File `"$(Join-Path $InstallDir 'uninstall.ps1')`""
    NoModify = 1; NoRepair = 1; EstimatedSize = $size
}
if ($RegistryPlan) { $values | ConvertTo-Json | Set-Content $RegistryPlan -Encoding utf8 }
if (-not $NoRegister) {
    $key = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\windowsRICE'
    New-Item -Path $key -Force | Out-Null
    foreach ($k in $values.Keys) {
        if ($values[$k] -is [int]) { Set-ItemProperty -Path $key -Name $k -Value $values[$k] -Type DWord }
        else { Set-ItemProperty -Path $key -Name $k -Value $values[$k] }
    }
}
"windowsRICE $version installed in $InstallDir"
if ($Launch) { Start-Process $exe }
