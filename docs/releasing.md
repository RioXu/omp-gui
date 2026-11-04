# 发布与安装包

[返回 README](../README.md) · [验证说明](testing.md) · [变更记录](changelog.md)

当前版本为 v0.9.2。[GitHub Releases](https://github.com/RioXu/omp-gui/releases) 提供单个 Windows x64 安装程序 `omp-gui-0.9.2-windows-x86_64-setup.exe` 和 `SHA256SUMS`。支持 Windows 10 1809 或更新版本及 Windows 11 的 x64 系统；不发布 ARM64 安装包。

安装程序使用 Inno Setup，默认安装到 `%LOCALAPPDATA%\Programs\OMP GUI`，创建开始菜单快捷方式，可选择桌面快捷方式；支持覆盖升级和系统“已安装的应用”中的卸载。会话及配置仍保存在 `%LOCALAPPDATA%\OMP GUI`，升级和卸载均保留。安装程序不安装 OMP、Git 或 Rust，不附带 runtime DLL；缺少 `VCRUNTIME140.dll` 时，需安装[微软官方 x64 运行库](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist)。

安装内容包括 GUI、`remote/linux-x86_64/omp-gui-remote`、快速使用说明和许可文件。Linux 服务在 Ubuntu 24.04 构建，与 GUI 使用相同版本及管理协议；首次 SSH 连接由 GUI 上传部署。较旧 glibc 系统可自编译服务或使用源码回退。

下载后用以下命令，与 `SHA256SUMS` 中的摘要比较：

```powershell
Get-FileHash .\omp-gui-0.9.2-windows-x86_64-setup.exe -Algorithm SHA256
```

## 维护流程

1. 在 `main` 更新三个 Cargo 包及锁文件版本，准备 README、变更记录和 `docs/releases/v<版本>.md`，提交并同步到 GitHub。不要覆盖已发布的标签。
2. 在 Windows 本机执行 [publish-release.ps1](../scripts/publish-release.ps1)。脚本要求干净的 `main`，校验三个 Cargo 版本及 GitHub 提交一致，拒绝覆盖已有标签。GitHub CLI 须先通过 `gh auth login` 登录。
3. Windows 本机运行格式检查、全功能 Clippy、默认测试及 release 构建，复用本机缓存。Linux x86_64 服务继续在 GitHub Actions 的 Ubuntu 24.04 上检查、测试并构建，上传 `linux-server-x86_64` artifact，包含程序、源码提交、版本／协议及平台信息。
4. 脚本下载与本地 HEAD 相同提交的 Linux artifact，校验提交、版本及协议，拒绝使用旧服务。[install-inno.ps1](../scripts/install-inno.ps1) 获取并校验官方固定版本编译器，[package-release.ps1](../scripts/package-release.ps1) 在本机校验 PE／ELF 架构并生成安装包和 SHA-256。
5. 用相同安装内容及临时应用 ID 验证安装、覆盖升级、文件完整性和卸载，避免影响已安装的正式程序。验证不创建快捷方式、不启动 GUI。
6. 验证后在 GitHub 创建当前提交的版本标签，上传一个 `*-setup.exe` 和 `SHA256SUMS` 至草稿 Release，全部上传成功后公开。程序与安装包尚未代码签名；GitHub 自动附带的源码压缩包不是安装程序。

GitHub Actions 保留 [Checks](../.github/workflows/check.yml) 和轻量 Linux 服务构建，不构建 Windows 发布程序、不打包或上传 Release。推送代码或版本号不再自动发布安装包；本机无需 WSL。

```powershell
# 本地构建、验证并上传
./scripts/publish-release.ps1

# 只在本地准备安装包，不创建标签或上传
./scripts/publish-release.ps1 -PrepareOnly
```

## 本地打包

先准备两个**同版本、同协议**的 release 二进制，再执行：

```powershell
$compiler = ./scripts/install-inno.ps1
./scripts/package-release.ps1 -Binary target/release/omp-gui.exe -RemoteRoot artifacts/release-remote -InnoCompiler $compiler
```

`RemoteRoot` 下需有 `linux-x86_64/omp-gui-remote`；默认输出到 `artifacts/release`，拒绝覆盖同名安装包。也可通过 `-InnoCompiler` 指定已有 Inno Setup 6.5.4 编译器。
