# 工作约定

- 除非用户另有明确指示，直接在 `main` 分支工作，不创建其他分支。
- 优先使用 gpui-kit 控件和扩展接口；仅在其能力不足时实现自定义组件。
- 如果仓库根目录存在 `.codegraph/`，理解或定位代码时先使用 CodeGraph；未索引时不要自行建立索引。
- 若本地存在 `docs/remote-development-plan.md`，涉及远程 OMP 支持时先阅读，并在完成任务后更新进度、验证结果及下一步；该清单仅本地保留，不提交 Git。
- 发布采用本地构建、打包并上传的模式，使用 `scripts/publish-release.ps1`；Linux 服务在本机 WSL 构建，GitHub Actions 仅做代码检查，不执行发布。安装包不附带 runtime DLL，仅检测 Windows 版本和架构。
