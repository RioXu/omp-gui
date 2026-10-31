# 组件职责与维护约定

[返回 README](../README.md) · [使用说明](usage.md) · [构建与验证](testing.md)

项目使用一个 Cargo 包，二进制入口只启动桌面应用；组件放在库中，使集成测试通过真实的组件接口运行。当前不拆成多个 crate，避免为目录整理引入额外的发布和依赖管理成本。

默认在 `main` 分支直接工作，只有用户明确要求时才创建或切换其他分支；仓库约定见 [AGENTS.md](../AGENTS.md)。

## 目录与依赖方向

```text
src/
  main.rs                         二进制入口
  lib.rs                          组件边界
  domain/                         会话、消息、评论、快照和历史规则
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
  desktop/
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

允许的依赖方向是 `desktop → infrastructure → domain`，桌面层也可直接使用领域类型。`domain` 不引用 GPUI、基础设施或桌面层；`infrastructure` 不引用桌面层或 GPUI。领域层负责数据和规则，涉及文件系统、环境配置、OMP 安装路径等操作放在基础设施中。

`desktop` 是 Windows 原生客户端，明确替代含义宽泛的 `app` 命名。`workspace` 是主窗口的工作区，而不是整个程序。OMP 通信明确称为 `omp::Client`，不再用 `engine` 混合指代协议、业务和测试。

## 状态、交互与渲染

`Workspace` 保留会话数据库、当前会话、持久化服务和跨功能协调。输入补全、查找、文档、模型控件及原生会话同步的临时状态分别由 `ComposerState`、`SearchState`、`DocumentState`、`ModelControlState`、`SessionSync` 定义；字段和辅助类型只在工作区范围内开放。GPUI 仍通过一个工作区实体串行更新，避免为目录拆分增加相互订阅的实体。

功能目录中的控制逻辑负责处理动作与更新状态，`view` 负责构建组件和绑定动作。OMP / 文件 / 配置后台任务发出结果，GPUI 实体在 UI 线程接收并更新状态。避免在渲染函数中执行磁盘、网络或子进程操作。

OMP 适配器进一步分为：`client.rs` 管理进程及收发线程，`framing.rs` 校验帧与重组分片，`messages.rs` 提取文本，`installation.rs` 定位安装，`diagnostics.rs` 提供 `--probe`。协议解析不依赖窗口生命周期。

文件标签复用 gpui-kit `Editor` 与已有语言映射。构建启用 `syntect` feature 后使用组件内置的语法高亮后端，语义颜色沿用当前主题，正文及行号背景仍由统一主题设置控制。

`omp/sessions` 通过安装的 OMP 查询配置根目录，复用原生项目 JSONL 存储。`mod.rs` 负责发现与缓存，`history.rs` 读取标题槽和当前分支的消息、思考及工具结果，`paths.rs` 统一 Windows 路径和项目桶编码，`operations.rs` 创建、迁移和删除记录及附件，`lease.rs` 与 OMP 的 Windows 进程占用锁互操作。原始日志不由 GUI 消息缓存重写。

`conversation/session_sync.rs` 在后台扫描已打开项目，并在 UI 线程合并结果；工作线程只接收轻量绑定信息，历史缓存共享且按原生文件修订跳过未变化记录。流式回复期间和早于最近消息事件的扫描不覆盖活动会话。`domain/native_history.rs` 按原生消息 ID 或有序文本匹配保留 GUI 消息 ID、评论关联与历史 Diff。持久化数据新增可缺省的原生绑定字段，兼容旧版工作区 JSON。

`conversation/empty_sessions.rs` 在切换离开时清理未使用的新会话。持久化仍保留空白原生绑定，不能只从 GUI JSON 过滤；清理须先停止当前 RPC，再由原生存储取得占用锁并重新核对所有 JSONL 分支，仅初始化元数据的 GUI 新会话才可删除。草稿、评论、快照、任务、附件、备份、未知记录、读错误及外部占用均保留。

`omp/display.rs` 为实时消息、原生日志和子会话提供统一投影，区分助手、Advisor、来信等可见内容，并遵守原生隐藏标记。投影修订加入原生缓存指纹，升级后会重新生成旧展示缓存。`domain/activity.rs` 合并任务与智能体状态，阻止迟到的运行中快照覆盖完成状态。`runtime/collaboration.rs` 管理子会话后台读取，结果校验主会话、子会话和请求身份，文件限于主会话附件范围。

文件链接通过 gpui-kit `MarkdownPlugin` 和 `Link` 渲染，保留原始 Markdown 及其源范围；不重写模型正文。`filesystem/file_references.rs` 负责路径解析与相对路径边界，`documents/file_links.rs` 负责点击打开文件标签。文件增删、重命名触发行内引用重解析；显式打开的外部文件父目录加入非递归监听。任务与子智能体面板的呈现放在 `view/activity.rs`。

配置命令使用明确的导航目标，避免把服务注册入口退化成设置大类；元数据未就绪时保留导航意图，加载后完成跳转。子智能体与角色模型表单使用 gpui-kit Form、Input、Textarea、Select，字段状态属于桌面层；原生文档转换、范围隔离、字段保留及冲突检查属于 `configuration/authoring`。明确带参数的命令继续交给 OMP。

工作区布局优先组合 gpui-kit `Sidebar`、`TabBar::underline()` 与 `Toolbar`。项目标题需保留悬停新建操作，原生 `SidebarGroup` 仅支持文本标题，因此 `view/sidebar/project.rs` 用轻量 `SidebarItem` 适配标题，并用原生 `Collapsible` 展示会话列表，滚动与会话菜单仍由原生组件管理；会话条目使用稳定 ID。右侧保留原生 Tree／变更虚拟列表，不引入 Dock；三栏标题高度由视图层共用常量定义。

新增功能应放入其所属组件；跨界功能由工作区协调，不向领域层添加窗口状态，也不把界面目录树构建混入文件读取适配器。通常将文件控制在 300 行左右，但不为行数拆散紧密相关的行为。只为实际复用或依赖隔离增加抽象。

## 必须保留的行为

- 工作区 JSON 格式、数据目录、命令行参数和 OMP 协议不因模块整理改变。
- 新会话使用映射项目目录，不创建分支或工作树；旧会话的执行路径兼容保留。同目录路径别名合并，不合并不同位置的同名目录。
- 项目会话与同一 OMP profile 的终端存储同步。GUI 创建立即落盘，删除先取得原生占用锁并成功删除记录后才清理 GUI 数据；外部删除须连续确认，读失败不视为删除。归档保留为 GUI 扩展状态，评论和草稿不写入原生日志元数据。
- 评论锚点绑定原始文本或不可变代码快照。展开/收起是临时展示状态，不修改历史内容。
- 会话标签固定在最左且不可关闭；文件和变更标签可独立关闭，收起工作面板不关闭文档。切换会话清理其文档状态。异步读取结果必须匹配会话和请求，避免迟到结果覆盖当前文档。
- 查找仅针对激活标签；文件监听刷新保留当前阅读位置和目录展开状态，历史快照不随磁盘修改。
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
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
```

本地构建额外执行 `cargo build --locked`。当前测试清单、本机集成测试的具体名称与依赖，以及 GUI 数据／OMP 配置的隔离方法统一维护在[验证说明](testing.md)，避免在多处保留过时计数和命令。

## 服务目录更新

[`resources/omp-services.json`](../resources/omp-services.json) 是附带的公开目录 UI 元数据，来源版本由文件内 `version` 标注，授权见 [OMP-LICENSE.txt](../resources/OMP-LICENSE.txt)。这个来源版本不表示本机 OMP 版本或最低兼容版本。

在本仓库根目录，使用 Python 运行[更新脚本](../scripts/update_service_catalog.py)：

```powershell
python scripts/update_service_catalog.py D:\src\oh-my-pi
```

输入必须是包含 `packages/catalog/src/models.json`、兼容规则和 `packages/coding-agent/package.json` 的 OMP 源码目录。脚本仅提取公开字段，不读取认证凭据；不要手工维护服务枚举。更新后核对来源版本、授权与目录测试，重新验证与本机可用模型的合并。

输入框和配置表单的可用模型及推理档位来自本机 OMP，不以附带目录判定账户可用性。服务注册界面则合并附带目录与本机目录，帮助注册尚未配置的服务；新的尚未注册内置服务可能需要随 GUI 更新元数据，也可通过自定义入口配置。
