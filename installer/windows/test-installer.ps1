# Installs, updates and uninstalls AeternaVault on a CI machine and checks
# that settings survive the update and that nothing is left behind.
# Changes the system: run it only on a throw-away machine (GitHub runner).
#
#   installer\windows\test-installer.ps1 -Setup dist\AeternaVault-0.5.0-setup-x64.exe [-Previous old-setup.exe]
param(
    [Parameter(Mandatory)][string]$Setup,
    [string]$Previous
)

$ErrorActionPreference = 'Stop'
if ($env:CI -ne 'true') { throw 'This test changes the system; it only runs in CI.' }

$app = Join-Path $env:ProgramFiles 'AeternaVault'
$module = Join-Path $env:ProgramFiles 'WindowsPowerShell\Modules\AeternaVault'
$envKey = 'HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Environment'
$runKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
$settings = Join-Path $env:APPDATA 'AeternaVault\config.toml'

function Assert([bool]$Condition, [string]$What) {
    if (-not $Condition) { throw "FAILED: $What" }
    Write-Host "ok  $What"
}

function Install([string]$File) {
    $p = Start-Process $File -ArgumentList '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/SP-', "/LOG=$env:RUNNER_TEMP\setup.log" -Wait -PassThru
    if ($p.ExitCode -ne 0) { Get-Content "$env:RUNNER_TEMP\setup.log" -Tail 40; throw "setup exited with $($p.ExitCode)" }
}

function OnPath { ((Get-ItemProperty $envKey).Path -split ';') -contains $app }

if ($Previous) {
    Install $Previous
    Assert (Test-Path "$app\unins000.exe") 'previous version installed'
}

Install $Setup
Assert (Test-Path "$app\aeternavault.exe") 'window program installed'
Assert (Test-Path "$app\aeternavault-cli.exe") 'command line installed'
Assert (Test-Path "$module\AeternaVault.psd1") 'PowerShell module installed'
Assert (OnPath) 'program folder on PATH'
$version = & "$app\aeternavault-cli.exe" --version
Assert ($LASTEXITCODE -eq 0) "aeternavault-cli runs ($version)"

# Settings made with the installed version.
$dest = Join-Path $env:RUNNER_TEMP 'av-dest'
& "$app\aeternavault-cli.exe" destination set $dest | Out-Null
& "$app\aeternavault-cli.exe" config set advanced.hardlink_unchanged false | Out-Null
Assert (Test-Path $settings) 'settings written to the user profile'

# The module works from the machine-wide module folder.
$json = powershell -NoProfile -Command "Import-Module AeternaVault; (Get-AVDestination).destination"
Assert ($json -eq $dest) 'Import-Module AeternaVault finds the installed CLI'

# A window waiting in the notification area is closed by the update.
$window = Start-Process "$app\aeternavault.exe" -ArgumentList '--background' -PassThru
Start-Sleep -Seconds 6
$running = -not $window.HasExited
if (-not $running) { Write-Warning 'the window could not start on this machine; skipping the running-window check' }

# Update over the installed version.
Install $Setup
if ($running) {
    Start-Sleep -Seconds 2
    Assert ($window.HasExited) 'running window was closed for the update'
}
Assert ((& "$app\aeternavault-cli.exe" config get advanced.hardlink_unchanged) -eq 'false') 'settings kept after the update'
Assert (@(((Get-ItemProperty $envKey).Path -split ';') | Where-Object { $_ -eq $app }).Count -eq 1) 'PATH entry not duplicated'

# The window registers its startup entry; uninstalling must remove it.
& "$app\aeternavault-cli.exe" system background on | Out-Null

# Uninstall.
$p = Start-Process "$app\unins000.exe" -ArgumentList '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART' -Wait -PassThru
Assert ($p.ExitCode -eq 0) 'uninstaller finished'
Start-Sleep -Seconds 3
Assert (-not (Test-Path "$app\aeternavault.exe")) 'program files removed'
Assert (-not (Test-Path $module)) 'PowerShell module removed'
Assert (-not (OnPath)) 'PATH entry removed'
Assert (-not (Test-Path "$env:ProgramData\Microsoft\Windows\Start Menu\Programs\AeternaVault.lnk")) 'Start menu entry removed'
$run = Get-ItemProperty $runKey -ErrorAction SilentlyContinue
Assert (-not ($run -and ($run.PSObject.Properties.Name -contains 'AeternaVault'))) 'startup entry removed'
Assert (Test-Path $settings) 'settings kept by a silent uninstall'
Write-Host 'Installer checks passed.'
