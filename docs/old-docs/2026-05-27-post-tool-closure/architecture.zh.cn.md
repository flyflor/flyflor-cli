# 架构

## 职责边界

`flyflor-cli` 是用于可视化和交互 Flyflor kernel 的 Rust TUI shell。它不是 kernel，不是权威 state machine，不是 tool executor，也不是 ledger owner。

CLI 负责：

- 用 `ratatui` 和 `crossterm` 绘制 terminal UI。
- 接收 keyboard、paste、mouse、selection、copy 和 slash-command input。
- 通过 `flyflor.ws.v1` WebSocket envelopes 向 kernel 发送用户意图。
- 渲染 kernel 返回的 snapshots 和 events。
- 为 ASK、plan decision、fork creation 和单轮 tool approval 发送结构化用户决策。
- 保存 scroll offsets、menus、focused right-panel section、pending turn placeholders 和 copy selections 等本地展示状态。

Flyflor kernel 负责：

- Conversation authority 和 durable history。
- Prompt layering 和 model calls。
- Memory、Crystal、Scope、codename、ContextFork 和 ASK 语义。
- Task planning state 和 plan decisions。
- Blackboard、recall、audit、query、replay 和 ledger data。
- Capability catalog、tool execution、sandbox、approval、quota 和 audit。
- Model/provider status 和 context-window telemetry。

CLI 应把 kernel data 当作 source-of-truth input。本地状态只服务展示连续性和交互手感。

## CLI 看到的 Kernel 分层

CLI 不实现 Flyflor 哲学层，但应该一致地渲染它们：

- Route decisions 显示在 Run timeline。
- Blackboard 显示为 transcript context rows、snapshots 和 `blackboard.*` events。
- ASK 显示为 continuation rows 和 menus。
- Hot memory 与 fork memory 显示为 context-window 和 fork/memory panels。
- `brain.db` 只显示为 kernel 提供的 labels 或 history/fork snapshots。
- Executive tools 显示为 capability snapshots、tool events、execution jobs、loop pauses 和显式单轮 approval state。

## 当前源码布局

当前 binary entrypoint 是 `src/main.rs`，由 `Cargo.toml` 配置为 `flyflor` binary。它仍然拥有 app loop、高层 state transitions、snapshot parsers 和 drawing glue。

Feature ownership 按 convention directories 拆分：

- `src/tui/terminal`：raw mode、alternate screen、mouse capture、clipboard fallback 和 panic/log setup。
- `src/tui/gateway`：`flyflor.ws.v1` envelope factory、fixed subscription list、startup bootstrap 和 command builders。
- `src/tui/ask`：ASK menu state、parser、view helpers 和 continuation answer metadata。
- `src/tui/plan`：plan menu state 与 `task.plan.decide` payload。
- `src/tui/fork`：active fork state、fork command payload 和 labels。
- `src/tui/run_timeline`：event timeline parsing/state/view。
- `src/tui/subagent`：subagent batch/child parsing/state/view。

新功能应该进入这些 convention directories 并组合进 `App`；不要增加并行 root-level runtime 或 WebSocket implementations。

## 非目标

CLI 不得成为：

- Kernel 的 prompt container。
- 权威 plan executor。
- Durable memory ledger。
- Kernel query、replay、audit 或 detail APIs 的替代品。
- Kernel tool rules 的第二套实现。
- `brain.db` 的本地写入者。

`brain.db` 是 kernel 侧用于 ledger/query/replay/audit/detail workflows 的存储。CLI 可以展示来自 `fork.memory` 数据的 `brain.db` 可用性或大小标签，但不得把 `brain.db` 当成本地 prompt container 或 write target。
