# TUI Model

## Display Contract

The TUI displays kernel state and local interaction state together. Kernel state is authoritative; local interaction state exists so the UI can scroll, focus, copy, open menus, and show pending requests smoothly.

The main screen has:

- A header with socket/history status, connection/fork label, and turn count.
- A left transcript with user prompts, assistant answers, context rows, ASK continuation rows, blackboard/replay rows, recall/thought rows, and create fork affordances.
- A right panel with TODO, Run timeline, model/status, context window, and fork/memory.
- A composer footer with interaction mode, active fork label, and local command hints.

The composer sends on `Enter` and inserts a newline on `Shift+Enter`.

## ASK Display

ASK data is read from turn metadata or ASK snapshots. If the metadata contains a continuation (`snapshotId` or `continuationId`), the transcript gets an `AskResume` context row. Opening it shows an ASK menu.

The ASK menu contains kernel-provided options plus `Other` free input. Choosing a fixed option sends a `gateway.message.send` with continuation metadata. Choosing `Other` arms one explicit continuation reply; the next composer submit sends the typed text with the same continuation metadata.

A pending ASK does not make the ordinary composer a default ASK answer. If the user types without explicitly choosing an ASK menu option or `Other`, the CLI sends normal message text with no continuation metadata.

The CLI does not decide ASK semantics. It only presents choices and returns the selected answer to the kernel.

## Plan Display

Plan and TODO data can come from task snapshots or turn metadata. The right-panel TODO section derives a `PlanState`:

- Empty: no task rows or only the placeholder plan.
- Generating: status text indicates generation.
- Awaiting confirmation: status text contains confirmation wording.
- Running: at least one active/current row exists.
- Abandoned: status text indicates abandonment.

When the plan awaits confirmation, `/todo` opens the plan menu. The menu exposes confirm, revise, and abandon. Confirm and abandon send `task.plan.decide` immediately. Revise changes the composer into a pending plan-revision input; the next submit sends `task.plan.decide` with `revision`.

The CLI does not execute the plan. It only displays plan state and sends the user's decision.

## Fork Display

Fork data appears in three places:

- Transcript context rows aggregate `planning.contextForks` metadata and expose fork summaries.
- The create-fork row or `/fork` command sends `fork.create` from the latest structured assistant turn anchor.
- The header and composer footer show the active fork label when the CLI enters a fork.

When a `fork.snapshot` enters an active fork view, the CLI stores the root turns, clears the transcript for the fork conversation, requests fork-scoped history, and carries `context.contextForkId` on subsequent messages.

`/exit fork` leaves the active fork view and restores root or parent display state when available. The kernel remains the authority for actual fork data.

## Blackboard Display

Blackboard data can arrive through metadata, snapshots, or subscription events:

- Turn metadata may create blackboard context rows in the left transcript.
- `blackboard.snapshot` becomes a synthetic display turn.
- The fixed gateway subscription includes stable `blackboard.*` runtime events. Those events are shown in Run so the process is visible instead of hidden behind snapshots.
- `/blackboard` copies the latest blackboard summary into the right-panel status line.

The CLI does not schedule workers, decide route convergence, or write Blackboard state.

## Run Timeline

Run is the visible execution spine for gateway events. It consumes `event.publish`, `event.snapshot`, `event`, and `execution.job.snapshot` data and renders route decisions, scope recall, blackboard turns, tool calls, ASK pauses, Confirm answers, plan writes, forks, Executive loop transitions, process events, worker events, and subagent lifecycle updates.

Job detail fetches are deduped or throttled by job id. Timeline display should not repeatedly request the same `execution.job.detail.get` payload while rendering.

Subagent events are merged into a batch/child tree. Loose child events are attached to their batch when a later snapshot or batch event arrives, and repeated snapshots update existing rows instead of duplicating them. This keeps subagents visible without making the CLI responsible for runtime scheduling.

## Tool Visibility And Approval Closure

Tool events and execution-job snapshots make the Executive exoskeleton visible. The CLI can show tool start/progress/success/failure, MCP tool execution, loop pause/resume, and budget exhaustion.

Normal per-turn approval for kernel `toolApprovals.mcpToolCalls` and `toolApprovals.userToolCalls` is closed through `/approve`. It marks only the next send and then clears. YOLO mode remains a separate local high-privilege interaction marker sent as metadata.

Citizen permission options such as `continue-tools`, `keep-budget`, and `keep-subagents` are rendered as Confirm authorization policy choices and sent as structured `confirmAnswer` metadata, with `askAnswer` retained only for kernel compatibility. Subscribed `confirm.answered` events appear in Run as Confirm rows and can close pending user-needed markers, but they must not be converted into ordinary user-message text or displayed with the ASK crystallization style. Normal ASK continuations still use the ASK menu and continuation metadata.

Startup `confirm.snapshot` data is also displayed as Confirm rows in Run so recent authorization decisions survive CLI reconnects. It remains read-model/display state only and never creates ASK continuation rows or Crystal candidate UI.

While a turn is active, the footer shows an animated Working line. Pressing Esc once arms interruption; pressing Esc again within the interrupt window sends `gateway.message.interrupt` for the pending public message id.

The Exo tool/subprocess section uses parsed tool names and lifecycle summaries rather than `unknown` placeholders. Waiting for permission, running, completed, and failed states must all have explicit labels. The latest Exo row auto-expands; older Exo rows stay collapsed by default. Expanded rows show recent output snippets without making the CLI execute tools locally.

`/undo` opens a menu of user-message rollback anchors. Confirming a row sends `gateway.message.undo`; kernel-side memory abandonment and ledger audit are authoritative.

## Status Model

The visible status model combines:

- `HistoryStatus`: loading, connected/live, mock/offline, or error.
- `InteractionMode`: ACT, PLAN, or YOLO.
- Pending work: pending turns, streaming footers, and pending fork creation.
- Model/provider fields from `StatusSnapshot`.
- Context-window telemetry from `StatusSnapshot`, local fallback estimation, and optional environment fallback.
- Clipboard and socket error messages surfaced through the right-panel status.

Status labels are display hints. They should not be used as durable state or protocol authority.

## Right Panel

The right panel has a sticky layout:

- TODO occupies the flexible top area and owns the right scroll state.
- A separator divides TODO from the fixed lower status area.
- The lower area displays Run, Model / Status, Context Window, and Fork / Memory.

Left/right arrows move focus across copyable right-panel sections. Pressing `y` copies the active selection when one exists; otherwise it copies the focused right-panel section. The TODO list is intentionally separate from the lower scroll/copy model.

## Hot Memory and Fork Memory

The Context Window section shows hot context usage. It prefers kernel-provided context telemetry, falls back to local estimation from recent turns and active fork id, and can use `FLYFLOR_CONTEXT_WINDOW` when the kernel does not provide a maximum. A kernel-provided `contextWindowTokens` value is authoritative and can represent large provider windows such as 1M-token models.

The Fork / Memory section shows up to five recent fork summaries and a `brain.db` label from `fork.memory` data. The `brain.db` label is display-only. `brain.db` is kernel-side ledger/query/replay/audit/detail storage, not a CLI prompt container.
