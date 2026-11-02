# 发布与安装包

[返回 README](../README.md) · [验证说明](testing.md) · [变更记录](changelog.md)

当前版本为 v0.8.3。[GitHub Releases](https://github.com/RioXu/omp-gui/releases) 提供单个 Windows x64 安装程序 `omp-gui-0.8.3-windows-x86_64-setup.exe` 和 `SHA256SUMS`。支持 Windows 10 1809 或更新版本及 Windows 11 的 x64 系统；不发布 ARM64 安装包。

安装程序使用 Inno Setup，默认安装到 `%LOCALAPPDATA%\Programs\OMP GUI`，创建开始菜单快捷方式，可选择桌面快捷方式；支持覆盖升级和系统“已安装的应用”中的卸载。会话及配置仍保存在 `%LOCALAPPDATA%\OMP GUI`，升级和卸载均保留。安装程序不安装 OMP、Git 或 Rust，不附带 runtime DLL；缺少 `VCRUNTIME140.dll` 时，需安装[微软官方 x64 运行库](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist)。

安装内容包括 GUI、`remote/linux-x86_64/omp-gui-remote`、快速使用说明和许可文件。Linux 服务在 Ubuntu 24.04 构建，与 GUI 使用相同版本及管理协议；首次 SSH 连接由 GUI 上传部署。较旧 glibc 系统可自编译服务或使用源码回退。

下载后用以下命令，与 `SHA256SUMS` 中的摘要比较：

```powershell
Get-FileHash .\omp-gui-0.8.3-windows-x86_64-setup.exe -Algorithm SHA256
```

## 维护流程

1. 在 `main` 更新三个 Cargo 包及锁文件版本，准备 README、变更记录和 `docs/releases/v<版本>.md`，完成验证后提交并推送。不要覆盖已发布的标签。
2. 修改根 `Cargo.toml` 的推送触发 [Release 工作流](../.github/workflows/release.yml)。若当前版本标签已存在，跳过发布；也可推送新版本标签或手动运行工作流。标签与三个 Cargo 包版本必须一致。
3. Windows 执行格式检查、全功能 Clippy、默认测试和 release 构建；Linux x86_64 执行共享核心／服务检查、测试和 release 构建。两端均成功后才打包。
4. [install-inno.ps1](../scripts/install-inno.ps1) 从官方发行页面获取固定版本编译器并校验 SHA-256 和数字签名。[package-release.ps1](../scripts/package-release.ps1) 校验 PE／ELF 架构，组装安装内容并生成安装程序及校验文件。
5. [verify-installer.ps1](../scripts/verify-installer.ps1) 在临时目录验证安装、覆盖升级、GUI／服务文件哈希、许可文件及卸载。验证要求该应用 ID 尚未安装，避免覆盖用户的现有安装；静默验证不创建快捷方式、不启动 GUI。
6. 工作流在经过验证的提交上创建版本标签和 Release，上传一个 `*-setup.exe` 及 `SHA256SUMS`。核对线上标签和附件。Windows 程序与安装包尚未代码签名；GitHub 自动附带的源码压缩包不是安装程序。

## 本地打包

先准备两个**同版本、同协议**的 release 二进制，再执行：

```powershell
$compiler = ./scripts/install-inno.ps1
./scripts/package-release.ps1 -Binary target/release/omp-gui.exe -RemoteRoot artifacts/release-remote -InnoCompiler $compiler
```

`RemoteRoot` 下需有 `linux-x86_64/omp-gui-remote`；默认输出到 `artifacts/release`，拒绝覆盖同名安装包。也可通过 `-InnoCompiler` 指定已有 Inno Setup 6.5.4 编译器。
