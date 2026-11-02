param(
    [Parameter(Mandatory)][string]$Binary,
    [Parameter(Mandatory)][string]$RemoteRoot,
    [string]$OutputDirectory = 'artifacts/release',
    [string]$InnoCompiler = 'ISCC.exe',
    [switch]$PassThru
)
$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path $PSScriptRoot -Parent
$version = [regex]::Match((Get-Content (Join-Path $repoRoot 'Cargo.toml') -Raw), '(?m)^version = "([^"]+)"').Groups[1].Value
if (-not $version) { throw 'Cargo.toml version missing' }
$binaryPath = (Resolve-Path -LiteralPath $Binary).Path
$remotePath = (Resolve-Path -LiteralPath $RemoteRoot).Path
$destination = [IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Path $destination -Force | Out-Null
$packageName = "omp-gui-$version-windows-x86_64"
$installer = Join-Path $destination "$packageName-setup.exe"
if (Test-Path -LiteralPath $installer) { throw "Installer already exists: $installer" }
# Use a fresh staging directory; never delete a caller-supplied path.
$stage = Join-Path ([IO.Path]::GetTempPath()) "omp-payload-$([guid]::NewGuid().ToString('N').Substring(0,12))"
New-Item -ItemType Directory -Path $stage | Out-Null
$pe = [IO.File]::ReadAllBytes($binaryPath)
if ($pe.Length -lt 64 -or $pe[0] -ne 0x4d -or $pe[1] -ne 0x5a) { throw 'GUI is not a Windows executable' }
$peOffset = [BitConverter]::ToInt32($pe, 60)
if ($peOffset -lt 0 -or $peOffset + 6 -gt $pe.Length -or [BitConverter]::ToUInt32($pe, $peOffset) -ne 0x00004550 -or [BitConverter]::ToUInt16($pe, $peOffset + 4) -ne 0x8664) { throw 'GUI is not Windows x86_64' }
Copy-Item -LiteralPath $binaryPath -Destination (Join-Path $stage 'omp-gui.exe')
foreach ($target in @(@{Name='linux-x86_64'; Machine=62})) {
    $source = Join-Path $remotePath "$($target.Name)/omp-gui-remote"
    $elf = [IO.File]::ReadAllBytes($source)
    if ($elf.Length -lt 20 -or [BitConverter]::ToUInt32($elf, 0) -ne 0x464c457f -or [BitConverter]::ToUInt16($elf, 18) -ne $target.Machine) { throw "Invalid ELF: $source" }
    $remoteDestination = Join-Path $stage "remote/$($target.Name)"
    New-Item -ItemType Directory -Path $remoteDestination -Force | Out-Null
    Copy-Item -LiteralPath $source -Destination (Join-Path $remoteDestination 'omp-gui-remote')
}
Copy-Item -LiteralPath (Join-Path $repoRoot 'LICENSE') -Destination $stage
$licenses = Join-Path $stage 'licenses'
New-Item -ItemType Directory -Path $licenses | Out-Null
foreach ($license in @('OMP-LICENSE.txt', 'KaTeX-OFL.txt')) {
    Copy-Item -LiteralPath (Join-Path $repoRoot "resources/$license") -Destination $licenses
}
@"
OMP GUI v$version - Windows x64

安装后从开始菜单启动 OMP GUI，无需安装 Rust。
支持 Windows 10 1809 或更新版本的 x64 系统；安装包不附带运行库 DLL。
缺少 VCRUNTIME140.dll 时，从微软官方安装 Microsoft Visual C++ v14 x64 运行库：
https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist
本地模式需要 OMP 和 Git。远程模式需要 Windows OpenSSH、远端 OMP 和 Git。
保留 remote 目录，GUI 自动部署随包提供的 Linux x86_64 服务。
本版仅附带 Linux x86_64 服务，不提供未经实机验证的 ARM64 版本。
Linux 预编译服务面向 Ubuntu 24.04 同等或较新的 glibc 环境；旧系统可使用源码构建或自备服务。
首次 SSH 连接请先在终端完成主机身份确认和密钥解锁。
GUI 数据默认保存在 %LOCALAPPDATA%\OMP GUI，升级和卸载均保留原数据目录。

使用说明：https://github.com/RioXu/omp-gui/blob/v$version/docs/usage.md
远程连接：https://github.com/RioXu/omp-gui/blob/v$version/docs/remote.md
变更记录：https://github.com/RioXu/omp-gui/blob/v$version/docs/changelog.md
"@ | Set-Content -LiteralPath (Join-Path $stage 'README.txt') -Encoding utf8
& $InnoCompiler "/DAppVersion=$version" "/DPayloadDir=$stage" "/DOutputDir=$destination" (Join-Path $PSScriptRoot 'installer.iss') | Out-Host
if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath $installer)) { throw 'Installer compilation failed' }
$checksum = (Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash.ToLowerInvariant()
"$checksum  $packageName-setup.exe" | Set-Content -LiteralPath (Join-Path $destination 'SHA256SUMS') -Encoding ascii
if ($PassThru) {
    [pscustomobject]@{Installer=$installer; Payload=$stage; Checksums=(Join-Path $destination 'SHA256SUMS'); Version=$version}
} else {
    Write-Output $installer
    Write-Output "SHA256 $checksum"
}
