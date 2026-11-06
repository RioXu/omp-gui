# 构建、测试与验证

[返回 README](../README.md) · [使用说明](usage.md) · [组件架构](architecture.md) · [历史验证与基准](validation-history.md)

## v0.9.5 验证范围（2026-11-06）

本版新增或更新的确定性回归覆盖：

- 主会话及子智能体在底部跟随输出，上滚暂停、回到底部恢复，键盘焦点不变；已选文字的阅读位置保持，完成和交互请求仍返回末尾。
- ask 的 Other 自由回复立即显示为用户消息；序列化重载、多次原生历史合并及同文普通输入不会丢失或重复。发送失败、取消和敏感输入不产生错误的可见回复。
- 未指示追加输入等待原生取消确认后恢复文字和附件，保留现有草稿；模型已接收时不误撤回，取消不停止正在执行的任务。
- 实际 ↑／↓ 按键恢复本会话用户历史，区分显示正文与 wire，保护已编辑的多行草稿并隔离会话。
- 1000 项过程的绘制仅创建可视区步骤，滚动后界面随可视范围切换；10000 行工具输出保持完整，只排版少量可视行，复用编辑器状态并拒绝编辑。
- 思考搜索和历史评论可定位内层虚拟列表中的步骤，既有消息历史每页 100 条的规则保持。

桌面库回归 75 项通过、4 项显式测试忽略。发布脚本执行格式检查、全功能 Clippy、完整 workspace 默认测试及 Windows 正式构建；GitHub Checks 另执行 Fast 后端和 Linux 服务检查。安装、覆盖升级、卸载、两端提交／版本／协议及上传摘要由发布流程验证，结果以对应提交的 Checks 与 Release 为准。

本版没有新增真实模型任务、SSH 故障联调或实机帧率基准。虚拟化回归验证界面创建与可视行范围，不把模拟时钟结果当作耗时承诺；完整历史读取、文本初始化和 Markdown 首次解析仍有成本。旧版结果见[历史记录](validation-history.md)。

## 标准构建与检查

需要 Windows、Rust MSVC 工具链、Visual Studio C++ Build Tools 及 Windows SDK。运行和文件状态测试还需要 Git；本机集成测试另需安装并配置 OMP。

```powershell
cargo fmt --all --check
cargo clippy --workspace --locked --all-targets --all-features -- -D warnings
cargo test --workspace --locked
cargo build --locked
```

发布优化构建使用 `cargo build --locked --release`，产物在 `target/release/omp-gui.exe`；普通构建在 `target/debug/omp-gui.exe`。

[CI](../.github/workflows/check.yml) 在 Windows 检查 workspace，在 Linux x86_64 检查共享核心、构建服务并上传含提交／版本元数据的 artifact。[本地发布脚本](../scripts/publish-release.ps1) 负责 Windows 正式构建、Linux artifact 校验、安装包验证和上传，见[发布说明](releasing.md)。修改文档时检查引用、链接及命令示例，不必重复执行真实模型任务。

在 Linux 单独验证远端组件：

```sh
cargo clippy --locked -p omp-core -p omp-gui-remote --all-targets -- -D warnings
cargo test --locked -p omp-core -p omp-gui-remote -- --test-threads=1
cargo build --locked --release -p omp-gui-remote
```

Linux 测试涉及原生进程级会话锁，采用串行执行以避免与并行子进程场景相互干扰；保留所有占用保护断言，不跳过锁测试。

### Windows 常见构建问题

- 找不到 MSVC 工具或 SDK 时，先确认已安装 C++ 桌面开发组件，可在 Visual Studio Developer PowerShell 中构建。
- 若 PATH 中的 coreutils `link.exe` 抢先于 MSVC 链接器，调整当前构建进程的 PATH 顺序，或在工具所属的 override 配置中关闭冲突命令。正常 MSVC 环境无需为项目另行硬编码 linker 路径。
- `omp-gui.exe` 正在运行时，Windows 可能拒绝替换同路径的构建产物；关闭占用该产物的窗口后再构建。

仓库忽略的 `target/` 里可能保留本机测试辅助脚本和临时工具链，它们不是项目依赖，也不应作为其他开发者必需的构建步骤。

## 测试归属

- 领域测试覆盖消息分组、排队与恢复、评论锚点、重复文本及快照。
- 基础设施测试覆盖协议分片、Windows 目录身份、Git 状态与路径、配置格式保留及外部修改冲突、提供商目录合并、子智能体文档和角色映射。
- 桌面测试覆盖命令路由、输入补全、模型分组及推理档位、目录树状态、Unicode 查找、高亮绘制、选区状态和文件行布局。
- 确定性测试靠近实现，使用 `#[cfg(test)]`；需要本机 OMP 的场景统一位于 [tests/omp](../tests/omp/mod.rs)，由 [omp_integration.rs](../tests/omp_integration.rs) 作为 Cargo 集成测试入口。

列出测试而不执行：

```powershell
cargo test --locked --lib -- --list
cargo test --locked --test omp_integration -- --list
```

## 显式启用的本机测试

| 测试名 | 行为与依赖 |
| --- | --- |
| `installed_registry_includes_configured_deepseek` | 读取本机模型目录，要求已配置 DeepSeek，不发送提示词 |
| `installed_catalog_exposes_native_enums_and_fast_cache` | 读取原生配置及缓存，不发送提示词 |
| `installed_omp_accepts_minimal_native_model_override` | 在临时配置目录验证模型参数覆盖，不发送提示词 |
| `live_process_stream_preserves_thinking_tools_and_final_answer` | 使用已配置模型验证思考、工具与最终回复流 |
| `live_tool_intents_reach_the_gui_projection` | 使用默认模型只读隔离文件，验证原生工具意图能投影为 GUI 动作摘要及最终结果 |
| `live_slash_commands_and_unicode_file_mentions` | 验证原生命令及中文、空格路径引用，发送模型任务 |
| `live_follow_up_waits_until_promoted` | 验证 follow-up 排队及提升为 steering，发送模型任务 |
| `live_steering_and_stop_are_acknowledged_and_settle` | 验证 steering、停止确认及会话结束，发送模型任务 |
| `installed_omp_discovers_gui_session_by_id_and_native_creation_is_imported` | 隔离原生会话互操作，要求显式设置测试配置目录；使用占位提供商凭据，不发送提示词 |

这些测试依赖当前安装和账户环境；真实模型任务消耗额度，应在已有相应授权时运行。只选择需要验证的场景，例如：

```powershell
cargo test --locked --test omp_integration installed_catalog -- --ignored --nocapture
cargo test --locked --test omp_integration installed_omp_accepts_minimal_native_model_override -- --ignored --nocapture
cargo test --locked --test omp_integration live_slash_commands_and_unicode_file_mentions -- --ignored --nocapture
```

不要把所有 `--ignored` 场景自动并入默认 CI。注册表测试还包含对本机特定提供商的断言，不能据其失败直接推断所有账户环境都不兼容。

运行原生会话互操作场景（测试会在此专用目录写入占位模型配置）：

```powershell
$syncRoot = Join-Path (Get-Location) 'target/session-sync-test'
$env:PI_CODING_AGENT_DIR = "$syncRoot/omp"
$env:PI_TEST_SESSION_OWNERS_DIR = "$syncRoot/owners"
cargo test --locked --test omp_integration installed_omp_discovers_gui_session -- --ignored --nocapture
```

测试会拒绝专用目录之外的配置路径。仅在当前 PowerShell 会话设置环境变量，完成后移除变量或关闭该会话。

## 显式 SSH 测试

[remote_integration.rs](../tests/remote_integration.rs) 默认忽略，必须提供 SSH Host、当前版本服务和独立测试项目。文件测试会修改专用 `remote-check.txt` 并恢复；模型测试创建并清理自己的原生日志。断线场景会终止自己的 SSH 桥；异常退出场景只终止绑定新建测试日志且 cwd 匹配的 OMP，不操作其他会话。

```powershell
$env:OMP_GUI_TEST_SSH_HOST = 'your-test-host'
$env:OMP_GUI_TEST_HELPER = '/absolute/path/to/omp-gui-remote'
$env:OMP_GUI_TEST_PROJECT = '/home/test/.cache/omp-gui-tests/run-unique/project'
cargo test --locked --test remote_integration -- --ignored --nocapture --test-threads=2 --skip large_project_index_and_history_do_not_block_session_creation
```

测试会拒绝约定范围之外的项目。需要先建立专用 Git 项目和含 `remote-code-1729` 的 `remote-check.txt`；禁止指定日常项目。监督服务崩溃测试使用专用状态目录和启动包装，并在终止前核对服务的全部直接子进程；不影响其他服务范围。`OMP_GUI_TEST_TRACE=1` 只打印事件类型和请求身份，帮助诊断阶段，不打印模型正文。

大历史回归 `large_project_index_and_history_do_not_block_session_creation` 另需显式设置 `OMP_GUI_TEST_READONLY_PROJECT`，此路径仅用于读取已有索引和历史；创建／删除仍只发生在上述专用测试项目。设置后移除命令中的 `--skip` 参数即可运行全部 8 项；单独运行该场景而未提供此变量会报错。耗时记录是测试主机上的后台操作结果，不等于完整界面进入可发送状态的耗时承诺。

复验桌面删除与后台同步的竞态，使用相同的专用 SSH Host、服务和 QA 项目变量：

```powershell
cargo test --locked -p omp-gui --lib remote_delete_active_session_survives_periodic_sync -- --ignored --nocapture
```

该场景运行 GPUI 的会话事件和同步处理，不依赖前台窗口激活；创建并清理自己的四个原生日志，包含切走后立即删除、归档后立即删除和当前连接删除。桌面数据使用无磁盘写入的测试替身，不操作日常会话。

自动部署另有显式检查：

```powershell
$env:OMP_GUI_TEST_INSTALL_SSH_HOST = 'your-test-host'
cargo test --locked --lib remote_install::tests::installs_embedded_remote_service -- --ignored --nocapture
```

它会在该用户的标准服务目录安装当前版本。实际测试未重启 Linux、改写 SSH／防火墙配置或模拟整机断电；服务换代与宿主重启不承诺工具持续执行。先前通过 UIA 获取状态后立即关闭测试窗口，曾触发外部 `accesskit_windows` 对失效窗口句柄的退出竞态；未将其计作远端功能验证通过，也未修改第三方源码。

## 界面验证隔离

GUI 会话数据和 OMP 配置是两个独立范围。仅指定 `--data-dir` 不会隔离 OMP 全局设置。

```powershell
$testRoot = Join-Path (Get-Location) 'target/ui-check'
New-Item -ItemType Directory -Path "$testRoot/data", "$testRoot/omp", "$testRoot/project" -Force | Out-Null
$env:PI_CODING_AGENT_DIR = "$testRoot/omp"
omp config path
cargo run -- --data-dir "$testRoot/data" --project "$testRoot/project"
```

保存设置之前确认 `omp config path` 和界面显示的保存位置都指向测试目录。具名 profile 或环境覆盖可能影响目录选择，应以本机实际返回路径为准。测试完成后关闭该 PowerShell 会话，或移除该会话中的临时环境变量；不要把临时配置设置成系统环境变量。

手工回归优先覆盖：项目与会话切换、文件／变更标签、外部文件刷新、当前标签查找、补全的鼠标与键盘确认、模型与推理切换，以及全局／项目范围下表单的保存、取消、冲突与重新载入。

## 已知验证边界

尚未逐一连接真实 LSP/MCP 服务，也未在本轮安装或卸载用户插件。当前没有宣称完整屏幕阅读器认证。
