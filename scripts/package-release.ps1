param(
    [Parameter(Mandatory)][string]$Binary,
    [Parameter(Mandatory)][string]$RemoteRoot,
    [string]$OutputDirectory = 'artifacts/release'
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
$archive = Join-Path $destination "$packageName.zip"
if (Test-Path -LiteralPath $archive) { throw "Archive already exists: $archive" }
# Use a fresh staging directory; never delete a caller-supplied path.
$stage = Join-Path $destination "$packageName-$([guid]::NewGuid().ToString('N'))"
New-Item -ItemType Directory -Path $stage | Out-Null
$pe = [IO.File]::ReadAllBytes($binaryPath)
if ($pe.Length -lt 64 -or $pe[0] -ne 0x4d -or $pe[1] -ne 0x5a) { throw 'GUI is not a Windows executable' }
$peOffset = [BitConverter]::ToInt32($pe, 60)
if ($peOffset -lt 0 -or $peOffset + 6 -gt $pe.Length -or [BitConverter]::ToUInt32($pe, $peOffset) -ne 0x00004550 -or [BitConverter]::ToUInt16($pe, $peOffset + 4) -ne 0x8664) { throw 'GUI is not Windows x86_64' }
Copy-Item -LiteralPath $binaryPath -Destination (Join-Path $stage 'omp-gui.exe')
foreach ($target in @(@{Name='linux-x86_64'; Machine=62}, @{Name='linux-aarch64'; Machine=183})) {
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
OMP GUI v$version - Windows x64 portable

完整解压后运行 omp-gui.exe，无需安装 Rust。
本地模式需要 OMP 和 Git。远程模式需要 Windows OpenSSH、远端 OMP 和 Git。
保留 remote 目录，GUI 按远端 Linux 架构自动部署相应服务。
Linux 预编译服务面向 Ubuntu 24.04 同等或较新的 glibc 环境；旧系统可使用源码构建或自备服务。
首次 SSH 连接请先在终端完成主机身份确认和密钥解锁。
GUI 数据默认保存在 %LOCALAPPDATA%\OMP GUI，升级时保留原数据目录。

使用说明：https://github.com/RioXu/omp-gui/blob/v$version/docs/usage.md
远程连接：https://github.com/RioXu/omp-gui/blob/v$version/docs/remote.md
变更记录：https://github.com/RioXu/omp-gui/blob/v$version/docs/changelog.md
"@ | Set-Content -LiteralPath (Join-Path $stage 'README.txt') -Encoding utf8
Compress-Archive -Path (Join-Path $stage '*') -DestinationPath $archive -CompressionLevel Optimal
$checksum = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
"$checksum  $packageName.zip" | Set-Content -LiteralPath (Join-Path $destination 'SHA256SUMS') -Encoding ascii
Write-Output $archive
Write-Output "SHA256 $checksum"
