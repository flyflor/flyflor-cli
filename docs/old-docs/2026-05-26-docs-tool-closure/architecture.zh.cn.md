# 架构

## 职责边界

`flyflor-cli` 是 Flyflor 系统的 Rust TUI 可视化/交互壳。它不是 kernel，不是权威状态机，也不是 ledger 所有者。

CLI 负责：

- 使用 `ratatui` 和 `crossterm` 绘制终端 UI。
- 接收键盘、粘贴、鼠标、选择、复制和 slash command 输入。
- 通过 WebSocket envelope 把用户意图发送给 Flyflor kernel。
- 渲染 kernel 返回的快照和事件。
- 维护本地展示状态，例如滚动位置、菜单、右侧面板焦点、pending turn 占位和复制选择。

Flyflor kernel 负责：

- 对话权威状态和持久历史。
- task planning 状态和计划决策。
- ASK continuation 语义。
- context fork 创建和 fork session 数据。
- blackboard、recall、memory、audit、query、replay 和 ledger 数据。
- 模型/provider 状态与 context-window 遥测。

CLI 应把 kernel 数据视为 source of truth。本地状态只服务于展示连续性和交互体验。

## 与 Kernel 的分层

CLI 和 kernel 的边界是 `flyflor.ws.v1` envelope 协议。CLI 发送 `gateway.message.send`、`history.list`、`task.list`、`gateway.status.get`、`fork.memory.get`、`event.subscribe`、`task.plan.decide` 和 `fork.create` 等命令。

Kernel 响应以快照或事件形式到达。CLI 将它们解析为本地 `SocketEvent` 变体，并把这些事件应用到内存中的 `App` 状态。kernel 不可用时，CLI 可以保持 mock/offline 展示，但不能虚构权威历史、计划、fork 或 ledger 状态。

## 当前 `src/main.rs` 聚合结构

当前 binary 入口是 `src/main.rs`，由 `Cargo.toml` 配置为 `flyflor` binary。它目前有意承载较多职责，多个关注点聚合在一个文件中：

- 终端生命周期：raw mode、alternate screen、mouse capture、bracketed paste、panic 日志与清理。
- 事件循环：socket drain、clipboard drain、draw pass、cursor update、键盘事件、粘贴事件和鼠标事件。
- App 状态：transcript turns、滚动状态、右侧面板数据、fork session、pending turns、菜单、status snapshots 和 interaction mode。
- 渲染：header、body layout、左侧 transcript、右侧面板、composer、command/ASK/plan 菜单、类 Markdown answer 渲染和 mermaid ASCII 渲染。
- 协议接线：socket worker、command channel、gateway command builders 和 envelope 解析器。
- 数据整形：history snapshots、context rows、task/todo rows、status snapshots、fork memory rows、ASK options 和 plan state。
- 测试：parser、envelope、render、selection、right-panel 和状态行为。

这种聚合让当前行为容易在一个文件里审查，但也让无关改动变得更危险，因为终端、协议、渲染和状态逻辑彼此相邻。

## 约定目录

当前拆分把 feature ownership 放在显式目录中：

- `src/tui/terminal`：raw mode、alternate screen、mouse capture、clipboard fallback 和 panic/log setup。
- `src/tui/gateway`：`flyflor.ws.v1` envelope factory、固定 subscription list、启动 bootstrap 和 command builders。
- `src/tui/ask`：ASK menu state、parser、view helpers 和 continuation answer metadata。
- `src/tui/plan`：plan menu state 与 `task.plan.decide` payload。
- `src/tui/fork`：active fork state、fork command payload 和 labels。
- `src/tui/run_timeline`：event timeline parsing/state/view。
- `src/tui/subagent`：subagent batch/child parsing/state/view。

`src/main.rs` 仍负责 app loop、顶层状态流转、snapshot parsers 和绘制 glue。新能力应该先进入上述约定目录，再 composition 到 `App`；不要新增并行的根级 runtime 或 WebSocket 实现。

## CLI 非目标

CLI 不应成为：

- kernel 的 prompt 容器。
- 权威计划执行器。
- 持久 memory ledger。
- kernel query、replay、audit 或 detail API 的替代品。
- kernel 规则的第二套实现。

尤其要注意，`brain.db` 是 kernel 侧用于 ledger/query/replay/audit/detail 工作流的存储。CLI 可以展示来自 `fork.memory` 数据的 `brain.db` 可用性或大小标签，但不得把 `brain.db` 当成本地 prompt 容器或写入目标。
