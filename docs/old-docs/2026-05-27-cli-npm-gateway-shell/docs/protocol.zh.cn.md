# WebSocket Protocol

## Transport

CLI 连接 `FLYFLOR_WS_URL`，默认值是 `ws://127.0.0.1:8787/ws`。Socket worker 在后台线程运行，并通过 `SocketCommand` 和 `SocketEvent` channel 与 UI 线程通信。

Kernel 文档当前用 `ws://127.0.0.1:8788/ws` 作为本地 smoke 示例。在代码默认值统一前，如果 kernel 运行在其他端口，请显式设置 `FLYFLOR_WS_URL`。

当 `FLYFLOR_HISTORY=0`、`false`、`FALSE`、`off` 或 `OFF` 时，socket worker 会禁用，UI 保持 mock/offline history mode。Demo mode 也会禁用 history/socket usage。

## Envelope Shape

Outgoing messages 使用 `protocol: "flyflor.ws.v1"`，并包含：

- `id`：envelope id。
- `type`：command type。
- `at`：RFC3339 timestamp。
- `requestId`：用于 command correlation 的 request id。
- `payload`：command-specific body。

Incoming messages 按 `type` 和 `payload` 解析。Unknown message types 会被忽略，除非被解析为 `error`。

## Startup Sequence

Socket 连接后，当前 CLI 发送：

- `client.hello`：标识 `flyflor-cli`、package version 和 `ratatui` capability。
- `history.list`：请求最近 history，后续可按 `contextForkId` 限定。
- `task.list`：请求 task/todo state。
- `capability.catalog.get`：请求 kernel 当前可见 capability/tool surface。
- `gateway.status.get`：请求 model/provider/context-window status。
- `fork.memory.get`：请求最近 fork memory 和 `brain.db` display fields。
- `event.subscribe`：只订阅 CLI 使用的已知安全 runtime events。

当前 CLI 在 transport connection 和 `client.hello` 发送路径成功后，就把 socket 标记为 connected。未来可以加入 `server.hello` parser，但 `server.hello` 应继续作为 handshake metadata，而不是权威业务状态。

kernel 暴露 `capability.catalog.get` 和 `capability.catalog.snapshot`；CLI startup 现在会请求 catalog。非 YOLO 流程下的普通 per-turn approval 通过 `/approve` 暴露，只给下一次 `gateway.message.send` 携带 kernel-shaped `context.toolApprovals`。

## Subscriptions

当前 `event.subscribe` payload 请求一份固定在源码中的 stable runtime events。列表位于 `src/tui/gateway/subscription.rs`，覆盖 plan、ASK、route/recall、blackboard、tool、Executive loop、subagent、process 和 worker lifecycle events。

它有意不订阅不存在或临时的 event 名称，例如 `fork.memory.*`；fork memory 仍在 final turn 后或显式命令中通过 `fork.memory.get` 刷新。

## Outgoing Commands

UI 可以发送这些 socket commands：

- `gateway.message.send`：普通用户消息或 ASK continuation answer。
- `gateway.message.undo`：按选中的用户消息 anchor 发送 rollback command。
- `gateway.message.interrupt`：按 public message id 终断 active turn。
- `history.list`：history refresh，可按 active context fork 限定范围。
- `task.list`：todo/task refresh。
- `gateway.status.get`：status refresh。
- `fork.memory.get`：recent fork memory refresh。
- `task.plan.decide`：确认、补充或放弃计划。
- `fork.create`：基于结构化 turn anchor 创建 context fork。
- `execution.job.detail.get`：请求 execution job detail snapshots 用于展示。

`gateway.message.send` 包含 conversation、thread、user identity、可选 `context.contextForkId`、可选 continuation metadata、可选单轮 `context.toolApprovals`，以及 TUI mode metadata：`act`、`plan` 或带 `yolo: true` 的 `act`。

`/approve` 只为下一次发送提交 `context.toolApprovals.mcpToolCalls=true` 和 `context.toolApprovals.userToolCalls=true`。YOLO 也会提交这些 approvals，但它额外携带高权限 metadata。CLI 不得本地执行已批准工具。

对 pending ASK 的普通 composer answer 也使用 `gateway.message.send`；CLI 会附带最新 continuation metadata，让 kernel 恢复原始 ASK/task context。

`/undo` 发送带所选 anchor 的 `gateway.message.undo`。Kernel 会记录 undo audit，并把受影响的热记忆、ASK、continuation state 标记为 abandoned，不删除 `brain.db`；CLI 只在发送命令后更新展示状态。

## Localization

TUI 文案从 JSON catalog 读取。默认文件位于 `i18n/zh-CN.json` 和 `i18n/en-US.json`。设置 `FLYFLOR_LANG=en` 可切换英文，设置 `FLYFLOR_I18N_DIR=/path/to/catalogs` 可加载 `<lang>.json`，或设置 `FLYFLOR_I18N_FILE=/path/to/custom.json` 直接覆盖 catalog。

## Snapshot Parsing

CLI 将 kernel snapshots 映射到本地状态：

- `history.snapshot` 变成排序后的 transcript `Turn`。
- `task.snapshot`、`task.list.result`、`task.list.snapshot`、`task.list` 或兼容 task data 变成右侧面板 TODO rows。
- `gateway.status.snapshot`、`gateway.status` 或 `status.snapshot` 变成 `StatusSnapshot` model/provider/context data。
- `fork.memory.snapshot`、`memory.fork.snapshot`、`fork.memory`、`fork.memory.result` 或 `fork.list.snapshot` 变成 `ForkMemorySnapshot`。
- `thought.snapshot`、`recall.snapshot`、`memory.snapshot`、`blackboard.snapshot` 和 `ask.snapshot` 变成用于展示的 synthetic context turns。
- `fork.snapshot` 在存在 fork id 时创建或进入 fork session。

Snapshot parsing 对 payload shape 保持兼容宽容，因为 CLI 是 compatibility display layer。宽容不代表 CLI 权威；kernel state 仍然胜出。

## Event Parsing

CLI 解析 turn 和 subscription events：

- `turn.delta`：把 streamed answer text 追加到 pending turn。
- `turn.final`：替换 pending answer，保存 metadata，更新 context rows，发现 latest context fork id，并请求 `fork.memory.get`。
- `turn.error`：把 pending turn 或右侧 status 标记为 failed。
- `event.publish`、`event.snapshot` 或 `event`：unwrap subscription events。
- `memory.task_plan.written` 与 `memory.task_plan.decided`：标记 plan data updated 并请求 `task.list`。
- `executive.loop.paused` 与 `executive.loop.resumed`：更新 ASK/run-loop process visibility。
- `blackboard.*`、`tool.*`、`mcp.tool.call.executed`、`route.escalated`、`scope.recall.*`、`memory.context_fork.written`、`process.*`、`worker.task.*` 和 `subagent.*`：变成 run-timeline rows。带 job id 的事件可触发 `execution.job.detail.get`，获得更丰富的 `execution.job.snapshot`。
- `error`：变成 `SocketEvent::Disconnected`。

Socket read errors 和 close frames 会记录日志，并在短延迟后重试。

## Context Window Authority

`gateway.status.snapshot.model.contextWindowTokens` 存在时是权威值。Kernel 会按显式配置、provider model metadata 和已知 fallback 解析它。CLI 可以本地估算当前 usage，但不能替换 kernel 提供的最大值。`FLYFLOR_CONTEXT_WINDOW` 只在 kernel 没有提供最大值时作为 display fallback。

## Kernel Authority

Protocol messages 是 commands 和 observations。它们不会把 kernel responsibilities 的 ownership 转移给 CLI。CLI 不应直接写 kernel ledger storage、调用 kernel-private APIs，或把本地 UI state 当作 durable state。

`brain.db` 是 kernel 侧用于 ledger/query/replay/audit/detail 的存储。CLI 只展示 fork memory responses 携带的 `brain.db` summary fields，例如 human-readable size 或 availability。
