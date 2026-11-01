# 构建、测试与验证记录

[返回 README](../README.md) · [使用说明](usage.md) · [组件架构](architecture.md)

## 当前验证基线

功能基线包含原生会话同步、文件链接、协作消息展示、组件式目录选择器及可移植共享核心，最后完成代码验证于 2026-11-01。本轮未发送真实模型提示词，也不代表发布了新安装包。

| 项目 | 已有验证结果与范围 |
| --- | --- |
| 离线测试 | Windows 共 86 项通过（共享核心 63、桌面 23）；增加环境兼容迁移、POSIX 路径隔离与 PATH 发现顺序，保留 Markdown、附件、并发、编辑与原生同步等覆盖 |
| Linux 共享核心 | 隔离 SSH 测试目录内 62 项通过，不构建 GPUI；测试主机安装 OMP 18.14.2。完整远程 GUI 尚未接入 |
| 组件式项目选择器 | 隔离演示窗口验证目录浏览、中文/空格路径输入、Enter 进入和加入项目列表；不调用 Windows 文件浏览器 |
| 本机 OMP 集成测试 | 8 项，默认 `ignored`；最近一次通过会话同步场景，其余 7 项未重跑 |
| 格式、Clippy、Windows 构建 | 功能基线已通过 |
| 原生会话互操作 | 本机 OMP 按 ID 前缀恢复 GUI 创建的会话、原生新建与改名、共享占用锁及删除均通过；不调用模型 |
| 会话同步界面 | 隔离窗口验证 GUI 新建落盘、终端新建自动出现、中文原生历史展示、外部删除自动移除及本地归档保留原生文件 |
| 文件链接 | 隔离窗口验证中文／空格相对路径、绝对路径复用标签、项目外文件预览、新文件创建后的链接重解析，以及链接内中文查找、高亮和当前项切换 |
| 协作消息与任务 | 使用按 OMP 18.11.3 格式构造的原生日志，验证 Advisor 原生封装的正文／名称／严重性、智能体来信、任务完成数／受阻原因／详情／备注、已完成子会话、发送与等待工具参数和输出；不是一次真实多模型运行 |
| 模型服务跳转 | 独立窗口验证 `/models` 的 Tab 确认；首次配置加载完成后自动进入服务与注册页。单元测试覆盖 `/model`、`/models` 及其他配置路由 |
| 速度设置 | 独立窗口验证 `/fast` 的 Enter 确认；单元测试覆盖 `/fast`、`/slow` 和对应设置过滤 |
| 子智能体表单 | 独立配置目录验证空名称时打开表单、必填提示、创建、保存及重新打开 |
| 角色模型表单 | 独立配置目录验证模型、DeepSeek 原生推理档位及 `max` 保存；OMP CLI 回读确认 `modelRoles` 值 |
| 文档行显示 | 原生绘制测试验证 400 px 列表宽度、整行选中及右边缘点击；验证深浅主题的正文与行号背景一致 |
| 项目与空会话 | 隔离窗口验证点击项目收起、折叠项目中新建会话自动展开，以及切换后保留未发送草稿和原生记录。单元测试验证原生空会话删除后不再导入，保护终端未命名会话、占用锁、其他分支消息、草稿、附件、备份及损坏记录 |
| Marker 任务组 | 隔离窗口验证完成／进行中／待处理／受阻／失败／停止／未知状态的图标与颜色、长任务换行、阶段完成数、受阻原因／详情／备注对齐、状态读屏名称及深浅主题；使用静态原生格式任务数据，不发送模型提示词 |
| 文件编辑与未保存保护 | 隔离窗口验证输入后不改磁盘、Ctrl+S 保存及圆点清除、BOM／CRLF 保留、保存后的撤销／重做、单文件保存／不保存／取消、保存失败保留提示框、多个未保存文件的切换确认／取消、关闭全部文件标签确认、窗口关闭及 Alt+F4 的确认／取消、外部修改不覆盖缓冲区；不发送模型提示词 |
| 附件与并发 | 本机 OMP 18.11.3 配合仅监听本机的模拟 OpenAI 服务，验证实际 Ctrl+V 图片卡片及原生图片请求、中文文件附件读取、两个会话同时输出且独立结束、侧栏靠右 Spinner 和上下文圆环；未使用真实视觉模型验证识图能力。原生文件拖放事件由绘制测试覆盖；桌面自动化工具不支持跨窗口拖放 |
| Markdown 公式边界 | 用含紧邻公式正文的 `$$` 多行公式复现后续章节被吞入代码块；回归测试验证公式首行、标题／粗体／列表、源范围映射和原生 TextView 绘制，实际回复使用隔离副本验证，不重发模型请求 |
| 工作区布局 | 隔离窗口验证 Sidebar 多项目滚动及长标题截断、底部设置入口、右侧四类 underline tabs、文件标签打开、约 280px 窄面板拖动、深浅主题和三栏标题对齐；不发送模型提示词 |
| 文件语法高亮 | 启用 gpui-kit 的 Syntect 后端，独立窗口验证 Rust、Python、JSON 的关键词／字符串／注释着色，以及 Python 文件自动刷新后的高亮 |

本次核对的本机 OMP 为 **18.11.3**。随仓库附带的公开目录 `resources/omp-services.json` 标注来源版本 **18.14.0**，包含 89 个提供商标识、5725 条聊天／搜索模型元数据、22 个搜索条目。二者不是同一版本概念，附带目录版本不构成最低兼容版本声明；运行时以本机 OMP 返回的可用模型及配置为准。

较早的真实 DeepSeek 运行、窗口验证与性能测量见[历史记录](changelog.md)。历史计数和耗时仅描述当时环境，不作为当前测试数量或性能承诺。

## 标准构建与检查

需要 Windows、Rust MSVC 工具链、Visual Studio C++ Build Tools 及 Windows SDK。运行和文件状态测试还需要 Git；本机集成测试另需安装并配置 OMP。

```powershell
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --locked
```

发布优化构建使用 `cargo build --locked --release`，产物在 `target/release/omp-gui.exe`；普通构建在 `target/debug/omp-gui.exe`。

[Windows CI](../.github/workflows/check.yml) 执行格式检查、所有目标的 Clippy 和默认测试；构建产物及发布包不是该工作流的交付步骤。修改文档时检查引用、链接及命令示例，不必重复执行真实模型任务。

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

尚未逐一连接真实 LSP/MCP 服务，也未在本轮安装或卸载用户插件。当前没有宣称完整屏幕阅读器认证。LaTeX 公式没有排版控件，因此没有公式视觉渲染的通过记录。
