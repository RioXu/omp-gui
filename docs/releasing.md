# 发布与便携包

[返回 README](../README.md) · [验证说明](testing.md) · [变更记录](changelog.md)

当前版本为 v0.8.2。发布产物在 [GitHub Releases](https://github.com/RioXu/omp-gui/releases)，Windows x64 便携包完整解压后运行 `omp-gui.exe`，无需安装 Rust。本地模式仍需 OMP 和 Git。

ZIP 包包含程序、`remote/linux-x86_64/omp-gui-remote`、`remote/linux-aarch64/omp-gui-remote`、快速使用说明及许可文件。远端服务与 GUI 使用同一版本，首次连接会上传适用的预编译服务；保留整个 `remote` 目录。预编译 Linux 服务在 Ubuntu 24.04 构建，较旧 glibc 系统可使用自编译服务覆盖或源码回退。GUI 与 OMP 的依赖、配置和数据目录见使用说明。

每次发布附带 `SHA256SUMS`，在 PowerShell 中执行以下命令，与文件内对应 ZIP 的摘要比较：

```powershell
Get-FileHash .\omp-gui-0.8.2-windows-x86_64.zip -Algorithm SHA256
```

## 维护流程

1. 在 `main` 核对三个 Cargo 包的版本、README、使用说明、变更记录和 `docs/releases/v<版本>.md`，完成并提交全部源码及文档。测试记录要区分当前验证与历史实测。
2. 确认源分支与 origin 一致后创建对应 `v<版本>` 标签，推送 `main` 和标签。不要覆盖已发布标签。
3. [Release 工作流](../.github/workflows/release.yml) 在 Windows 检查格式、所有功能的 Clippy、默认测试及 release 构建，在 Linux x86_64／ARM64 检查并构建服务；全部成功后才打包发布。标签必须与根 Cargo 版本一致。
4. 工作流使用 [package-release.ps1](../scripts/package-release.ps1) 校验 PE／ELF 架构，组装便携包与校验文件，使用 GitHub Actions 自带令牌创建 Release；无需在开发机保存发布令牌。
5. 核对线上标签指向、附件名及 SHA-256，下载并解压检查包内容。Windows 程序未进行代码签名；GitHub 自动附带的源码压缩包不是便携程序包。

本地已有三个对应 release 二进制时，也可调用打包脚本：

```powershell
./scripts/package-release.ps1 -Binary target/release/omp-gui.exe -RemoteRoot artifacts/release-remote
```

`RemoteRoot` 下应有 `linux-x86_64/omp-gui-remote` 和 `linux-aarch64/omp-gui-remote`；默认输出到 `artifacts/release`，若同名 ZIP 已存在则拒绝覆盖。标签说明使用对应的 `docs/releases/v<版本>.md`。
