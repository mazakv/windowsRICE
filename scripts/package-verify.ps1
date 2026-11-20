# Verifies the distributable zip (integrity) and the installer (behaviour)
# in a sandbox under target\pkgtest. The registry is never written: the
# Apps & features values are checked through -RegistryPlan. Refuses to run
# while any windowsRICE is running, so it can never close the user's bar.
param([string]$ProjectRoot = (Split-Path $PSScriptRoot -Parent))
$ErrorActionPreference = 'Stop'
Set-Location $ProjectRoot
if (@(Get-Process windowsrice-panel -ErrorAction SilentlyContinue).Count -gt 0) { throw 'a windowsRICE bar is running; close it before this test' }
$root = Join-Path $ProjectRoot 'target\pkgtest'
if (Test-Path $root) { Remove-Item -Recurse -Force $root }
New-Item -ItemType Directory -Force $root | Out-Null
$checks = [System.Collections.Generic.List[object]]::new()
function Check([string]$what, [bool]$ok, [string]$detail = '') { $checks.Add([pscustomobject]@{ check = $what; pass = $ok; detail = $detail }) }
Add-Type -Namespace WPV -Name Shell -MemberDefinition '[DllImport("shell32.dll", CharSet=CharSet.Unicode)] public static extern uint ExtractIconExW(string file, int index, IntPtr[] large, IntPtr[] small, uint n);'

# ================================================================ package integrity
$expectedVersion = ([regex]::Match((Get-Content Cargo.toml -Raw), '(?m)^version = "([^"]+)"')).Groups[1].Value
$zip = Get-Item (Join-Path $ProjectRoot "target\dist\windowsRICE-$expectedVersion-win-x64.zip") -ErrorAction SilentlyContinue
Check 'package zip for the workspace version exists' ($null -ne $zip) "windowsRICE-$expectedVersion-win-x64.zip"
$line = (Get-Content "$($zip.FullName).sha256" -Raw).Trim()
$sum, $named = $line -split '\s+', 2
Check 'checksum file names the zip and matches it' ($named -eq $zip.Name -and $sum -eq (Get-FileHash $zip.FullName -Algorithm SHA256).Hash) $line
$pkg = Join-Path $root 'extracted'; Expand-Archive $zip.FullName $pkg
$names = @((Get-ChildItem -Recurse -File $pkg).Name | Sort-Object)
$allowed = @('INSTALL.txt', 'install.ps1', 'uninstall.ps1', 'windowsrice-panel.exe', 'windowsrice.ico') | Sort-Object
Check 'zip holds exactly the release files (no captures, configs or development files)' ((Compare-Object $names $allowed) -eq $null) ($names -join ', ')
$exePkg = Join-Path $pkg 'windowsrice-panel.exe'
$v = (Get-Item $exePkg).VersionInfo
Check 'executable version information' ($v.ProductName -eq 'windowsRICE' -and $v.ProductVersion -eq $expectedVersion -and $v.FileDescription -eq 'windowsRICE Bar') "$($v.ProductName) $($v.ProductVersion) '$($v.FileDescription)'"
$icons = [WPV.Shell]::ExtractIconExW($exePkg, -1, $null, $null, 0)
Check 'executable has the application icon' ($icons -ge 1) "icons: $icons"
$icoSame = (Get-FileHash (Join-Path $pkg 'windowsrice.ico')).Hash -eq (Get-FileHash 'assets\windowsrice.ico').Hash
Check 'icon file matches assets\windowsrice.ico' $icoSame
$user = Split-Path $env:USERPROFILE -Leaf
$leaks = foreach ($f in Get-ChildItem -File $pkg) {
    $t = [Text.Encoding]::ASCII.GetString([IO.File]::ReadAllBytes($f.FullName)) + [Text.Encoding]::Unicode.GetString([IO.File]::ReadAllBytes($f.FullName))
    if ($t -match [regex]::Escape($user) -or $t -match '(?i)password|secret|api[_-]?key|BEGIN [A-Z ]*PRIVATE KEY') { $f.Name }
}
Check 'no user name, local paths or secrets in any shipped file' (@($leaks).Count -eq 0) (@($leaks) -join ', ')
$exeHash = (Get-FileHash $exePkg).Hash -eq (Get-FileHash (Join-Path $ProjectRoot 'target\package\release\windowsrice-panel.exe')).Hash
Check 'shipped executable is the package build' $exeHash

# ================================================================ installer behaviour
$dirs = @{ InstallDir = Join-Path $root 'Programs\windowsRICE'; StartMenuDir = Join-Path $root 'StartMenu'; StartupDir = Join-Path $root 'Startup' }
$settings = Join-Path $root 'AppData\windowsRICE'
$plan = Join-Path $root 'registry-plan.json'
function Run([string]$script, [string[]]$more) {
    $a = @('-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', $script) + $more
    # Expected refusals arrive on stderr; only the exit code decides.
    $prev = $ErrorActionPreference; $ErrorActionPreference = 'Continue'
    $out = & powershell.exe @a 2>&1 | ForEach-Object { "$_" } | Out-String
    $code = $LASTEXITCODE
    $ErrorActionPreference = $prev
    [pscustomobject]@{ ok = ($code -eq 0); out = $out.Trim() }
}
function Install([string[]]$more = @()) { Run (Join-Path $pkg 'install.ps1') (@('-InstallDir', $dirs.InstallDir, '-StartMenuDir', $dirs.StartMenuDir, '-StartupDir', $dirs.StartupDir, '-NoRegister', '-RegistryPlan', $plan) + $more) }
function Uninstall([string[]]$more = @()) { Run (Join-Path $dirs.InstallDir 'uninstall.ps1') (@('-SettingsDir', $settings, '-NoRegister') + $more) }
function Target([string]$lnk) { $s = (New-Object -ComObject WScript.Shell).CreateShortcut($lnk); "$($s.TargetPath)|$($s.Arguments)" }
$exe = Join-Path $dirs.InstallDir 'windowsrice-panel.exe'
$startupLnk = Join-Path $dirs.StartupDir 'windowsRICE.lnk'

# fresh install from the extracted zip
$r = Install
Check 'fresh install from the extracted zip' ($r.ok -and (Test-Path $exe)) $r.out
Check 'Start menu shortcuts point to the installed exe' ((Target (Join-Path $dirs.StartMenuDir 'windowsRICE.lnk')) -eq "$exe|" -and (Target (Join-Path $dirs.StartMenuDir 'windowsRICE Design Studio.lnk')) -eq "$exe|--studio")
Check 'no Startup shortcut unless asked' (-not (Test-Path $startupLnk))
$reg = Get-Content $plan -Raw | ConvertFrom-Json
Check 'Apps & features values (planned)' ($reg.DisplayName -eq 'windowsRICE' -and $reg.DisplayVersion -eq $expectedVersion -and $reg.UninstallString -match [regex]::Escape((Join-Path $dirs.InstallDir 'uninstall.ps1')) -and $reg.InstallLocation -eq $dirs.InstallDir -and $reg.NoModify -eq 1) "$($reg.DisplayName) $($reg.DisplayVersion)"

# the installed exe runs; upgrade while it runs
New-Item -ItemType Directory -Force $settings | Out-Null
$cfg = Join-Path $settings 'config.toml'; Set-Content $cfg "schema_version = 3`n[bar]`nspacing = 6`n" -Encoding utf8
$stats = Join-Path $root 'run.json'
$p = Start-Process $exe -ArgumentList @('--config', $cfg, '--stats', $stats, '--no-hotkey', '--exit-after', '60', '--title', '"Package test"') -PassThru
Start-Sleep -Seconds 4
Set-Content (Join-Path $dirs.InstallDir 'my-notes.txt') 'user file' -Encoding utf8
$r = Install @('-StartWithWindows')
$null = $p.WaitForExit(5000)
$j = Get-Content $stats -Raw | ConvertFrom-Json
Check 'upgrade closes the running bar cleanly (--quit)' ($r.ok -and $p.HasExited -and $j.exit_reason -eq '--quit') "exit_reason=$($j.exit_reason)"
Check 'startup opt-in creates the Startup shortcut' ((Target $startupLnk) -eq "$exe|")
Check 'upgrade keeps settings and the user''s own files' (((Get-Content $cfg -Raw) -match 'spacing = 6') -and (Test-Path (Join-Path $dirs.InstallDir 'my-notes.txt')))
$r = Install
Check 'upgrade keeps the start-with-Windows choice' ($r.ok -and (Test-Path $startupLnk))
$r = Install @('-NoStartWithWindows')
Check 'startup opt-out removes the Startup shortcut' ($r.ok -and -not (Test-Path $startupLnk))

# interrupted install: crash after moving the old install aside
Rename-Item $dirs.InstallDir 'windowsRICE.old'
New-Item -ItemType Directory (Join-Path $root 'Programs\windowsRICE.new') | Out-Null
$r = Install
Check 'interrupted install is restored and completed' ($r.ok -and (Test-Path $exe) -and ($r.out -match 'Restored') -and (Test-Path (Join-Path $dirs.InstallDir 'my-notes.txt')) -and -not (Test-Path (Join-Path $root 'Programs\windowsRICE.old')) -and -not (Test-Path (Join-Path $root 'Programs\windowsRICE.new'))) $r.out

# refuses a folder that is not windowsRICE's
$foreign = Join-Path $root 'Foreign'; New-Item -ItemType Directory $foreign | Out-Null; Set-Content (Join-Path $foreign 'keep.txt') 'x'
$r = Run (Join-Path $pkg 'install.ps1') @('-InstallDir', $foreign, '-StartMenuDir', (Join-Path $root 'SM2'), '-StartupDir', (Join-Path $root 'SU2'), '-NoRegister')
Check 'refuses to install into an unrelated non-empty folder' (-not $r.ok -and (Test-Path (Join-Path $foreign 'keep.txt')) -and -not (Test-Path (Join-Path $foreign 'windowsrice-panel.exe'))) ($r.out -split "`n" | Select-Object -First 1)
$r = Run (Join-Path $pkg 'install.ps1') @('-InstallDir', $env:LOCALAPPDATA, '-NoRegister', '-StartMenuDir', (Join-Path $root 'SM3'), '-StartupDir', (Join-Path $root 'SU3'))
Check 'refuses a profile root as install folder' (-not $r.ok) ($r.out -split "`n" | Select-Object -First 1)

# settings removal needs confirmation
$r = Uninstall @('-RemoveSettings')
Check 'removing settings without confirmation is refused (nothing changed)' (-not $r.ok -and (Test-Path $exe) -and (Test-Path $cfg)) ($r.out -split "`n" | Select-Object -First 1)
$wrong = Join-Path $root 'Documents'; New-Item -ItemType Directory $wrong | Out-Null
$r = Run (Join-Path $dirs.InstallDir 'uninstall.ps1') @('-SettingsDir', $wrong, '-NoRegister', '-RemoveSettings', '-Yes')
Check 'refuses to remove a folder that is not windowsRICE settings' (-not $r.ok -and (Test-Path $wrong) -and (Test-Path $exe))

# uninstall keeps settings and the user's own files
$r = Uninstall
Check 'uninstall removes windowsRICE files and its shortcuts' ($r.ok -and -not (Test-Path $exe) -and -not (Test-Path (Join-Path $dirs.StartMenuDir 'windowsRICE.lnk'))) $r.out
Check 'uninstall keeps settings by default' ((Get-Content $cfg -Raw) -match 'spacing = 6')
Check 'uninstall keeps the user''s own files (and their folder)' (Test-Path (Join-Path $dirs.InstallDir 'my-notes.txt'))
Remove-Item -Force (Join-Path $dirs.InstallDir 'my-notes.txt')

# repeated install and uninstall
$ok = $true
foreach ($i in 1..3) {
    $a = Install; $b = Uninstall
    $ok = $ok -and $a.ok -and $b.ok -and -not (Test-Path $dirs.InstallDir) -and (Test-Path $cfg)
}
Check 'three install and uninstall cycles leave no install and keep settings' $ok

# confirmed settings removal
$null = Install
$r = Uninstall @('-RemoveSettings', '-Yes')
Check 'confirmed -RemoveSettings removes only the settings folder' ($r.ok -and -not (Test-Path $settings) -and (Test-Path $wrong) -and -not (Test-Path $dirs.InstallDir)) $r.out
Check 'nothing left running' (@(Get-Process windowsrice-panel -ErrorAction SilentlyContinue).Count -eq 0)

$checks | Format-Table -AutoSize -Wrap | Out-String -Width 220
$checks | ConvertTo-Json | Set-Content (Join-Path $root 'checks.json') -Encoding utf8
$failed = @($checks | Where-Object { -not $_.pass }).Count
"{0} checks, {1} failed" -f $checks.Count, $failed
exit [int]($failed -gt 0)
