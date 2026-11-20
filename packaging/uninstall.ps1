# windowsRICE per-user uninstaller.
#
#   powershell -ExecutionPolicy Bypass -File uninstall.ps1 [-RemoveSettings [-Yes]]
#
# Closes a running windowsRICE, removes its shortcuts, the Apps & features
# entry and the files it installed. Files of your own in the install folder
# are left alone (the folder is then kept). Settings in %APPDATA%\windowsRICE
# (config.toml, custom themes and presets) are kept unless -RemoveSettings
# is given and confirmed: interactively by typing YES, or with -Yes.
param(
    [string]$InstallDir = $PSScriptRoot,
    [string]$SettingsDir = (Join-Path $env:APPDATA 'windowsRICE'),
    [switch]$RemoveSettings,
    [switch]$Yes,
    [switch]$NoRegister
)
$ErrorActionPreference = 'Stop'
$exeName = 'windowsrice-panel.exe'
$own = @($exeName, 'uninstall.ps1', 'windowsrice.ico', 'INSTALL.txt', 'install.json')
$exe = Join-Path $InstallDir $exeName
if (-not (Test-Path $exe)) { throw "No windowsRICE installation in $InstallDir." }
if ($RemoveSettings) {
    # Only windowsRICE's own settings folder may be removed.
    if ((Split-Path $SettingsDir -Leaf) -ne 'windowsRICE') { throw "Refusing to remove ${SettingsDir}: it is not a windowsRICE settings folder." }
    if (-not $Yes) {
        $interactive = [Environment]::UserInteractive -and -not ([Environment]::GetCommandLineArgs() -match '^-NonInteractive$')
        if (-not $interactive) { throw 'Removing settings needs confirmation: run interactively or add -Yes. Nothing was changed.' }
        $answer = Read-Host "Also delete your windowsRICE settings, themes and presets in $SettingsDir? Type YES to confirm"
        if ($answer -cne 'YES') { throw 'Settings removal not confirmed. Nothing was changed.' }
    }
}
$info = @{}
$infoFile = Join-Path $InstallDir 'install.json'
if (Test-Path $infoFile) { $info = Get-Content $infoFile -Raw | ConvertFrom-Json }
$startMenu = if ($info.start_menu) { $info.start_menu } else { Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs' }
$startup = if ($info.startup) { $info.startup } else { Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\Startup' }

# Close a running windowsRICE from this install.
& $exe --quit | Out-Null
$full = (Resolve-Path $exe).Path
for ($i = 0; $i -lt 50; $i++) {
    if (@(Get-Process windowsrice-panel -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $full }).Count -eq 0) { break }
    Start-Sleep -Milliseconds 100
}
if (@(Get-Process windowsrice-panel -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $full }).Count -gt 0) {
    throw 'windowsRICE is still running; close it and run the uninstaller again. Nothing was changed.'
}

# Shortcuts that point to this install.
$shell = New-Object -ComObject WScript.Shell
foreach ($l in (Join-Path $startMenu 'windowsRICE.lnk'), (Join-Path $startMenu 'windowsRICE Design Studio.lnk'), (Join-Path $startup 'windowsRICE.lnk')) {
    if ((Test-Path $l) -and ($shell.CreateShortcut($l).TargetPath -eq $full)) { Remove-Item -Force $l }
}
if (-not $NoRegister -and $info.registered -ne $false) {
    $key = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\windowsRICE'
    if (Test-Path $key) { Remove-Item -Recurse -Force $key }
}
# Only windowsRICE's own files; the folder goes only when nothing else is left.
Set-Location ([IO.Path]::GetTempPath())
foreach ($f in $own) { $p = Join-Path $InstallDir $f; if (Test-Path $p) { Remove-Item -Force $p } }
$left = @(Get-ChildItem -Force $InstallDir -ErrorAction SilentlyContinue)
if ($left.Count -eq 0) { Remove-Item -Force $InstallDir } else { "Kept ${InstallDir}: it contains files that are not part of windowsRICE." }
if ($RemoveSettings) {
    if (Test-Path $SettingsDir) { Remove-Item -Recurse -Force $SettingsDir }
    "windowsRICE removed, including settings in $SettingsDir"
} else {
    "windowsRICE removed. Your settings are kept in $SettingsDir"
}
