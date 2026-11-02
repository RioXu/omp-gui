# 构建、测试与验证记录

[返回 README](../README.md) · [使用说明](usage.md) · [组件架构](architecture.md)

## 当前验证基线

功能基线包含原生会话同步、组件式目录选择器、可移植共享核心和 SSH Linux 远程工作环境，Windows 代码验证更新于 2026-11-02；本轮增加性能与流式解析回归，Linux 实机结果仍为先前记录。0.8.2 另验证了显式空闲释放、快速删除及占用保护；以下记录明确区分当前 Windows 验证与历史 Linux 实测；便携包的自动构建及发布流程见[发布说明](releasing.md)。

| 项目 | 已有验证结果与范围 |
| --- | --- |
| 离线测试 | Windows 共 130 项通过（共享核心 84、桌面 46）；新增弹窗短距离过渡、中途关闭连续性、减少动态效果和目录加载／错误布局稳定性验证，以及原生动作意图投影、跨会话摘要隔离、两侧各 10% 留白的响应式布局和连续层级竖线绘制验证，保留共享快照、增量更新、协议、保存和原生同步等覆盖 |
| Linux 共享核心与服务 | 隔离 SSH 测试目录内核心 71 项通过，服务构建及 core／remote Clippy 通过，不构建 GPUI；实际目标 Linux x86_64，OMP 18.14.2。ARM64 仅配置 CI，未实机验收 |
| 公式与面板动效 | 真实回复隔离副本验证行内／块级公式、深色皮肤下的 SVG 字形和思考正文颜色；回归覆盖常见数学语法、缓存复用、单行公式规范化和原文范围映射。原生鼠标点击测试验证切页立即生效、不触发扫描，首个 16ms 动画帧已有明显位移、约 128ms 内到位以及快速反向切换 |
| 动画性能 | 首次未优化调试版的 render 时间戳采样约 17–21 次更新／秒；随后用原生 Draw／Present 定位为 CPU 绘制瓶颈，并验证优化开发构建满足 60FPS 帧预算。详细数据、P95 和验证边界见下方原生动画基准；16ms 模拟时钟测试不作为实际帧率证据 |
| 原文定位 | 隔离副本复现真实回复定位导致的重复无障碍节点崩溃；修复后两种跳转入口正常，跨段落引用准确高亮。回归验证长原文重复往返定位且引用位于可见区域 |
| 会话选区与皮肤 | 真实 Git 项目验证跨行拖选松开后持续高亮、Ctrl+C 复制；隔离副本验证相同正文和两种皮肤。回归覆盖 Git 元数据不重解析 Markdown、只读 status 不修改索引、皮肤切回后恢复配色与字体 |
| 组件式项目选择器 | 0.8.0 隔离窗口验证浏览、中文/空格路径及 Enter；0.8.1 原生绘制测试验证图标／名称同行、32px 行高及最近路径前缀。新窗口激活失败，未宣称本轮人工视觉验收完成 |
| 本机 OMP 集成测试 | 0.8.0 的 8 项全部通过；0.8.2 重跑原生互操作场景，验证显式释放后的锁／删除及标题互通，不调用模型。其他 7 项未重跑，均默认 `ignored` |
| SSH 主流程 | 实际远端文件读写、Git、配置及子智能体表单、冲突保护、保存确认跨连接查询、迟到重复请求不覆盖新版本；模型读取远端项目和 Windows 中文附件；两个远端模型会话并行通过 |
| SSH 中断与重开 | 工具运行中实际中断 SSH，任务继续并恢复结果；重新创建 GUI 连接读取原生历史，确认提示词只执行一次；另一 GUI 连接同一会话被拒绝 |
| SSH GUI 删除竞态 | 0.8.2 显式 GPUI 测试执行真实桌面连接、删除和周期同步：未打开的新会话约 0.52 秒，刚切走／后台释放中的会话约 1.18 秒，刚归档约 0.63 秒，当前已连接会话约 0.58 秒，均无需等待 10 秒。只创建及删除专用 QA 记录，不发送模型请求 |
| 显式释放保护 | 释放所属空闲 OMP 并检查原生锁约 0.38 秒；竞争 GUI 无法释放实际所有者。另启动监督服务之外的真实 OMP，恢复专用日志并改名取得原生锁，确认 GUI 释放／删除均拒绝，终端仍能响应；运行工具时请求释放被拒绝，重新连接后模型继续完成 |
| OMP／监督服务异常退出 | 终止仅绑定本次新建日志和专用 cwd 的 OMP，明确报告退出；另强制终止独立数据范围、仅拥有本次会话的监督服务，验证进程代次变化、恢复原生历史和不重复提示词；0.8.2 共 8 项显式 SSH 集成测试全部通过，另有上述桌面 GPUI 删除场景 |
| 大项目与启动恢复 | 对实际项目 22 个原生日志、395208994 字节只读验证，索引仅 8843 字节；最大会话 2977 条消息／25498026 字节投影成功分片加载。加载期间后台新建约 0.71–0.77 秒，未变化历史不再传输。独立 GUI 数据目录恢复全部索引、模型与草稿；读取可访问性状态确认，未执行日常项目模型任务 |
| 远端路径与 profile | 实机验证中文／空格目录、目录符号链接、含反斜线的 Linux 文件名、只读保护、错误 OMP 覆盖、应用错误后通道存活，以及独立 profile 的原生记录不混入默认 profile |
| 自动部署 | 0.8.2／管理协议 3 从内置源码上传、校验、远端 release 构建及握手通过；使用测试主机已有 Cargo 缓存，不代表无依赖网络／无 Rust 的源码回退可用 |
| 远端界面 | 隔离 GUI 验证环境菜单、远端项目／会话、Python 文件高亮及编辑、Ctrl+S 回读、外部修改通知自动刷新、保存后关闭及远端 `/model` 设置目标；最终版本复查未保存切换环境时确认／取消、dirty 清除和设置页 Host／profile 标识 |
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
| 响应式正文与项目层级 | 原生绘制测试验证 850／1100／1500／1900 px 窗口、深浅主题中正文／通知／输入框同步伸缩，并断言两侧各占会话区域的 10%；项目到首个会话间距不超过 2 px，连续竖线实际绘制，2000 会话仍保持虚拟化与折叠 |
| 原生动作摘要 | OMP 18.14.0 默认模型实测读取隔离 note.txt，收到“读取测试文件”并返回 323；独立 GUI 用 deepseek-flash 实际读取文件及执行只读等待命令，确认“等待只读校验完成”显示在输入框上方、完成后消失。原生绘制测试另验证长摘要单行、上下位置和跨会话生命周期；隔离窗口确认最大化后的正文与输入区伸缩 |
| 文件语法高亮 | 启用 gpui-kit 的 Syntect 后端，独立窗口验证 Rust、Python、JSON 的关键词／字符串／注释着色，以及 Python 文件自动刷新后的高亮 |

本次核对的本机 OMP 为 **18.14.0**，较早的功能验证使用 18.11.3。随仓库附带的公开目录 `resources/omp-services.json` 标注来源版本 **18.14.0**，包含 89 个提供商标识、5725 条聊天／搜索模型元数据、22 个搜索条目。安装版本与目录来源不是同一概念，附带目录版本不构成最低兼容版本声明；运行时以本机 OMP 返回的可用模型及配置为准。

较早的真实 DeepSeek 运行、窗口验证与性能测量见[历史记录](changelog.md)。历史计数和耗时仅描述当时环境，不作为当前测试数量或性能承诺。

## 性能回归（2026-11-02）

默认测试覆盖共享快照的编辑隔离、慢写盘时保存请求合并及关闭落盘、sonic-rs 与既有 JSON 的中文／转义／整数及浮点兼容性、流式消息的历史实体复用和滚动锚点、空闲缓存回收与草稿恢复、侧栏缓存失效和 2000 个会话的可见区域绘制。

手动合成基准（不读取真实历史，不调用模型）：

```powershell
cargo test --locked -p omp-core --release workspace_performance_baseline -- --ignored --nocapture
```

本机 Windows x64 release、200 个会话各 50 条消息、约 12.6 MB JSON 的一次测量：共享快照平均 4.53 µs，serde_json 整库编码平均 31.26 ms，sonic-rs 平均 19.89 ms，首次修改共享会话复制约 0.15 ms。两种编码均保持通用编译目标，未启用 target-cpu=native；耗时仅供同一合成负载比较，不代表真实工作区的启动、帧耗时或总内存承诺。

### 流式 JSON 帧解析比较

测试实现位于 `crates/omp-core/src/infrastructure/omp/stream_json_bench.rs`，simd-json 已接入生产接收路径。复现桌面程序实际启用的 serde_json 特性：

```powershell
cargo test --locked -p omp-core --release --features serde_json/preserve_order,serde_json/raw_value stream_json_performance -- --ignored --nocapture
```

2026-11-02，Windows x64、i7-11800H、Rust 1.97.1；serde_json 1.0.154、sonic-rs 0.5.10、simd-json 0.19.0，默认分配器和通用编译目标。构建完成后单独运行，不与编译并行；每种负载各测 9 轮，每轮各实现累计至少 80 ms，轮换测试次序，表中为每帧平均耗时的中位数（µs，越低越好）。

| 合成 OMP 帧 | 平均字节数 | serde_json | sonic-rs | simd-json 复制输入 | simd-json 可变输入 | simd-json 可变输入＋复用 Buffers |
|---|---:|---:|---:|---:|---:|---:|
| 短文本增量 | 234 | 4.126 | 4.124 | 4.373 | 4.277 | 3.525 |
| 累积增长的消息快照 | 38,340 | 84.130 | 61.904 | 67.276 | 63.999 | 59.211 |
| 含嵌套任务的工具事件 | 6,031 | 78.925 | 76.178 | 72.366 | 72.259 | 70.323 |
| 512 KiB base64 传输分片 | 524,382 | 220.683 | 190.895 | 1004.143 | 685.489 | 292.736 |

所有实现均生成当前应用使用的 `serde_json::Value`，包含解析、输出分配和释放，不包含网络、文件读取、消息合并和界面渲染。模拟接收所需的初始输入缓冲分配对所有实现统一排除；“复制输入”额外计入适配原先 `Decoder::decode(&[u8])` 的复制成本。“可变输入”直接消费接收线程已有的 Vec；复用 Buffers 另保留解析器内部暂存区。正确性测试逐帧比较三库的输出。上表为接入前的对比记录，默认 simd-json 特性；最新生产复测额外启用 big-int-as-float 兼容旧解析行为。

普通流式事件中，复用 Buffers 的 simd-json 相比原先 serde_json 解析耗时降低约 11%–30%，相比 sonic-rs 降低约 4%–15%；大分片仍是 sonic-rs 更快。生产路径据此采用小于 256 KiB 的帧走 simd-json 并复用 Buffers，较大帧及重组正文走 sonic-rs，接收 Vec 也跨帧复用。基准新增 Production 实现，直接调用生产 JSON 分流函数，包含大帧嵌套深度预检查；仍不包含完整分片组装或界面处理。保存路径继续使用 sonic-rs。

接入后的独立复测（同一机器、命令和 9 轮中位数，simd-json 已启用 big-int-as-float，单位 µs／帧）：

| 合成 OMP 帧 | 同轮 serde_json 基线 | 生产分流函数 | 解析耗时降低 |
|---|---:|---:|---:|
| 短文本增量 | 4.117 | 3.655 | 11% |
| 累积增长的消息快照 | 86.879 | 58.617 | 33% |
| 含嵌套任务的工具事件 | 81.117 | 71.282 | 12% |
| 512 KiB base64 传输分片 | 218.407 | 207.242 | 5% |

大帧的生产结果包含嵌套预检查，因而略慢于直接调用 sonic-rs，仍优于原解析路径。流式解析阶段当时通过格式检查、全 workspace Clippy、117 项默认测试及本地构建，另有 20 项显式测试保持忽略；这是历史阶段计数，当前默认测试为上表的 130 项。此处单独运行了上述发布模式基准。

协议回归测试覆盖两条路径在 256 KiB 阈值附近切换、复用输入后旧事件仍然有效、中文与代理对及转义、64 位边界和超大整数浮点退化、非法 UTF-8／JSON／尾随数据、与旧解析器一致的容器嵌套上限、字符串中括号不计入嵌套，以及分片乱序、重组正文校验和大小限制。

## 标准构建与检查

需要 Windows、Rust MSVC 工具链、Visual Studio C++ Build Tools 及 Windows SDK。运行和文件状态测试还需要 Git；本机集成测试另需安装并配置 OMP。

```powershell
cargo fmt --all --check
cargo clippy --workspace --locked --all-targets --all-features -- -D warnings
cargo test --workspace --locked
cargo build --locked
```

发布优化构建使用 `cargo build --locked --release`，产物在 `target/release/omp-gui.exe`；普通构建在 `target/debug/omp-gui.exe`。

[CI](../.github/workflows/check.yml) 在 Windows 检查 workspace，并在 Linux x86_64／ARM64 检查共享核心和服务、构建远端 release 产物。检查工作流保存独立的 Linux artifact；另有 [Release 工作流](../.github/workflows/release.yml) 为版本标签构建 Windows 优化程序与两种 Linux 服务，全部检查通过后组装便携 ZIP、生成 SHA-256 并发布，见[发布说明](releasing.md)。修改文档时检查引用、链接及命令示例，不必重复执行真实模型任务。

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

## 原生动画帧率基准

2026-11-02，相同公式长回复隔离副本、前台窗口、每组 48 次动作，原生 Draw／Present 实测如下。全部组的 `active_switches` 均为 48；高刷组屏幕刷新率为 165Hz。表中 FPS 指连续动画帧的提交频率，60Hz 显示器的物理显示仍最多为 60 FPS。

| 场景 | 平均绘制 | 平均动画提交频率 | P95 提交间隔 |
| --- | --- | --- | --- |
| 未优化开发构建，165Hz 标签 | 42.30ms | 22.6 FPS | 48.45ms |
| 优化开发构建，165Hz 标签 | 7.50ms | 91.3 FPS | 19.18ms |
| 优化开发构建，165Hz 弹窗 | 6.99ms | 73.8 FPS | 25.30ms |
| 优化开发构建，60Hz 标签 | 6.11ms | 113.6 次／秒 | 15.06ms |
| 优化开发构建，60Hz 弹窗 | 6.48ms | 113.7 次／秒 | 14.63ms |

60Hz 两组分别包含 658／1000 个连续动画提交间隔；P95 均低于 16.67ms。仍存在少量约 19ms 的长帧，不承诺系统任意负载下零掉帧。原始本地报告为 `artifacts/native-frame-baseline.json`、`native-frame-optimized-{tabs,dialogs}.json`、`native-frame-60-{tabs,dialogs}.json`（均不提交）。

使用含长回复与公式的隔离演示数据，保持窗口处于前台、关闭其他测试负载。以下命令只在 `--demo` 和显式 `--data-dir` 同时存在时启用自动切页，不发送模型请求；普通构建不包含分析器。

```powershell
cargo build --locked --features frame-profile
target/debug/omp-gui.exe --demo --data-dir artifacts/math-qa/data --frame-profile-output artifacts/native-frame-tabs.json
```

预热 3 秒后，每 220ms 切换一次原生页签，共 48 次；约 14 秒生成 JSON。关闭该隔离窗口后，可在同一命令追加 `--frame-profile-dialogs`，另指定输出文件，验证 24 次弹窗打开／关闭。原始样本包含 GPUI 的布局、预绘制、绘制和平台提交时间，以及连续动画帧的实际提交间隔。`active_switches` 必须为 48，否则前台采样无效；空闲不绘制的间隔不计入 FPS。`summary` 汇总平均 FPS 与 P95 帧间隔。提交间隔是原生运行时指标，不等同于显示器物理扫描输出。

多显示器验证可追加 `--frame-profile-hz 60`，由 GPUI 选择刷新率匹配的显示器；必须再检查报告内的 `refresh_ms`，匹配失败会使用默认屏幕。该参数仅在启用分析功能的演示构建中生效，不修改系统显示设置。
