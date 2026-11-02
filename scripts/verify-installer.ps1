param(
    [Parameter(Mandatory)][string]$Installer,
    [Parameter(Mandatory)][string]$Binary,
    [Parameter(Mandatory)][string]$RemoteBinary,
    [string]$InstallerAppId = 'RioXu.OMP.GUI'
)
$ErrorActionPreference = 'Stop'
$installerPath = (Resolve-Path -LiteralPath $Installer).Path
$registry = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\${InstallerAppId}_is1"
if (Test-Path -LiteralPath $registry) { throw 'Installer verification requires a fresh application ID; do not overwrite an existing installation' }
$root = Join-Path ([IO.Path]::GetTempPath()) "omp-install-qa-$([guid]::NewGuid().ToString('N'))"
$app = Join-Path $root 'app'
New-Item -ItemType Directory -Path $root | Out-Null
$sentinel = Join-Path $root 'user-data.txt'
Set-Content -LiteralPath $sentinel -Value 'preserve user data' -Encoding utf8
foreach ($step in @('install', 'upgrade')) {
    $process = Start-Process -FilePath $installerPath -ArgumentList @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/NOICONS', '/TASKS=""', "/DIR=`"$app`"", "/LOG=`"$root/$step.log`"") -WindowStyle Hidden -Wait -PassThru
    if ($process.ExitCode -ne 0) { throw "$step failed: $($process.ExitCode); logs: $root" }
    foreach ($pair in @(@{Installed='omp-gui.exe'; Source=$Binary}, @{Installed='remote/linux-x86_64/omp-gui-remote'; Source=$RemoteBinary})) {
        if ((Get-FileHash -LiteralPath (Join-Path $app $pair.Installed)).Hash -ne (Get-FileHash -LiteralPath $pair.Source).Hash) { throw "Installed file mismatch: $($pair.Installed)" }
    }
    foreach ($file in @('README.txt', 'LICENSE', 'licenses/OMP-LICENSE.txt', 'licenses/KaTeX-OFL.txt', 'unins000.exe')) {
        if (-not (Test-Path -LiteralPath (Join-Path $app $file))) { throw "Missing installed file: $file" }
    }
    if (-not (Test-Path -LiteralPath $registry)) { throw 'Missing uninstall registration' }
}
$uninstaller = Join-Path $app 'unins000.exe'
$process = Start-Process -FilePath $uninstaller -ArgumentList @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', "/LOG=`"$root/uninstall.log`"") -WindowStyle Hidden -Wait -PassThru
if ($process.ExitCode -ne 0) { throw "Uninstall failed: $($process.ExitCode); logs: $root" }
if (Test-Path -LiteralPath (Join-Path $app 'omp-gui.exe')) { throw 'Uninstall left the application executable' }
if (Test-Path -LiteralPath $registry) { throw 'Uninstall left its registration' }
if ((Get-Content -LiteralPath $sentinel -Raw).Trim() -ne 'preserve user data') { throw 'Uninstall modified user data' }
Write-Output "Install, upgrade and uninstall verified. Logs: $root"
