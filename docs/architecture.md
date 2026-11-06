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

`remote/requests.rs` 使用有界管理队列和请求期限，发生传输错误后重建通道；只读操作可重试，变更只查询受理／结果记录。`remote/wire.rs` 和 `remote/wire/serde_codec.rs` 将现有 Serde 领域类型直接映射为平坦的 rkyv 归档，并在校验后直接反序列化，不经过 JSON 文本或中间 Value 树；`remote/response.rs` 负责有界二进制响应与校验，超限通过应用错误报告，不关闭管理通道。`remote/session.rs` 为模型事件桥保留进程代次、序号和未确认操作，退避重连，不重发原生变更。大附件、会话索引／历史同步使用独立通道，新建／删除不排在历史传输之后；管理操作、文件监听与模型事件不共用阻塞流。不依赖 Windows 的 SSH ControlMaster。

远端 `NativeStore::sync` 先扫描仅含元数据的索引，再按当前 GUI 会话绑定按需读取历史。`native_index_revision` 表示文件大小／修改时间得到的索引版本，`native_revision` 仅表示已加载正文版本；元数据刷新不能把未加载正文标成最新，也不能清空已有正文、任务或子智能体。会话恢复等待原生协商完成，空闲会话重新核对日志，运行中会话查询当前消息。

Linux `daemon` 按用户／profile／协议／版本建立私有 Unix socket，每个原生会话只有一个写入者。OMP 由持续存在的会话线程创建，与 SSH 生命周期分离；父线程意外退出时通过 Linux 父死亡信号结束 OMP，避免遗留无管理的原生写入者。服务保留有界内存事件；缺口或积压过多时由 GUI 请求原生历史、状态及订阅快照，并按权威列表清理／去重交互请求。操作记录先落盘再执行，保存确认后再清 dirty；确认不明时保留编辑，必要时核对实际目标与精确内容。断线不代表任务停止，远端 OMP／服务退出不承诺工具无缝恢复。生命周期、部署和资源限制见[远程说明](remote.md)。

## 状态、交互与渲染

`view/thinking.rs` 的预览与全文共用 Markdown 渲染，思考字号小于正文并带边框；预览限制为最新五行。历史思考默认折叠，本次运行中展示过的预览和过程在完成后保留，用户可手动收起。`SessionRuntime::expanded_process` 和 `thinking_previews` 记录每会话的展示状态，不改写原文或原生历史。

`conversation/transcript_layout.rs` 使用原生 Tail 跟随模式：停留最新页底部时追踪输出，上滚暂停，回到底部恢复；历史页使用 Normal 模式；已有行用 `ListState::remeasure_items` 保留高度提示及绝对锚点，仅对新增／移除行使用 `splice`。列表重建时只为未跟随的视图恢复具体阅读锚点，避免覆盖原生末尾跟随。`runtime/attention.rs` 在模型输出结束及交互请求处标记会话末尾跳转，待正文同步后执行；后台会话保留标记至激活，不改变当前会话。窗口未聚焦时复用 gpui-kit 的系统通知，按会话去重并支持点击定位；启动时设置应用身份。阅读位置验证须等待 Markdown 解析与绘制完成，不能只比较事件处理后的逻辑下标。

`view/composer_tasks.rs` 复用 `render_todos`，通过 gpui-kit Popover 在按钮上方显示覆盖式任务浮层，按会话控制展开状态；浮层不参与主布局，不挤动会话、输入框和动作摘要。上下文圆环位于模型选择器左侧。流式搜索刷新仅更新高亮，显式搜索动作才定位。

`desktop/computer_overlay.rs` 根据本机会话实时事件维护临时操作提示，不从恢复的历史启动提示。各会话持有独立的 Activity，结束／停止／等待交互／断线及释放时撤下；原生服务按会话所有者合并占用。Windows 实现位于 `desktop/computer_overlay/windows.rs`，使用 gpui-kit 尚未暴露的穿透、无激活、置顶及截图排除窗口能力；系统模拟点击与按键仅生成有界、短暂的视觉反馈，不持久化输入原文。窗口和输入钩子在无人操作时释放，反馈过期后停用计时器；输入钩子始终转发事件，不参与实际执行。屏幕提示仅支持本机 Windows 10 2004 及以上，远端会话不触发本机提示。

`Workspace` 保留会话数据库、当前会话、持久化服务和跨功能协调。输入补全、查找、文档、模型控件及原生会话同步的临时状态分别由 `ComposerState`、`SearchState`、`DocumentState`、`ModelControlState`、`SessionSync` 定义；字段和辅助类型只在工作区范围内开放。GPUI 仍通过一个工作区实体串行更新，避免为目录拆分增加相互订阅的实体。

功能目录中的控制逻辑负责处理动作与更新状态，`view` 负责构建组件和绑定动作。OMP / 文件 / 配置后台任务发出结果，GPUI 实体在 UI 线程接收并更新状态。避免在渲染函数中执行磁盘、网络或子进程操作。

`desktop/markdown.rs` 兼容原生解析器对多行公式围栏的要求，仅在显示副本中补齐定界符换行；通过 Markdown AST 排除代码示例，并记录插入位置。`TextEntry` 保留原始正文及双向字节范围映射，选区创建评论时映回原文，定位历史评论时映到显示源，不改写持久化回复；主会话、思考与子智能体展示共用处理。

会话消息、插入通知及输入区共享可用宽度，保留会话区域两侧各 10% 的边距，不再使用固定 52 rem 上限。`omp/intent.rs` 投影 OMP 终端同源的工具意图：工具执行事件的 intent，以及流式工具调用 arguments.i／intent；忽略非字符串和空值，采用最后一个有效意图，整理为单行。`runtime/working_intent.rs` 管理每会话临时摘要，开始新任务、完成、断开或空闲状态确认时清除，停止期间忽略迟到更新；不从思考正文推测摘要，也不改变持久化消息。`view/working_intent.rs` 在输入框上方组合原生 Spinner、状态语义和完整文字提示，长文字单行截断。

`runtime/session_state.rs` 定义每个会话独立的 `SessionRuntime`，包含 OMP 连接、模型控件、输入实体、交互答复输入、队列、错误、tokens、协作状态及消息视图。`runtime/sessions.rs` 按稳定会话 ID 保留后台状态，在同一 UI 更新中临时切换所属状态并消费事件，完成后立即恢复可见会话；持久化的活动 ID 不随后台轮询变化。后台事件不修改可见文件面板或查找状态，输入订阅还校验实体身份，延迟发送校验原输入的归属。后台空闲连接释放原生占用，正在运行或等待交互的连接保留；扫描只合并不早于各会话最近事件的原生日志。

`domain/attachments.rs` 定义可序列化附件元数据，草稿及消息保存元数据，二进制不进入工作区 JSON。`filesystem/attachments.rs` 保存本地副本、校验大小和图片解码限额、转换 BMP 等图片，并生成原生 `images` 与文件引用。`composer/attachments.rs` 在后台导入，完成事件校验原会话身份；`view/attachments.rs` 组合原生 Attachment 卡片。提交、停止及断线恢复同时保留文字和附件。发送采用 OMP 原生 JSONL 输入格式，允许有界的 64 MiB 逻辑消息；接收物理帧仍保持 1 MiB 限制并重组协议 v2 分片。

OMP 适配器进一步分为：`client.rs` 管理进程及收发线程，`framing.rs` 校验帧与重组分片，`messages.rs` 提取文本，`installation.rs` 定位安装，`diagnostics.rs` 提供 `--probe`。接收线程复用可变行缓冲，Decoder 为每个连接保留 simd-json Buffers；小于 256 KiB 的帧直接原地解析，较大的帧和重组后的正文由 sonic-rs 解析。按大小分流覆盖常见的大块 base64 分片，避免为了识别事件类型先解析一遍正文；该阈值是实现策略，不代表所有负载的通用性能交点。两条路径仍输出拥有自身字符串的 serde_json::Value，事件不引用可变接收缓冲。保留原有物理帧、逻辑消息和分片顺序校验；两条解析路径保持最多 127 层容器嵌套，大帧预检查用 memchr 跳过字符串及转义，避免深嵌套耗尽线程栈。simd-json 启用 big-int-as-float，保留原解析器对超出 64 位整数的浮点退化行为。协议解析不依赖窗口生命周期。

文件标签复用 gpui-kit `Editor` 与已有语言映射。普通文件的保存基线由 `domain/file_buffer.rs` 管理，与不可变评论快照分开；缓冲区归属各自标签，编辑订阅随标签释放。`documents/editing.rs` 协调编辑、保存和引用快照，`documents/closing.rs` 使用原生 Dialog 处理单文件三选项与批量丢弃确认，并挂接原生窗口关闭回调。`filesystem/document_store.rs` 检查打开的目标和磁盘基线，通过同目录临时文件替换完成写入，保留 BOM、换行格式和权限。构建启用 `syntect` feature 后使用组件内置的语法高亮后端，语义颜色沿用当前主题，正文及行号背景仍由统一主题设置控制。

`omp/sessions` 通过安装的 OMP 查询配置根目录，复用原生项目 JSONL 存储。`mod.rs` 负责发现与缓存，`history.rs` 读取标题槽和当前分支的消息、思考及工具结果，`paths.rs` 统一 Windows 路径和项目桶编码，`operations.rs` 创建、迁移和删除记录及附件，`lease.rs` 与 OMP 的 Windows 进程占用锁互操作。原始日志不由 GUI 消息缓存重写。

`omp/owned_process.rs` 在 Windows 为每个会话传输及 OMP 配置命令创建匿名、不可继承的 Job Object，设置 `KILL_ON_JOB_CLOSE`。子进程先暂停启动，加入 Job 后再恢复，避免包装进程先派生未托管的 Bun；失败路径终止并等待根进程。正常释放终止所属进程树，GUI 被强制结束后系统关闭 Job 句柄并回收后代。配置命令同时排空 stdout／stderr，在根进程结束后回收后代，凭据输入错误同样触发 RAII 清理。清理只根据持有的进程和 Job 句柄，不枚举或按名称结束系统进程。Linux 会话监督策略不变。

`conversation/session_sync.rs` 在后台扫描已打开项目，并在 UI 线程合并结果；工作线程只接收轻量绑定信息，历史缓存共享且按原生文件修订跳过未变化记录。流式回复期间和早于最近消息事件的扫描不覆盖活动会话。`domain/native_history.rs` 按原生消息 ID 或有序文本匹配保留 GUI 消息 ID、评论关联与历史 Diff。持久化数据新增可缺省的原生绑定字段，兼容旧版工作区 JSON。

`conversation/empty_sessions.rs` 在切换离开时清理未使用的新会话。持久化仍保留空白原生绑定，不能只从 GUI JSON 过滤；清理须先停止当前 RPC，再由原生存储取得占用锁并重新核对所有 JSONL 分支，仅初始化元数据的 GUI 新会话才可删除。草稿、评论、快照、任务、附件、备份、未知记录、读错误及外部占用均保留。

`omp/display.rs` 为实时消息、原生日志和子会话提供统一投影，区分助手、Advisor、来信等可见内容，并遵守原生隐藏标记。投影修订加入原生缓存指纹，升级后会重新生成旧展示缓存。`domain/activity.rs` 合并任务与智能体状态，阻止迟到的运行中快照覆盖完成状态。`runtime/collaboration.rs` 管理子会话后台读取，结果校验主会话、子会话和请求身份，文件限于主会话附件范围。

文件链接通过 gpui-kit `MarkdownPlugin` 和 `Link` 渲染，保留原始 Markdown 及其源范围；不重写模型正文。`filesystem/file_references.rs` 负责路径解析与相对路径边界，`documents/file_links.rs` 负责点击打开文件标签。文件增删、重命名触发行内引用重解析；显式打开的外部文件父目录加入非递归监听。任务内容放在 `view/activity.rs`，任务浮层入口位于 `view/composer_tasks.rs`；子智能体列表及只读历史放在 `view/subagents.rs`，列表只呈现 role、状态、标题，点击后在现有 TabBar 中打开子会话标签。子会话标签按主会话和智能体 ID 隔离，不作为项目文件读取或保存；活动子会话独立于面板开关定期刷新。任务阶段使用统一边框容器，任务行组合 gpui-kit `Marker`、`MarkerIcon`、`MarkerContent` 和 `Spinner`，状态名称仅用于提示与无障碍标签，不修改原生任务数据。

配置命令使用明确的导航目标，避免把服务注册入口退化成设置大类；元数据未就绪时保留导航意图，加载后完成跳转。子智能体与角色模型表单使用 gpui-kit Form、Input、Textarea、Select，字段状态属于桌面层；原生文档转换、范围隔离、字段保留及冲突检查属于 `configuration/authoring`。明确带参数的命令继续交给 OMP。

`settings/agents.rs` 提供独立的子智能体管理页，沿用全局／项目范围和既有编辑表单；角色与提示词页只保留角色映射及系统提示词。配置目录扫描同时生成 `AgentEntry` 摘要，格式错误的文件仍列出并允许源码修复；旧目录响应缺少摘要时可退回文件名列表。进入管理页后，通过所选环境执行 `omp agents unpack --dir ... --json` 自动导入当前范围，不使用覆盖标志；按范围记录导入状态，避免刷新后重复导入，手动刷新允许重试失败。`Configuration::join` 与 `display_path` 将 Windows 扩展盘符／UNC 路径转为普通形式，POSIX 路径不经 Windows 处理。移除通过 `ConfigAction::RemoveAgent` 在所属环境检查目录和文件版本，改名为带唯一标识的 `.bak`；不操作其他范围或内置定义。工具调用页的 Eval 定位入口仅过滤原生 `eval.*` 条目，不修改开关。

工作区布局优先组合 gpui-kit `Sidebar`、`TabBar::underline()` 与 `Toolbar`。项目标题需保留悬停新建操作，原生 `SidebarGroup` 仅支持文本标题，因此 `view/sidebar/project.rs` 用轻量 `SidebarItem` 适配标题。项目标题与各会话分别作为原生 Sidebar 虚拟列表中的条目，折叠项目时不生成子条目；`view/sidebar/cache.rs` 缓存按项目分组及时间排序，标题、归属、归档、删除或更新时间变化时重建，消息正文变化复用缓存。菜单只在可见条目渲染时构建，点击按稳定会话 ID 重新定位。右侧保留原生 Tree／变更虚拟列表，不引入 Dock；三栏标题高度由视图层共用常量定义。

`domain/session_list.rs` 用按会话写时复制的共享存储提供轻量工作区快照，序列化仍为原来的 JSON 会话数组。保存时 UI 线程只提交共享快照，后台用 sonic-rs 编码并执行原子写盘；一个正在处理的快照和至多一个最新待处理快照取代无界保存队列，关闭时等待最后一次保存。首次修改仍被保存线程持有的会话会复制该会话，其他会话保持共享。

流式消息用消息 ID 索引和待更新下标合并批次，仅同步变化的 TextView；消息数量不变时复用列表分组，追加消息时重建受影响的尾部，只使对应尾部列表高度失效。原生历史替换或排队消息重排仍重建当前显示页，并清理页外文本缓存。后台会话只更新领域数据，显示同步延迟到激活。

空闲后台运行时缓存按最近访问时间回收：最多 3 个，文本源大小乘 4 的估算预算合计 32 MiB，最长保留 120 秒。预算限制可重建视图缓存，不代表进程总内存上限；领域历史仍在工作区中。运行、重连、准备输入、等待交互、子会话加载及尚未完成的连接释放确认受到保护。无需处理事件的空闲运行时跳过事件轮询，淘汰后从领域数据恢复草稿和消息视图。

新增功能应放入其所属组件；跨界功能由工作区协调，不向领域层添加窗口状态，也不把界面目录树构建混入文件读取适配器。通常将文件控制在 300 行左右，但不为行数拆散紧密相关的行为。只为实际复用或依赖隔离增加抽象。

### 会话分页与搜索状态

过程记录在 `view/process_view.rs` 中独立虚拟化：较短过程直接展示，长过程使用固定高度的原生 List，仅为可视区及小幅预加载范围创建步骤组件。长工具文本使用只读 Editor 的可视行排版；List 状态与文本状态在会话运行时中复用，翻历史页时清理；编辑器源文本计入空闲缓存的估算预算。这里不做步骤或工具文本分页，消息历史的 100 条分页仍是独立规则。搜索和评论定位同时滚动外层会话和内层过程列表。完整文本读取、编辑器初始化与 Markdown 首次解析仍有成本，虚拟化只限制界面创建和布局范围。

`SessionRuntime.history_start/history_end` 只控制当前展示窗口，`None` 表示跟随最新页；每页最多 100 条消息，原始 `Session.messages` 不裁剪。翻页重建该页的 TextView 并清理工具 Diff 缓存，消息索引仍指向原始历史。回合可在页边界拆分；历史评论通过原始消息下标切页。此上限不限制单条消息大小、工作区持久化体积或远端传输量。

子智能体的 `CollaborationState` 独立保存页范围、分组、ListState 和正文缓存；相同正文复用 TextViewState。主会话与子智能体复用虚拟行的延后定位逻辑，避免同帧重排造成无障碍节点重复。

搜索在激活会话的当前页收集正文与思考块，使用 Markdown 解析后的 RenderedText 范围匹配和高亮。思考搜索状态按块保存，定位时同时展开回合和思考块。异步解析只完成用户已发起的定位请求；流式新增命中不自动跳转。换页、切换搜索范围和关闭搜索会清理失效状态。工具参数及输出不加入搜索索引。

### 交互回复与输入恢复

`preferences::answer_request` 在交互响应成功写入传输后，将非空、非敏感的 input/editor 回复保存为 GUI 用户消息，`details.omp_gui_interaction` 记录请求编号。它不是额外的原生 prompt；`domain/native_history.rs` 按相邻消息锚点保留 GUI 回复，并排除同文原生用户输入的误匹配。传输失败时不移除问题或清空输入。

单项取消仅使用原生 `remove_queued_message(queue="followUp")`，等待匹配请求编号的 `removed=true` 后再恢复文字、附件和未执行评论；模型已接收或请求失败时不伪造撤回。取消请求在所属 SessionRuntime 中跟踪，防止后台缓存回收和空闲连接释放丢失确认。

历史输入浏览状态属于当前会话的 ComposerState，从完整 GUI 用户消息提取展示文字，排除模型正文及 wire 中附加的评论上下文。只有空输入框或仍未改动的历史恢复文本接管方向键，补全菜单优先；修改后继续普通多行编辑，不自动恢复附件或提交输入。

## 必须保留的行为

- 工作区 JSON 格式、数据目录、命令行参数和 OMP 协议不因模块整理改变。
- 新会话使用映射项目目录，不创建分支或工作树；旧会话的执行路径兼容保留。同目录路径别名合并，不合并不同位置的同名目录。
- 项目会话与同一 OMP profile 的终端存储同步。GUI 创建立即落盘，删除先取得原生占用锁并成功删除记录后才清理 GUI 数据；外部删除须连续确认，读失败不视为删除。归档保留为 GUI 扩展状态，评论和草稿不写入原生日志元数据。
- 评论锚点绑定原始文本或不可变代码快照。展开／收起是临时展示状态，不修改历史内容。流式增量不得切换可见会话或抢占键盘焦点；最新页底部跟随输出，上滚及历史分页保留阅读锚点；完成及交互请求显式返回对应会话最新页和末尾，后台会话不切走前台。正文完成时不得自动折叠已展示的过程。
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
cargo clippy --workspace --locked --all-targets --all-features -- -D warnings
cargo test --workspace --locked
```

本地构建额外执行 `cargo build --locked`。当前测试清单、本机集成测试的具体名称与依赖，以及 GUI 数据／OMP 配置的隔离方法统一维护在[验证说明](testing.md)，避免在多处保留过时计数和命令。

开发构建对工作区代码采用 `opt-level=1`、依赖库采用 `opt-level=2`，保留增量编译与调试断言；避免未优化的 GPUI 布局、文字与场景处理阻塞动画。首次依赖构建更慢，日常增量构建复用依赖。单元测试保留 `opt-level=0`，原生动画性能以优化后的开发程序实测，不以测试的模拟时钟判定。默认不包含分析器，显式 `frame-profile` 功能可启用原生 Draw／Present 采样；隔离演示窗口的采样方法与历史数据见[历史验证](validation-history.md)。

默认后端仍为 `gpui-pre 0.3.10`；显式 `gpui-fast` 功能通过 gpui-kit 的官方接口将核心、平台与组件统一切换到 `gpui-fast 0.1.12`，无需改应用导入或替换上游源码。分析器与测试支持也通过 Kit 转发，确保实际采样和指针事件测试使用所选后端。Fast 当前要求 `taffy 0.15.0`、`unicode-properties 0.1.3`，锁文件统一使用兼容版本；`0.1.13` 要求另一组 GPUI 辅助库，不与本项目固定的 Kit 版本混用。Fast 仍为实验性选择，具体测量与验证范围见[验证说明](validation-history.md#gpui-fast-后端对比)。

## 服务目录更新

[`resources/omp-services.json`](../resources/omp-services.json) 是附带的公开目录 UI 元数据，来源版本由文件内 `version` 标注，授权见 [OMP-LICENSE.txt](../resources/OMP-LICENSE.txt)。这个来源版本不表示本机 OMP 版本或最低兼容版本。

在本仓库根目录，使用 Python 运行[更新脚本](../scripts/update_service_catalog.py)：

```powershell
python scripts/update_service_catalog.py D:\src\oh-my-pi
```

输入必须是包含 `packages/catalog/src/models.json`、兼容规则和 `packages/coding-agent/package.json` 的 OMP 源码目录。脚本仅提取公开字段，不读取认证凭据；不要手工维护服务枚举。更新后核对来源版本、授权与目录测试，重新验证与本机可用模型的合并。

输入框和配置表单的可用模型及推理档位来自所属环境的 OMP，不以附带目录判定账户可用性。服务注册界面则合并附带目录与该环境的目录，帮助注册尚未配置的服务；新的尚未注册内置服务可能需要随 GUI 更新元数据，也可通过自定义入口配置。

数学排版通过原生 Markdown 扩展接入 [RaTeX](https://github.com/erweixin/RaTeX)，在解析阶段生成带字体轮廓的 SVG，再由 GPUI 绘制；缓存上限为 128 项／4 MiB。内嵌 KaTeX 字体的许可见 [OFL](../resources/KaTeX-OFL.txt)。公式节点保留原始 LaTeX 与源区间，皮肤切换复用几何数据并采用当前文字颜色。

Windows 程序图标采用黄色（`#FFD43B`）倾斜的 π 与感叹号，代表 Oh My Pi。矢量母版为 [omp.svg](../resources/omp.svg)，使用 `uv run scripts/generate-icon.py` 导出透明背景的 [omp.png](../resources/omp.png) 和 [omp.ico](../resources/omp.ico)；ICO 包含 16、20、24、32、40、48、64、96、128、256 px 的 32 位图像。构建脚本使用 `embed-resource` 将 [omp-gui.rc](../resources/omp-gui.rc) 仅链接到 GUI 二进制；图标资源 ID 为 1，与 GPUI 原生窗口加载规则一致。安装程序也使用同一 ICO 文件。


### v0.8.4 远端性能边界

应用通信采用管理协议 5：SSH stdio 与监督 Unix socket 均使用 OMR5／长度前缀／rkyv 归档。命令行版本查询和部署 shell 保持普通 CLI 语义；原生 OMP 子进程一侧保持 JSONL RPC v2，磁盘 JSON／JSONL 和操作确认记录格式不变。附件上传使用原始字节，不经过 Base64。

SessionStore 发现结果在进程内有界共享，按 cwd／OMP／profile 环境隔离并定期重查。索引通过 notify 维护变化目录集合和完整补查时限；历史缓存按源文件大小／mtime 校验。历史逐行解析，工具结果通过调用 ID 索引定位。文件通知携带合并路径，桌面保留定期完整刷新，普通事件选择性重读标签；路径匹配保留 POSIX 反斜线语义。

附着流程在全局注册表锁外发现配置及验证路径，再持锁二次检查并注册唯一 actor。Client 输出事件使用有界 crossbeam channel，actor 用 select 同时等待事件、输入和退出期限，保留 owner／generation／原生 lease 保护及普通断线不终止任务的语义。

通知显示通过 domain::notice::display_body 生成运行时 TextEntry 的展示副本，保留 Message.text 和原生 JSONL。domain::notice::lsp 将 details.files 中的 OMP 字符串、标准 Diagnostic 对象及 publishDiagnostics 参数归一为文件／诊断／原文块；缺少结构化数据时回退解析原文。兼容数字严重等级、零基 range（显示时加一）、字符串／数字 code、多行 message、relatedInformation 和 file URI。显示副本使用 omp-lsp 围栏传递类型化块，由 view/notice/lsp.rs 的原生 MarkdownPlugin 渲染 gpui-kit Marker；块提供可搜索文本及源范围，诊断字符按字面呈现。文件路径用 gpui-kit 原生 Link 承载可换行普通文本，保留鼠标／键盘打开，元数据自然换行，状态图标和主题颜色与任务面板一致。通知复用 gpui-kit TextView.max_lines(3)，展开状态保存在所属 SessionRuntime 的消息键中；定位评论和搜索时先展开再 reveal，底部短历史展开时显式锚定消息，避免标题被挤出视口。
