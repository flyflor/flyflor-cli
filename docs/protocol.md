# WebSocket Protocol

## Transport

The CLI connects to `FLYFLOR_WS_URL` or, by default, `ws://127.0.0.1:8787/ws`. The socket worker runs on a background thread and communicates with the UI thread through `SocketCommand` and `SocketEvent` channels.

The kernel docs currently use `ws://127.0.0.1:8788/ws` for local smoke examples. Until code defaults are unified, set `FLYFLOR_WS_URL` explicitly when connecting the CLI to a kernel running on another port.

When `FLYFLOR_HISTORY=0`, `false`, `FALSE`, `off`, or `OFF`, the socket worker is disabled and the UI stays in mock/offline history mode. Demo mode also disables history/socket usage.

## Envelope Shape

Outgoing messages use `protocol: "flyflor.ws.v1"` and include:

- `id`: envelope id.
- `type`: command type.
- `at`: RFC3339 timestamp.
- `requestId`: request id for command correlation.
- `payload`: command-specific body.

Incoming messages are parsed by `type` and `payload`. Unknown message types are ignored unless they are parsed as `error`.

## Startup Sequence

After the socket connects, the current CLI sends:

- `client.hello`: identifies `flyflor-cli`, package version, and `ratatui` capability.
- `history.list`: requests recent history, optionally scoped later by `contextForkId`.
- `task.list`: requests task/todo state.
- `capability.catalog.get`: requests the visible kernel capability/tool surface.
- `gateway.status.get`: requests model/provider/context-window status.
- `fork.memory.get`: requests recent fork memory and `brain.db` display fields.
- `event.subscribe`: subscribes only to known-safe runtime events used by the CLI.

The current CLI marks the socket as connected after the transport connection and `client.hello` send path succeed. A future `server.hello` parser may be added, but `server.hello` should remain handshake metadata, not authoritative business state.

The kernel exposes `capability.catalog.get` and `capability.catalog.snapshot`; CLI startup now requests the catalog. Normal non-YOLO per-turn approval is exposed through `/approve`, which marks only the next `gateway.message.send` with kernel-shaped `context.toolApprovals`.

## Subscriptions

The current `event.subscribe` payload requests a fixed, source-controlled list of stable runtime events. The list lives in `src/tui/gateway/subscription.rs` and covers plan, ASK, route/recall, blackboard, tool, Executive loop, subagent, process, and worker lifecycle events.

It deliberately does not subscribe to nonexistent or provisional event names, such as `fork.memory.*`; fork memory refreshes continue through `fork.memory.get` after final turns and explicit commands.

## Outgoing Commands

The UI can send these socket commands:

- `gateway.message.send`: normal user message or explicit ASK continuation answer.
- `gateway.message.undo`: rollback command for a selected user-message anchor.
- `gateway.message.interrupt`: interrupt an active turn by public message id.
- `history.list`: history refresh, optionally scoped by active context fork.
- `task.list`: todo/task refresh.
- `gateway.status.get`: status refresh.
- `fork.memory.get`: recent fork memory refresh.
- `task.plan.decide`: plan confirm, revise, or abandon.
- `fork.create`: create a context fork from a structured turn anchor.
- `execution.job.detail.get`: fetch execution job detail snapshots for display.

`gateway.message.send` includes conversation, thread, user identity, optional `context.contextForkId`, optional continuation metadata, optional one-turn `context.toolApprovals`, and TUI mode metadata: `act`, `plan`, or `act` with `yolo: true`.

## Routing And Context Boundary

Channel identity is routing and audit metadata only. It maps to `conversationKey`, `threadId`, `chatType`, `user`, and gateway-specific metadata. The CLI must not move channel identity into `payload.context` or treat it as prompt continuity.

`gateway.message.send.payload.context` is the explicit context boundary. Valid context fields are `activeScope`, `contextForkId`, `skillNames`, and `toolApprovals`. The current CLI sends `contextForkId` for explicit fork work and `toolApprovals` for one-turn approval; future `activeScope` or `skillNames` payloads must come from explicit user/kernel state, not from channel identity or history data.

`history.list` and read-model snapshots are query/display data. They may narrow the UI by `contextForkId`, but they must not be fed back into `gateway.message.send` as prompt context.

`/approve` submits `context.toolApprovals.mcpToolCalls=true` and `context.toolApprovals.userToolCalls=true` for the next send only. YOLO also submits these approvals, but carries separate high-privilege metadata. The CLI must not execute approved tools locally.

Pending ASK state must not hijack ordinary composer input. Normal typed text remains a normal `gateway.message.send` without continuation metadata unless the user explicitly confirms an ASK menu action.

ASK fixed-option confirmation and ASK `Other` confirmation also use `gateway.message.send`; only those explicit paths attach the latest continuation metadata so the kernel resumes the original ASK/task context.

Citizen permission choices, including `continue-tools`, `keep-budget`, and `keep-subagents`, are represented as structured metadata in the outgoing payload. They must not be sent as plain user-message text.

`/undo` sends `gateway.message.undo` with the selected anchor. The kernel records undo audit and abandons affected hot memory / ASK / continuation state without deleting `brain.db`; the CLI only updates presentation state after sending the command.

`execution.job.detail.get` requests are display fetches only. The CLI dedupes or throttles them by job id so timeline rendering does not create socket noise.

## Localization

TUI copy is loaded from JSON catalogs. The bundled defaults live in `i18n/zh-CN.json` and `i18n/en-US.json`. Set `FLYFLOR_LANG=en` to select English, `FLYFLOR_I18N_DIR=/path/to/catalogs` to load `<lang>.json`, or `FLYFLOR_I18N_FILE=/path/to/custom.json` to override the catalog directly.

## Snapshot Parsing

The CLI maps kernel snapshots into local state:

- `history.snapshot` becomes sorted transcript `Turn` values.
- `task.snapshot`, `task.list.result`, `task.list.snapshot`, `task.list`, or compatible task data become right-panel TODO rows.
- `gateway.status.snapshot`, `gateway.status`, or `status.snapshot` become `StatusSnapshot` model/provider/context data.
- `fork.memory.snapshot`, `memory.fork.snapshot`, `fork.memory`, `fork.memory.result`, or `fork.list.snapshot` become `ForkMemorySnapshot`.
- `thought.snapshot`, `recall.snapshot`, `memory.snapshot`, `blackboard.snapshot`, and `ask.snapshot` become synthetic context turns for display.
- `fork.snapshot` creates or enters an active fork view when a fork id is present.

Snapshot parsing is deliberately tolerant about payload shape because the CLI is a compatibility display layer. Tolerance does not make the CLI authoritative; kernel state still wins.

## Event Parsing

The CLI parses turn and subscription events:

- `turn.delta`: appends streamed answer text to the pending turn.
- `turn.final`: replaces the pending answer, stores metadata, updates context rows, discovers latest context fork id, and requests `fork.memory.get`.
- `turn.error`: marks the pending turn or right-panel status as failed.
- `event.publish`, `event.snapshot`, or `event`: unwraps subscription events.
- `memory.task_plan.written` and `memory.task_plan.decided`: mark plan data as updated and request `task.list`.
- `executive.loop.paused` and `executive.loop.resumed`: update ASK/run-loop process visibility.
- `blackboard.*`, `tool.*`, `mcp.tool.call.executed`, `route.escalated`, `scope.recall.*`, `memory.context_fork.written`, `process.*`, `worker.task.*`, and `subagent.*`: become run-timeline rows. Job ids can trigger `execution.job.detail.get` for a richer `execution.job.snapshot`.
- `error`: becomes `SocketEvent::Disconnected`.

Socket read errors and close frames are logged and cause the worker to retry after a short delay.

## Context Window Authority

`gateway.status.snapshot.model.contextWindowTokens` is authoritative when present. The kernel resolves it from explicit config, provider model metadata, and known fallback data. The CLI may estimate current usage locally, but it must not replace a kernel-provided maximum. `FLYFLOR_CONTEXT_WINDOW` is only a display fallback when the kernel omits the maximum.

## Kernel Authority

Protocol messages are commands and observations. They do not transfer ownership of kernel responsibilities to the CLI. The CLI should never write directly to kernel ledger storage, call kernel-private APIs, or treat local UI state as durable state.

`brain.db` is kernel-side storage for ledger/query/replay/audit/detail. The CLI only displays `brain.db` summary fields carried by fork memory responses, such as human-readable size or availability.
