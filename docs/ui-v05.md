# v0.5 会话交互与视觉修订

[返回 README](../README.md) · [当前使用说明](usage.md) · [变更记录](changelog.md)

本页是 v0.5 的历史设计记录，保留当时的决策与验证范围。当前交互以[使用说明](usage.md)为准。

当时参考了 [UI/UX Pro Max](https://github.com/nextlevelbuilder/ui-ux-pro-max-skill) 的开发工具检索与通用规则，保持既有简洁基线。技术栈为 Rust / gpui-kit；这份历史参考不属于项目构建依赖，也未套用 Web、移动端或 WinUI 的实现规则。

## 应用的规则

- 使用语义色统一深浅主题：背景、侧栏、输入区域、边框及次要文本保持清楚层级；主按钮使用蓝色。
- 保持 4/8 px 间距节奏、统一文本宽度和原生矢量图标；过程状态降为次要层级。
- 运行中提供明确的反馈与停止入口，输入框保持可编辑；指令提交等待确认期间避免重复发送。
- 折叠与展开保留阅读位置，不强制滚到底部。用户已经向上阅读时，新增流式内容不夺回滚动控制。
- 原生虚拟化 List、Collapsible、Spinner、TextView 和 Diff 负责展示；隐藏的整轮过程不创建工具详情内容。

设计检索匹配开发工具的简洁方向；“折叠/滚动”专项检索两次未获得合适的桌面条目，采用通用可预测交互原则，并根据 GPUI 的滚动锚点 API 验证。

## 交互语义

一次连续的模型回复可以包含多个原始消息，但只占一个会话展示行。过程记录默认收起，最终总结显示；展开过程不改变原始消息编号及评论锚点。生成中的思考可流式查看，完成后收起。

读文件显示路径记录，不展开正文。编辑工具优先使用 OMP 返回的历史 before/after 快照生成 Diff；没有快照时使用工具提供的 diff，绝不以当前文件冒充当时版本。其他工具的代码块只呈现 command/code/script 和输出结果，不展示 timeout 等协议参数。

v0.5 当时通过 OMP `prompt.streamingBehavior=steer` 追加运行中的指令。停止使用 `abort_and_restore_queue`，取回未执行输入并在历史中标记；会话仅在原生 `sessionSettled` 为真或收到 `session_settled` 后结束运行状态。API 请求失败或连接断开时，未确认的输入恢复为草稿。

后续版本已将普通追加改为 `followUp` 排队；用户点击“指示”后才提升为 steering。这一行为以当前实现和[队列使用说明](usage.md#输入排队与评论)为准。

## 当时的验证

- 单元覆盖会话分组、原消息定位、列表滚动锚点、命令参数过滤、多文件历史 Diff。
- 真实 OMP / DeepSeek 集成测试验证追加指令被采纳，以及停止确认与会话结束。
- Windows 窗口验证第 9 轮展开后仍停在该轮、读文件不可展开、命令/输出代码块、编辑 Diff、运行图标、停止与追加输入。
- 继续沿用原生主题的焦点反馈与可访问名称。没有宣称完整屏幕阅读器认证；不对 Windows 桌面套用手机尺寸验证。

## 当前维护入口

主题语义在 [desktop/theme.rs](../src/desktop/theme.rs)，会话展示与选区交互在 [workspace/view](../src/desktop/workspace/view/mod.rs)，消息分组和滚动协调在 [conversation/transcript_layout.rs](../src/desktop/workspace/conversation/transcript_layout.rs)。目录及依赖边界见[组件架构](architecture.md)。

后续已补充独立文件／变更标签、当前标签查找、自动刷新、整行变更高亮、正文与行号同色，以及原生子智能体和角色配置表单；相关当前行为与验证记录分别维护在 [usage.md](usage.md) 和 [testing.md](testing.md)。
