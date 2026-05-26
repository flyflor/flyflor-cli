# WebSocket Protocol

## Transport

The CLI connects to `FLYFLOR_WS_URL` or, by default,
`ws://127.0.0.1:8787/ws`. The socket worker runs on a background thread and
communicates with the UI thread through `SocketCommand` and `SocketEvent`
channels.

The WebSocket gateway and event stream are the vascular boundary between the
CLI and kernel: commands and observations flow through `flyflor.ws.v1`
envelopes and subscription events. The CLI does not call kernel-private APIs or
reach into kernel runtime internals.

When `FLYFLOR_HISTORY=0`, `false`, `FALSE`, `off`, or `OFF`, the socket worker is
disabled and the UI stays in mock/offline history mode. Demo mode also disables
history/socket usage.

## Envelope Shape

Outgoing messages use `protocol: "flyflor.ws.v1"` and include:

- `id`: envelope id.
- `type`: command type.
- `at`: RFC3339 timestamp.
- `requestId`: request id for command correlation.
- `payload`: command-specific body.

Incoming messages are parsed by `type` and `payload`. Unknown message types are
ignored unless they are parsed as `error`.

## Startup Sequence

After the socket connects, the CLI sends this startup sequence:

- `client.hello`: identifies `flyflor-cli`, package version, and `ratatui`
  capability.
- `history.list`: requests recent history, optionally scoped later by
  `contextForkId`.
- `task.list`: requests task/todo state.
- `gateway.status.get`: requests model/provider/context-window status.
- `fork.memory.get`: requests recent fork memory and `brain.db` display fields.
- `event.subscribe`: subscribes only to known-safe runtime events used by the
  CLI.

The current CLI marks the socket as connected after the transport connection and
`client.hello` send path succeed. A future `server.hello` parser may be added,
but `server.hello` should remain handshake metadata, not authoritative business
state.

## Subscriptions

The current `event.subscribe` payload requests a fixed, source-controlled list
of stable runtime events. The list lives in `src/tui/gateway/subscription.rs`
and covers plan, ASK, route/recall, blackboard, tool, Executive loop, and
subagent lifecycle events. It deliberately does not subscribe to nonexistent or
provisional event names, such as `fork.memory.*`; fork memory refreshes continue
through `fork.memory.get` after final turns and explicit commands.

## Outgoing Commands

The UI can send these socket commands:

- `gateway.message.send`: normal user message or ASK continuation answer.
- `history.list`: history refresh, optionally scoped by active context fork.
- `task.list`: todo/task refresh.
- `gateway.status.get`: status refresh.
- `fork.memory.get`: recent fork memory refresh.
- `task.plan.decide`: plan confirm, revise, or abandon.
- `fork.create`: create a context fork from a structured turn anchor.
- `execution.job.detail.get`: fetch execution job detail snapshots for display.
- `ask.detail.get`: fetch ASK detail snapshots for display.
- `blackboard.detail.get`: fetch blackboard detail snapshots for display.

`gateway.message.send` includes conversation, thread, user identity, optional
`context.contextForkId`, optional continuation metadata, and TUI mode metadata:
`act`, `plan`, or `act` with `yolo: true`.

## Snapshot Parsing

The CLI maps kernel snapshots into local state:

- `history.snapshot` becomes sorted transcript `Turn` values.
- `task.snapshot`, `task.list.result`, `task.list.snapshot`, `task.list`, or
  compatible task data become right-panel TODO rows.
- `gateway.status.snapshot`, `gateway.status`, or `status.snapshot` become
  `StatusSnapshot` model/provider/context data.
- `fork.memory.snapshot`, `memory.fork.snapshot`, `fork.memory`,
  `fork.memory.result`, or `fork.list.snapshot` become `ForkMemorySnapshot`.
- `thought.snapshot`, `recall.snapshot`, `memory.snapshot`,
  `blackboard.snapshot`, and `ask.snapshot` become synthetic context turns for
  display.
- `fork.snapshot` creates or enters a fork session when a fork id is present.

Snapshot parsing is deliberately tolerant about payload shape because the CLI is
a compatibility display layer. Tolerance does not make the CLI authoritative;
kernel state still wins.

## Event Parsing

The CLI parses turn and subscription events:

- `turn.delta`: appends streamed answer text to the pending turn.
- `turn.final`: replaces the pending answer, stores metadata, updates context
  rows, discovers latest context fork id, and requests `fork.memory.get`.
- `turn.error`: marks the pending turn or right-panel status as failed.
- `event.publish`, `event.snapshot`, or `event`: unwraps subscription events.
- `memory.task_plan.written` and `memory.task_plan.decided`: mark plan data as
  updated and request `task.list`.
- `executive.loop.paused` and `executive.loop.resumed`: update ASK/run-loop
  process visibility.
- `blackboard.*`, `tool.*`, `mcp.tool.call.executed`, `route.escalated`,
  `scope.recall.*`, and `subagent.*`: become run-timeline rows or trigger
  detail snapshot fetches.
- `error`: becomes `SocketEvent::Disconnected`.

Socket read errors and close frames are logged and cause the worker to retry
after a short delay.

## Kernel Authority

Protocol messages are commands and observations. They do not transfer ownership
of kernel responsibilities to the CLI. The CLI should never write directly to
kernel ledger storage, call kernel-private APIs, or treat local UI state as
durable state.

`brain.db` is kernel-side storage for ledger/query/replay/audit/detail. The CLI
only displays `brain.db` summary fields carried by fork memory responses, such
as human-readable size or availability.
