# WebSocket 协议

## 传输

CLI 连接 `FLYFLOR_WS_URL`，默认值是 `ws://127.0.0.1:8787/ws`。socket worker 在后台线程运行，并通过 `SocketCommand` 和 `SocketEvent` channel 与 UI 线程通信。

WebSocket gateway 和 event stream 是 CLI 与 kernel 之间的血管层边界：
命令与观察只通过 `flyflor.ws.v1` envelope 和 subscription event 流动。CLI
不调用 kernel 私有 API，也不伸入 kernel runtime 内部实现。

当 `FLYFLOR_HISTORY=0`、`false`、`FALSE`、`off` 或 `OFF` 时，socket worker 被禁用，UI 保持 mock/offline history 模式。Demo mode 也会禁用 history/socket 使用。

## Envelope 形状

出站消息使用 `protocol: "flyflor.ws.v1"`，并包含：

- `id`：envelope id。
- `type`：命令类型。
- `at`：RFC3339 时间戳。
- `requestId`：用于命令关联的 request id。
- `payload`：命令专属 body。

入站消息按 `type` 和 `payload` 解析。未知 message type 会被忽略，除非它被解析为 `error`。

## 启动序列

socket 连接后，CLI 会发送以下启动序列：

- `client.hello`：标识 `flyflor-cli`、package version 和 `ratatui` capability。
- `history.list`：请求最近历史，后续可按 `contextForkId` 限定范围。
- `task.list`：请求 task/todo 状态。
- `gateway.status.get`：请求模型/provider/context-window 状态。
- `fork.memory.get`：请求最近 fork memory 和 `brain.db` 展示字段。
- `event.subscribe`：只订阅 CLI 使用的已知安全 runtime event。

当前 CLI 在 transport connection 和 `client.hello` 发送路径成功后，就把 socket 标记为 connected。未来可以加入 `server.hello` 解析器，但 `server.hello` 应继续作为 handshake metadata，而不是权威业务状态。

## 订阅

当前 `event.subscribe` payload 请求一份固定在源码中的稳定 runtime event
列表。列表位于 `src/tui/gateway/subscription.rs`，覆盖 plan、ASK、
route/recall、blackboard、tool、Executive loop 和 subagent lifecycle
事件。CLI 有意不订阅不存在或临时的 event 名称，例如 `fork.memory.*`；
Fork memory 仍在 final turn 后或显式命令中通过 `fork.memory.get` 刷新。

## 出站命令

UI 可以发送这些 socket 命令：

- `gateway.message.send`：普通用户消息或 ASK continuation 回答。
- `history.list`：历史刷新，可按 active context fork 限定范围。
- `task.list`：todo/task 刷新。
- `gateway.status.get`：status 刷新。
- `fork.memory.get`：最近 fork memory 刷新。
- `task.plan.decide`：确认、补充或放弃计划。
- `fork.create`：基于结构化 turn anchor 创建 context fork。
- `execution.job.detail.get`：请求 execution job detail snapshot 用于展示。
- `ask.detail.get`：请求 ASK detail snapshot 用于展示。
- `blackboard.detail.get`：请求 blackboard detail snapshot 用于展示。

`gateway.message.send` 包含 conversation、thread、user identity、可选 `context.contextForkId`、可选 continuation metadata，以及 TUI mode metadata：`act`、`plan` 或带 `yolo: true` 的 `act`。

## 快照解析

CLI 将 kernel 快照映射为本地状态：

- `history.snapshot` 变成排序后的 transcript `Turn`。
- `task.snapshot`、`task.list.result`、`task.list.snapshot`、`task.list` 或兼容 task 数据变成右侧面板 TODO 行。
- `gateway.status.snapshot`、`gateway.status` 或 `status.snapshot` 变成 `StatusSnapshot` model/provider/context 数据。
- `fork.memory.snapshot`、`memory.fork.snapshot`、`fork.memory`、`fork.memory.result` 或 `fork.list.snapshot` 变成 `ForkMemorySnapshot`。
- `thought.snapshot`、`recall.snapshot`、`memory.snapshot`、`blackboard.snapshot` 和 `ask.snapshot` 变成用于展示的 synthetic context turn。
- `fork.snapshot` 在存在 fork id 时创建或进入 fork session。

快照解析刻意对 payload shape 保持宽容，因为 CLI 是兼容展示层。宽容并不代表 CLI 拥有权威；kernel 状态仍然优先。

## 事件解析

CLI 解析 turn 和 subscription 事件：

- `turn.delta`：把流式 answer 文本追加到 pending turn。
- `turn.final`：替换 pending answer，保存 metadata，更新 context rows，发现最新 context fork id，并请求 `fork.memory.get`。
- `turn.error`：把 pending turn 或右侧面板状态标记为失败。
- `event.publish`、`event.snapshot` 或 `event`：解包 subscription event。
- `memory.task_plan.written` 与 `memory.task_plan.decided`：标记 plan 数据已更新并请求 `task.list`。
- `executive.loop.paused` 与 `executive.loop.resumed`：更新 ASK/run-loop 过程可见性。
- `blackboard.*`、`tool.*`、`mcp.tool.call.executed`、`route.escalated`、`scope.recall.*` 和 `subagent.*`：变成 run-timeline 行，或触发 detail snapshot 拉取。
- `error`：转换为 `SocketEvent::Disconnected`。

Socket read error 和 close frame 会写入日志，并让 worker 短暂等待后重试。

## Kernel 权威

协议消息是命令和观察，不会把 kernel 职责转移给 CLI。CLI 不应直接写 kernel ledger storage，也不应把本地 UI 状态当成持久状态。
CLI 也不调用 kernel 私有 API。

`brain.db` 是 kernel 侧用于 ledger/query/replay/audit/detail 的存储。CLI 只展示 fork memory 响应携带的 `brain.db` 摘要字段，例如人类可读大小或可用性。
