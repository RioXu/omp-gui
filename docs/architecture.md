# 组件职责与维护约定

[返回 README](../README.md) · [使用说明](usage.md) · [构建与验证](testing.md)

项目使用 Cargo workspace。根包负责 Windows 桌面界面；`crates/omp-core` 提供可在 Windows 和 Linux 独立构建的领域规则及基础设施，不依赖 GPUI；`crates/omp-remote` 是 Linux 服务。根库重导出共享组件，保持已有集成测试入口兼容。

默认在 `main` 分支直接工作，只有用户明确要求时才创建或切换其他分支；仓库约定见 [AGENTS.md](../AGENTS.md)。

## 目录与依赖方向

```text
src/
  main.rs                         二进制入口
  lib.rs                          组件边界
crates/omp-core/src/
  domain/                         会话、消息、评论、快照和历史规则
  environment/                    环境身份及本地/POSIX 路径类型
  host/                           目录／文件／监听接口、ExecutionHost、NativeStore、Configuration
  remote/                         版本化管理协议、SSH、重连与操作确认记录
  infrastructure/
    omp/                          OMP 子进程、JSONL 协议与安装检测
      sessions/                   原生会话发现、历史读取、迁移、创建删除和占用锁
    filesystem/                   项目文件、Git 状态与差异、目录身份归一化
    persistence/                  数据目录锁、原子保存与恢复
    configuration/
      settings.rs                 OMP 配置目录、范围、缓存和读写
      authoring/                  子智能体文档、角色映射与原生模型能力
      providers/                  服务目录、模型覆盖、连接验证与凭据导入
    process.rs                    Windows 隐藏子进程的通用启动方式
crates/omp-remote/src/
  main.rs                         stdio 管理请求、监听模式与用户环境启动
  daemon/                         私有 Unix socket、会话监督与有界事件恢复
src/desktop/
    project_picker.rs             本地／远端共用的组件式目录选择器
    remote_install.rs             校验并上传服务产物或内置源码
    bootstrap.rs                  参数、主题、快捷键和原生窗口创建
    theme.rs                      gpui-kit 语义主题
    workspace/                    主工作区协调与交互功能
      composer/                   输入、文件引用、命令补全和设置命令路由
      conversation/               会话生命周期、提交队列、评论和消息列表
      documents/                  文档标签、异步读取、文件监听和目录树
      search/                     当前标签内的匹配、高亮和定位
      runtime/                    OMP 连接、事件分发与模型/推理控件状态
      view/                       原生界面的布局与组件渲染
      initialization.rs           工作区装配和 GPUI 订阅
      preferences.rs              设置页入口、主题和面板偏好
    settings/                     配置页导航、类型控件和编辑交互
      navigation.rs               命令对应的具体页面或功能目标
      editors/                    子智能体与角色模型表单、共享模型选择器
    services/                     模型提供商及搜索服务的图形化管理
tests/
  omp_integration.rs              本机 OMP 集成测试入口
  omp/                            配置、输入引用、流式回复和队列场景
resources/                        随版本附带的服务目录等静态数据
scripts/                          开发与数据更新工具
```

桌面层通过 `host` 能力接口使用本地／远端适配，也可直接使用领域类型；适配器调用基础设施。`domain` 和纯环境／路径类型不引用 GPUI、基础设施或桌面层；`infrastructure` 与 `remote` 不引用桌面层或 GPUI。领域层负责数据和规则，文件系统、环境配置及 OMP 安装发现等效果放在适配器及基础设施中，Linux 服务只依赖共享核心。

`desktop` 是 Windows 原生客户端，明确替代含义宽泛的 `app` 命名。`workspace` 是主窗口的工作区，而不是整个程序。OMP 通信明确称为 `omp::Client`，不再用 `engine` 混合指代协议、业务和测试。

`DirectoryBrowser` 隔离目录选择与本机文件访问；`ProjectFiles` 隔离项目列表、文档读取和带版本校验的保存；`FileWatch` 隔离文件通知；`SessionConnection` 隔离会话 RPC 传输。`ExecutionHost` 组合本地与 SSH 适配，`NativeStore` 负责对应原生存储，`Configuration` 将配置效果留在所属环境。`environment::HostPath` 区分本地路径与 POSIX 路径，远端路径不能交给本地解析器。目录选择由 `desktop/project_picker.rs` 统一使用 Input、Button、虚拟列表及 Dialog 构成，不调用系统文件选择窗口。浏览异步执行，按请求代次丢弃迟到结果。

新工作区的 OMP 使用 PATH 中的 `omp`，设置路径留空恢复自动查找，已有人工路径覆盖保留。共享核心同时兼容 Windows 互斥锁和 Linux 抽象 Unix socket 原生会话占用规则；Linux 路径身份区分大小写。

`workspace/environments.rs` 管理稳定环境身份、连接、切换及异步会话创建／删除。Host 或 profile 改变时新增归属，不把旧历史改指向新主机；会话保留 `ProjectLocation`，后台事件在其原运行时处理。工作区版本 1 兼容迁移到版本 2，首次保存保留原始备份，不修改原生 OMP 日志。

`SessionConnection::release_idle` 消费当前所有者的连接；本地释放自己的进程，远端通过当前订阅代次请求监督服务释放空闲子进程，并在子进程退出后确认。普通 Drop 只断开远端桥，不能等价于停止任务。`SessionRuntime::releasing` 保存后台释放确认，删除或快速切回时先核对它；删除期间禁止自动重连，最终删除仍取得 OMP 原生锁，不按 PID 或路径杀其他写者。

`remote/requests.rs` 使用有界管理队列和请求期限，发生传输错误后重建通道；只读操作可重试，变更只查询受理／结果记录。`remote/response.rs` 负责有界响应分片与校验，超限通过应用错误报告，不关闭管理通道。`remote/session.rs` 为模型事件桥保留进程代次、序号和未确认操作，退避重连，不重发原生变更。大附件、会话索引／历史同步使用独立通道，新建／删除不排在历史传输之后；管理操作、文件监听与模型事件不共用阻塞流。不依赖 Windows 的 SSH ControlMaster。

远端 `NativeStore::sync` 先扫描仅含元数据的索引，再按当前 GUI 会话绑定按需读取历史。`native_index_revision` 表示文件大小／修改时间得到的索引版本，`native_revision` 仅表示已加载正文版本；元数据刷新不能把未加载正文标成最新，也不能清空已有正文、任务或子智能体。会话恢复等待原生协商完成，空闲会话重新核对日志，运行中会话查询当前消息。

Linux `daemon` 按用户／profile／协议／版本建立私有 Unix socket，每个原生会话只有一个写入者。OMP 由持续存在的会话线程创建，与 SSH 生命周期分离；父线程意外退出时通过 Linux 父死亡信号结束 OMP，避免遗留无管理的原生写入者。服务保留有界内存事件；缺口或积压过多时由 GUI 请求原生历史、状态及订阅快照，并按权威列表清理／去重交互请求。操作记录先落盘再执行，保存确认后再清 dirty；确认不明时保留编辑，必要时核对实际目标与精确内容。断线不代表任务停止，远端 OMP／服务退出不承诺工具无缝恢复。生命周期、部署和资源限制见[远程说明](remote.md)。

## 状态、交互与渲染

`Workspace` 保留会话数据库、当前会话、持久化服务和跨功能协调。输入补全、查找、文档、模型控件及原生会话同步的临时状态分别由 `ComposerState`、`SearchState`、`DocumentState`、`ModelControlState`、`SessionSync` 定义；字段和辅助类型只在工作区范围内开放。GPUI 仍通过一个工作区实体串行更新，避免为目录拆分增加相互订阅的实体。

功能目录中的控制逻辑负责处理动作与更新状态，`view` 负责构建组件和绑定动作。OMP / 文件 / 配置后台任务发出结果，GPUI 实体在 UI 线程接收并更新状态。避免在渲染函数中执行磁盘、网络或子进程操作。

`desktop/markdown.rs` 兼容原生解析器对多行公式围栏的要求，仅在显示副本中补齐定界符换行；通过 Markdown AST 排除代码示例，并记录插入位置。`TextEntry` 保留原始正文及双向字节范围映射，选区创建评论时映回原文，定位历史评论时映到显示源，不改写持久化回复；主会话、思考与子智能体展示共用处理。

`runtime/session_state.rs` 定义每个会话独立的 `SessionRuntime`，包含 OMP 连接、模型控件、输入实体、交互答复输入、队列、错误、tokens、协作状态及消息视图。`runtime/sessions.rs` 按稳定会话 ID 保留后台状态，在同一 UI 更新中临时切换所属状态并消费事件，完成后立即恢复可见会话；持久化的活动 ID 不随后台轮询变化。后台事件不修改可见文件面板或查找状态，输入订阅还校验实体身份，延迟发送校验原输入的归属。后台空闲连接释放原生占用，正在运行或等待交互的连接保留；扫描只合并不早于各会话最近事件的原生日志。

`domain/attachments.rs` 定义可序列化附件元数据，草稿及消息保存元数据，二进制不进入工作区 JSON。`filesystem/attachments.rs` 保存本地副本、校验大小和图片解码限额、转换 BMP 等图片，并生成原生 `images` 与文件引用。`composer/attachments.rs` 在后台导入，完成事件校验原会话身份；`view/attachments.rs` 组合原生 Attachment 卡片。提交、停止及断线恢复同时保留文字和附件。发送采用 OMP 原生 JSONL 输入格式，允许有界的 64 MiB 逻辑消息；接收物理帧仍保持 1 MiB 限制并重组协议 v2 分片。

OMP 适配器进一步分为：`client.rs` 管理进程及收发线程，`framing.rs` 校验帧与重组分片，`messages.rs` 提取文本，`installation.rs` 定位安装，`diagnostics.rs` 提供 `--probe`。协议解析不依赖窗口生命周期。

文件标签复用 gpui-kit `Editor` 与已有语言映射。普通文件的保存基线由 `domain/file_buffer.rs` 管理，与不可变评论快照分开；缓冲区归属各自标签，编辑订阅随标签释放。`documents/editing.rs` 协调编辑、保存和引用快照，`documents/closing.rs` 使用原生 Dialog 处理单文件三选项与批量丢弃确认，并挂接原生窗口关闭回调。`filesystem/document_store.rs` 检查打开的目标和磁盘基线，通过同目录临时文件替换完成写入，保留 BOM、换行格式和权限。构建启用 `syntect` feature 后使用组件内置的语法高亮后端，语义颜色沿用当前主题，正文及行号背景仍由统一主题设置控制。

`omp/sessions` 通过安装的 OMP 查询配置根目录，复用原生项目 JSONL 存储。`mod.rs` 负责发现与缓存，`history.rs` 读取标题槽和当前分支的消息、思考及工具结果，`paths.rs` 统一 Windows 路径和项目桶编码，`operations.rs` 创建、迁移和删除记录及附件，`lease.rs` 与 OMP 的 Windows 进程占用锁互操作。原始日志不由 GUI 消息缓存重写。

`conversation/session_sync.rs` 在后台扫描已打开项目，并在 UI 线程合并结果；工作线程只接收轻量绑定信息，历史缓存共享且按原生文件修订跳过未变化记录。流式回复期间和早于最近消息事件的扫描不覆盖活动会话。`domain/native_history.rs` 按原生消息 ID 或有序文本匹配保留 GUI 消息 ID、评论关联与历史 Diff。持久化数据新增可缺省的原生绑定字段，兼容旧版工作区 JSON。

`conversation/empty_sessions.rs` 在切换离开时清理未使用的新会话。持久化仍保留空白原生绑定，不能只从 GUI JSON 过滤；清理须先停止当前 RPC，再由原生存储取得占用锁并重新核对所有 JSONL 分支，仅初始化元数据的 GUI 新会话才可删除。草稿、评论、快照、任务、附件、备份、未知记录、读错误及外部占用均保留。

`omp/display.rs` 为实时消息、原生日志和子会话提供统一投影，区分助手、Advisor、来信等可见内容，并遵守原生隐藏标记。投影修订加入原生缓存指纹，升级后会重新生成旧展示缓存。`domain/activity.rs` 合并任务与智能体状态，阻止迟到的运行中快照覆盖完成状态。`runtime/collaboration.rs` 管理子会话后台读取，结果校验主会话、子会话和请求身份，文件限于主会话附件范围。

文件链接通过 gpui-kit `MarkdownPlugin` 和 `Link` 渲染，保留原始 Markdown 及其源范围；不重写模型正文。`filesystem/file_references.rs` 负责路径解析与相对路径边界，`documents/file_links.rs` 负责点击打开文件标签。文件增删、重命名触发行内引用重解析；显式打开的外部文件父目录加入非递归监听。任务与子智能体面板的呈现放在 `view/activity.rs`；任务阶段使用统一边框容器，任务行组合 gpui-kit `Marker`、`MarkerIcon`、`MarkerContent` 和 `Spinner`，状态名称仅用于提示与无障碍标签，不修改原生任务数据。

配置命令使用明确的导航目标，避免把服务注册入口退化成设置大类；元数据未就绪时保留导航意图，加载后完成跳转。子智能体与角色模型表单使用 gpui-kit Form、Input、Textarea、Select，字段状态属于桌面层；原生文档转换、范围隔离、字段保留及冲突检查属于 `configuration/authoring`。明确带参数的命令继续交给 OMP。

工作区布局优先组合 gpui-kit `Sidebar`、`TabBar::underline()` 与 `Toolbar`。项目标题需保留悬停新建操作，原生 `SidebarGroup` 仅支持文本标题，因此 `view/sidebar/project.rs` 用轻量 `SidebarItem` 适配标题，并用原生 `Collapsible` 展示会话列表，滚动与会话菜单仍由原生组件管理；会话条目使用稳定 ID。右侧保留原生 Tree／变更虚拟列表，不引入 Dock；三栏标题高度由视图层共用常量定义。

新增功能应放入其所属组件；跨界功能由工作区协调，不向领域层添加窗口状态，也不把界面目录树构建混入文件读取适配器。通常将文件控制在 300 行左右，但不为行数拆散紧密相关的行为。只为实际复用或依赖隔离增加抽象。

## 必须保留的行为

- 工作区 JSON 格式、数据目录、命令行参数和 OMP 协议不因模块整理改变。
- 新会话使用映射项目目录，不创建分支或工作树；旧会话的执行路径兼容保留。同目录路径别名合并，不合并不同位置的同名目录。
- 项目会话与同一 OMP profile 的终端存储同步。GUI 创建立即落盘，删除先取得原生占用锁并成功删除记录后才清理 GUI 数据；外部删除须连续确认，读失败不视为删除。归档保留为 GUI 扩展状态，评论和草稿不写入原生日志元数据。
- 评论锚点绑定原始文本或不可变代码快照。展开/收起是临时展示状态，不修改历史内容。
- 会话标签固定在最左且不可关闭；文件和变更标签可独立关闭，收起工作面板不关闭文档。切换会话清理其文档状态。异步读取结果必须匹配会话和请求，避免迟到结果覆盖当前文档。
- 查找仅针对激活标签；文件监听刷新保留当前阅读位置和目录展开状态，已修改缓冲区不接受后台读取覆盖；保存复查磁盘版本。切换会话、新建会话和清理活动会话须在产生副作用前通过未保存文档确认；外部删除原生会话时保留活动编辑缓冲区。历史快照不随磁盘修改。
- 变更列表保持平铺，颜色由 Git 状态决定；文件树使用相同颜色。状态读取保留中文、空格及重命名目标，并把子目录项目的路径转为相对于当前项目的路径。
- 排队输入在 OMP 确认接收后才进入历史；重启时未确认的输入恢复为草稿并保留状态说明。
- 模型菜单展示已配置提供商下的原生可用模型，推理档位以当前模型返回值为准。
- 服务目录合并本机目录与附带公开目录；只保存用户修改的模型覆盖字段，保留未知字段与外部修改检测。不导出或回显认证凭据。
- 控件优先使用 gpui-kit 的 Select、Switch、Tree、Collapsible、Form、Diff 和 ResizeHandle，大列表使用原生虚拟化组件。

## 测试归属与验证

确定性单元测试靠近被测实现，放在 `#[cfg(test)] mod tests` 或其 `tests.rs` 中。这符合 Rust 惯例，也能测试私有规则。协议分片测试属于 `omp/framing.rs`；GPUI 指针事件、目录树和高亮测试属于对应桌面功能。

需要安装 OMP、读取本机配置或发送模型请求的测试统一放到 `tests/omp/`，通过公开组件接口验证，使用 `#[ignore]` 显式启用。生产通信模块不再包含这类测试文件。默认测试不会调用模型或改动用户配置。

CI 执行格式检查、所有目标的 Clippy 和默认测试，配置见 [check.yml](../.github/workflows/check.yml)：

```powershell
cargo fmt --all --check
cargo clippy --workspace --locked --all-targets -- -D warnings
cargo test --workspace --locked
```

本地构建额外执行 `cargo build --locked`。当前测试清单、本机集成测试的具体名称与依赖，以及 GUI 数据／OMP 配置的隔离方法统一维护在[验证说明](testing.md)，避免在多处保留过时计数和命令。

## 服务目录更新

[`resources/omp-services.json`](../resources/omp-services.json) 是附带的公开目录 UI 元数据，来源版本由文件内 `version` 标注，授权见 [OMP-LICENSE.txt](../resources/OMP-LICENSE.txt)。这个来源版本不表示本机 OMP 版本或最低兼容版本。

在本仓库根目录，使用 Python 运行[更新脚本](../scripts/update_service_catalog.py)：

```powershell
python scripts/update_service_catalog.py D:\src\oh-my-pi
```

输入必须是包含 `packages/catalog/src/models.json`、兼容规则和 `packages/coding-agent/package.json` 的 OMP 源码目录。脚本仅提取公开字段，不读取认证凭据；不要手工维护服务枚举。更新后核对来源版本、授权与目录测试，重新验证与本机可用模型的合并。

输入框和配置表单的可用模型及推理档位来自所属环境的 OMP，不以附带目录判定账户可用性。服务注册界面则合并附带目录与该环境的目录，帮助注册尚未配置的服务；新的尚未注册内置服务可能需要随 GUI 更新元数据，也可通过自定义入口配置。
