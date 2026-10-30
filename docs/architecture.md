# 组件职责与维护约定

项目使用一个 Cargo 包，二进制入口只启动桌面应用；组件放在库中，使集成测试通过真实的组件接口运行。当前不拆成多个 crate，避免为目录整理引入额外的发布和依赖管理成本。

## 目录与依赖方向

```text
src/
  main.rs                         二进制入口
  lib.rs                          组件边界
  domain/                         会话、消息、评论、快照和历史规则
  infrastructure/
    omp/                          OMP 子进程、JSONL 协议与安装检测
    filesystem/                   项目文件、Git 差异与目录身份归一化
    persistence/                  数据目录锁、原子保存与恢复
    configuration/
      settings.rs                 OMP 配置目录、范围、缓存和读写
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

`Workspace` 保留会话数据库、当前会话、持久化服务和跨功能协调。输入补全、查找、文档及模型控件的临时状态分别由 `ComposerState`、`SearchState`、`DocumentState`、`ModelControlState` 定义；字段和辅助类型只在工作区范围内开放。GPUI 仍通过一个工作区实体串行更新，避免为目录拆分增加相互订阅的实体。

功能目录中的控制逻辑负责处理动作与更新状态，`view` 负责构建组件和绑定动作。OMP / 文件 / 配置后台任务发出结果，GPUI 实体在 UI 线程接收并更新状态。避免在渲染函数中执行磁盘、网络或子进程操作。

OMP 适配器进一步分为：`client.rs` 管理进程及收发线程，`framing.rs` 校验帧与重组分片，`messages.rs` 提取文本，`installation.rs` 定位安装，`diagnostics.rs` 提供 `--probe`。协议解析不依赖窗口生命周期。

新增功能应放入其所属组件；跨界功能由工作区协调，不向领域层添加窗口状态，也不把界面目录树构建混入文件读取适配器。通常将文件控制在 300 行左右，但不为行数拆散紧密相关的行为。只为实际复用或依赖隔离增加抽象。

## 必须保留的行为

- 工作区 JSON 格式、数据目录、命令行参数和 OMP 协议不因模块整理改变。
- 新会话使用映射项目目录，不创建分支或工作树；旧会话的执行路径兼容保留。同目录路径别名合并，不合并不同位置的同名目录。
- 评论锚点绑定原始文本或不可变代码快照。展开/收起是临时展示状态，不修改历史内容。
- 会话标签固定在最左且不可关闭；文件和变更标签可独立关闭，收起工作面板不关闭文档。切换会话清理其文档状态。异步读取结果必须匹配会话和请求，避免迟到结果覆盖当前文档。
- 查找仅针对激活标签；文件监听刷新保留当前阅读位置和目录展开状态，历史快照不随磁盘修改。
- 排队输入在 OMP 确认接收后才进入历史；重启时未确认的输入恢复为草稿并保留状态说明。
- 模型菜单展示已配置提供商下的原生可用模型，推理档位以当前模型返回值为准。
- 服务目录合并本机目录与附带公开目录；只保存用户修改的模型覆盖字段，保留未知字段与外部修改检测。不导出或回显认证凭据。
- 控件优先使用 gpui-kit 的 Select、Switch、Tree、Collapsible、Form、Diff 和 ResizeHandle，大列表使用原生虚拟化组件。

## 测试归属与验证

确定性单元测试靠近被测实现，放在 `#[cfg(test)] mod tests` 或其 `tests.rs` 中。这符合 Rust 惯例，也能测试私有规则。协议分片测试属于 `omp/framing.rs`；GPUI 指针事件、目录树和高亮测试属于对应桌面功能。

需要安装 OMP、读取本机配置或发送模型请求的测试统一放到 `tests/omp/`，通过公开组件接口验证，使用 `#[ignore]` 显式启用。生产通信模块不再包含这类测试文件。默认测试不会调用模型或改动用户配置。

CI 执行格式检查、所有目标的 Clippy 和离线测试：

```powershell
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --locked
```

按需验证本机 OMP（真实模型测试会消耗额度，须已有相应授权）：

```powershell
cargo test --test omp_integration installed_registry -- --ignored --nocapture
cargo test --test omp_integration installed_catalog -- --ignored --nocapture
cargo test --test omp_integration live_process_stream -- --ignored --nocapture
```

界面回归使用独立 `--data-dir` 和测试项目。读取真实账号时不修改配置；保存测试使用独立 OMP 配置目录或测试项目范围。

## 服务目录更新

`resources/omp-services.json` 是 OMP 18.14.0 官方公开目录的 UI 元数据，授权见同目录 `OMP-LICENSE.txt`。通过 `scripts/update_service_catalog.py <OMP源码目录>` 重新生成；不要手工维护服务枚举。运行时本机可用目录优先，新增的已注册服务自动合并；新的尚未注册内置服务需随 GUI 更新目录，也可使用自定义入口。
