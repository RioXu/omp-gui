# OMP GUI

基于 Rust / gpui-kit 0.8.1 的 Windows 原生 OMP 客户端。支持会话、代码选区评论、文件与变更标签、当前标签查找，以及 OMP 模型、服务和子智能体配置。界面不使用 WebView。

本文对应 v0.9.5 源码，核对于 2026-11-06。构建版本以 [Cargo.toml](Cargo.toml) 为准。

v0.9.5 将长过程记录和工具文本改为可视区域渲染，完善底部跟随、ask 自由回复显示、追加输入取消及历史输入恢复。消息历史仍每页最多 100 条，过程记录连续滚动，不分页。完整改动见[变更记录](docs/changelog.md)，验证范围见[测试说明](docs/testing.md)。

## 快速开始

桌面端需要 Windows。本地模式需要本机 OMP 和 Git；远程模式使用 OpenSSH 连接 Linux，在远端运行 OMP 和 Git。源码构建另需 Rust MSVC 工具链、Visual Studio C++ Build Tools 和 Windows SDK。本次发布仅包含 Windows x64 和 Linux x86_64，ARM64 未经实机验证，不提供发布包。

可从 [Releases](https://github.com/RioXu/omp-gui/releases) 下载单个 Windows x64 `*-setup.exe` 安装包，安装后从开始菜单启动。支持 Windows 10 1809 或更新版本及 Windows 11 x64；内含 Linux x86_64 服务，首次 SSH 连接自动部署，不需要本机 Rust。安装程序不附带 runtime DLL；缺少 `VCRUNTIME140.dll` 时需安装[微软官方 x64 运行库](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist)。升级和卸载保留会话数据。校验、打包及发布流程见[发布说明](docs/releasing.md)。

```powershell
cargo build --locked
cargo run -- --project "D:\dev\your-project"
```

也可直接运行构建后的 `target/debug/omp-gui.exe`。悬停“项目”标题可添加目录，悬停项目行可新建会话；新会话直接修改项目目录，不创建分支或工作树。已有旧工作树会话继续使用原执行位置。

```powershell
cargo run -- --demo
cargo run -- --data-dir ".\target\gui-data" --project "D:\dev\your-project"
cargo run -- --probe --data-dir ".\target\probe-data"
```

演示模式不发送模型任务；设置功能仍依赖本机 OMP。`--probe` 使用 PATH 中的 `omp` 检查协议握手，不发送提示词。发布优化构建及 Windows 环境问题见[构建与验证](docs/testing.md)。

左侧顶部的连接选择器可新增 SSH 连接，例如 `unraid-Ubtuntu`，并切换本地／远程环境。OMP 默认从各自用户环境查找，也支持路径覆盖；两种模式共用组件式目录选择器。远端文件按需读取和保存，不复制整份项目。连接中断或关闭 GUI 后，远端运行中的任务继续由监督服务管理，重连后恢复状态；远端服务或主机重启不保证工具继续运行。部署、profile 和限制见[远程 Linux 使用说明](docs/remote.md)。

## 主要操作

- `Ctrl+Enter` 发送，运行中的追加输入默认排队；`@` 引用文件，`/` 补全命令，支持拖入文件和粘贴图片。
- 多个会话可并行运行；项目会话与同一 OMP profile 的终端记录同步。
- 文件和变更在独立标签页打开；普通文件可编辑、`Ctrl+S` 保存，并保护未保存的修改。
- 工作面板提供变更、文件和子智能体；子智能体点击后打开只读会话标签。
- 主会话和子智能体按页展示历史，`Ctrl+F` 搜索当前页的正文和思考内容；文件及变更搜索当前标签全文。范围与边界见[历史分页与搜索](docs/usage.md#历史分页与搜索)。
- 回复支持公式、代码选区评论及现存文件路径链接；右上角可选择皮肤。
- 设置支持模型服务、子智能体定义及角色模型关联，使用对应环境的原生 OMP 配置。

快捷键、任务队列、通知、阅读行为和配置操作统一见[使用与配置](docs/usage.md)。

## 文档导航

| 文档 | 内容 |
| --- | --- |
| [使用与配置](docs/usage.md) | 快捷键、队列、评论、文件标签、命令目标、模型与子智能体表单、数据范围及限制 |
| [组件职责与维护约定](docs/architecture.md) | 模块边界、依赖方向、状态归属和行为约定 |
| [远程 Linux](docs/remote.md) | SSH、自动部署、环境与 profile、远端项目及断线恢复 |
| [构建、测试与验证](docs/testing.md) | Windows／Linux 检查、离线测试、显式 OMP／SSH 测试及界面验证隔离 |
| [历史验证与性能记录](docs/validation-history.md) | 各开发阶段的实测环境、旧版计数、模型／SSH 联调及性能基准 |
| [变更与历史记录](docs/changelog.md) | 本版改动和早期版本记录 |
| [发布与安装包](docs/releasing.md) | 版本、发布流程、安装与卸载和 SHA-256 校验 |

## 数据与兼容

GUI 数据默认位于 `%LOCALAPPDATA%\OMP GUI`，可用 `--data-dir` 或 `OMP_GUI_DATA_DIR` 指定；一个数据目录同时只有一个可写窗口。OMP 配置目录由所选环境的 `omp config path` 决定，和 GUI 数据目录分别管理。工作区升级为版本 2，首次保存旧版本时保留原始 `workspace.v1.backup.json`。

原生会话共用 OMP 的项目存储，GUI 删除也会删除终端可恢复的对应记录。评论、草稿和归档状态保存在 GUI 数据目录。`--data-dir` 仅隔离 GUI 数据；测试时如需隔离原生会话，另设 `PI_CODING_AGENT_DIR`，见[界面验证隔离](docs/testing.md#界面验证隔离)。

普通文件支持编辑与保存；变更、历史引用及演示快照保持只读。打开文件支持 UTF-8、单文件不超过 2 MB、最多枚举 5000 项。公式支持常见 LaTeX 数学语法，无法排版时回退为文本。高级服务配置、认证和未覆盖的功能仍可通过 OMP 原生入口维护，详见[使用范围](docs/usage.md#数据目录与限制)。

已验证本机 OMP 18.11.3／18.14.0 和远端 Linux x86_64 的 OMP 18.14.2；附带公开目录来源标注为 18.14.0，这些不是最低版本声明。协议与配置参考 [OMP 官方文档](https://github.com/can1357/oh-my-pi/tree/main/docs)，原生组件参考 [gpui-kit](https://github.com/longbridge/gpui-kit)。
