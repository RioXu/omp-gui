# OMP GUI

基于 Rust / gpui-kit 0.8.1 的 Windows 原生 OMP 客户端。支持会话、代码选区评论、文件与变更标签、当前标签查找，以及 OMP 模型、服务和子智能体配置。界面不使用 WebView。

本文对应当前源码，最后核对于 2026-10-31。Cargo 包版本仍为 0.6.1；尚未发布的源码改动与历史版本记录见[变更记录](docs/changelog.md)。

## 快速开始

运行需要 Windows、本机 OMP 和 Git。源码构建另需 Rust MSVC 工具链、Visual Studio C++ Build Tools 和 Windows SDK。

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

## 主要操作

- `Ctrl+Enter` 发送；运行中追加输入默认排队，“指示”将等待项提升为 steering。
- `@` 引用文件，`/` 补全命令；`↑↓` 选择，`Tab` / `Enter` 确认，`Esc` 关闭候选。
- `Ctrl+F` 查找激活标签；查找框内 `Enter` / `Shift+Enter` 跳到下一项／上一项。
- 会话标签固定在最左且不可关闭；文件和变更标签可分别关闭。工作面板的变更平铺、文件按层级展示，并自动刷新。
- 选中回复文字或代码可添加评论；历史评论绑定不可变快照，跟进以追加方式保存。
- `/model`、`/models`、`/switch` 打开模型服务与注册；`/fast`、`/slow` 打开速度与服务等级。带参数的命令继续由 OMP 执行。
- 子智能体和角色模型关联使用原生表单，支持模型选择、对应推理档位及继承默认；保存范围可选全局或当前项目。
- 项目会话与同一 OMP 配置目录下的终端会话双向同步：新建、历史、标题和删除共享原生记录；归档仍由 GUI 本地管理。

## 文档导航

| 文档 | 内容 |
| --- | --- |
| [使用与配置](docs/usage.md) | 快捷键、队列、评论、文件标签、命令目标、模型与子智能体表单、数据范围及限制 |
| [组件职责与维护约定](docs/architecture.md) | 模块边界、依赖方向、状态归属和行为约定 |
| [构建、测试与验证](docs/testing.md) | Windows 构建、56 项离线测试、8 项显式本机测试、界面验证隔离与结果 |
| [变更与历史记录](docs/changelog.md) | 当前未发布改动和早期版本记录 |

## 数据与兼容

GUI 数据默认位于 `%LOCALAPPDATA%\OMP GUI`，可用 `--data-dir` 或 `OMP_GUI_DATA_DIR` 指定；一个数据目录同时只有一个可写窗口。OMP 配置目录由本机 `omp config path` 决定，和 GUI 数据目录分别管理。

原生会话共用 OMP 的项目存储，GUI 删除也会删除终端可恢复的对应记录。评论、草稿和归档状态保存在 GUI 数据目录。`--data-dir` 仅隔离 GUI 数据；测试时如需隔离原生会话，另设 `PI_CODING_AGENT_DIR`，见[界面验证隔离](docs/testing.md#界面验证隔离)。

文件预览只读，支持 UTF-8、单文件不超过 2 MB、最多枚举 5000 项。LaTeX 公式暂显示原文。高级服务配置、认证和未覆盖的功能仍可通过 OMP 原生入口维护，详见[使用范围](docs/usage.md#数据目录与限制)。

当前已验证本机 OMP 18.11.3；附带公开目录来源标注为 18.14.0，二者并非同一兼容性声明。协议与配置参考 [OMP 官方文档](https://github.com/can1357/oh-my-pi/tree/main/docs)，原生组件参考 [gpui-kit](https://github.com/longbridge/gpui-kit)。
