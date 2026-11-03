# 构建、测试与验证

[返回 README](../README.md) · [使用说明](usage.md) · [组件架构](architecture.md) · [历史验证与基准](validation-history.md)

## v0.9.1 验证范围（2026-11-03）

本版新增两项原生绘制回归，先在旧逻辑下复现跳转，再验证修复：无选区、停留在已显示正文底部时追加正文；主动上滚后接收 message_end、agent_end 和 session_settled；完成时不自动收起正在阅读的过程记录及思考预览。断言在 Markdown 解析和列表绘制后检查滚动像素位置，同时检查当前会话和键盘焦点。

Windows workspace 默认测试 157 项通过（core 100、桌面 57）；显式集成测试与手工基准保持忽略，桌面库另有 4 项显式测试忽略。既有回归继续覆盖鼠标拖选、复制与评论、首次流式搜索命中、后台会话隔离、五行思考预览、任务浮层、公式和文件标签。思考预览与全文采用小一级字号和边框，二者布局保持一致。

本次修复通过事件和绘制测试复现，不依赖模型自述；未新增真实模型或 SSH 联调结果。全 workspace、Fast 后端、Linux 服务及安装包检查由发布流程分别执行。各项旧版模型联调、SSH 故障验证和性能数据保存在[历史记录](validation-history.md)，不得当作本版本的新实测。

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
