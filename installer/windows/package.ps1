# Builds the Windows downloads from target\release into dist\:
#   AeternaVault-<version>-setup-x64.exe   installer
#   AeternaVault-<version>-windows-x64.zip portable (no installation)
param([string]$Version)

$ErrorActionPreference = 'Stop'
$root = Resolve-Path (Join-Path $PSScriptRoot '..\..')
Set-Location $root
if (-not $Version) {
    $Version = (Select-String -Path Cargo.toml -Pattern '^version = "(.+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
}
foreach ($exe in 'aeternavault.exe', 'aeternavault-cli.exe') {
    if (-not (Test-Path "target\release\$exe")) { throw "target\release\$exe is missing: run cargo build --release first" }
}
New-Item -ItemType Directory -Force dist | Out-Null

$iscc = @(
    (Get-Command iscc -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source),
    "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
    "$env:ProgramFiles\Inno Setup 6\ISCC.exe",
    "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe"
) | Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1
if (-not $iscc) { throw 'Inno Setup 6 (ISCC.exe) was not found' }
& $iscc /Q "/DAppVersion=$Version" installer\windows\AeternaVault.iss
if ($LASTEXITCODE -ne 0) { throw "ISCC failed with $LASTEXITCODE" }

$name = "AeternaVault-$Version-windows-x64"
$staging = Join-Path ([IO.Path]::GetTempPath()) $name
if (Test-Path $staging) { Remove-Item -Recurse -Force $staging }
New-Item -ItemType Directory -Force "$staging\powershell", "$staging\docs", "$staging\tools" | Out-Null
Copy-Item target\release\aeternavault.exe, target\release\aeternavault-cli.exe, README.md, LICENSE-MIT, LICENSE-APACHE $staging
Copy-Item -Recurse powershell\AeternaVault "$staging\powershell\"
Copy-Item docs\ENCRYPTION.md, docs\cli-windows.md "$staging\docs\"
Copy-Item tools\aeterna-decrypt.py "$staging\tools\"
$zip = "dist\$name.zip"
if (Test-Path $zip) { Remove-Item $zip }
Compress-Archive -Path "$staging\*" -DestinationPath $zip
Remove-Item -Recurse -Force $staging

Get-ChildItem dist -File | Where-Object Name -like "AeternaVault-$Version-*" | Select-Object Name, Length
