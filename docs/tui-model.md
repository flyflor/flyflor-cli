# TUI Model

## Display Contract

The TUI displays kernel state and local interaction state together. Kernel state
is authoritative; local interaction state exists so the UI can scroll, focus,
copy, open menus, and show pending requests smoothly.

The main screen has:

- A header with socket/history status, session label, and turn count.
- A left transcript with user prompts, assistant answers, context rows, ASK
  continuation rows, blackboard/replay rows, recall/thought rows, and create
  fork affordances.
- A right panel with TODO, model/status, context window, and fork/memory.
- A composer footer with interaction mode, active fork label, and local command
  hints.

## ASK Display

ASK data is read from turn metadata or ASK snapshots. If the metadata contains a
continuation (`snapshotId` or `continuationId`), the transcript gets an
`AskResume` context row. Opening it shows an ASK menu.

The ASK menu contains kernel-provided options plus `Other` free input. Choosing
a fixed option sends a `gateway.message.send` with continuation metadata.
Choosing `Other` stores the continuation locally until the next composer submit,
then sends the typed text with the same continuation metadata.

The CLI does not decide the ASK semantics. It only presents choices and returns
the selected answer to the kernel.

## Plan Display

Plan and TODO data can come from task snapshots or turn metadata. The right-panel
TODO section derives a `PlanState`:

- Empty: no task rows or only the placeholder plan.
- Generating: status text indicates generation.
- Awaiting confirmation: status text contains confirmation wording.
- Running: at least one active/current row exists.
- Abandoned: status text indicates abandonment.

When the plan awaits confirmation, `/todo` opens the plan menu. The menu exposes
confirm, revise, and abandon. Confirm and abandon send `task.plan.decide`
immediately. Revise changes the composer into a pending plan-revision input; the
next submit sends `task.plan.decide` with `revision`.

The CLI does not execute the plan. It only displays plan state and sends the
user's decision.

## Fork Display

Fork data appears in three places:

- Transcript context rows aggregate `planning.contextForks` metadata and expose
  fork summaries.
- The create-fork row or `/fork` command sends `fork.create` from the latest
  structured assistant turn anchor.
- The header and composer footer show the active fork session label when the CLI
  enters a fork.

When a `fork.snapshot` creates an active fork session, the CLI stores the root
turns, clears the transcript for the fork conversation, requests fork-scoped
history, and carries `context.contextForkId` on subsequent messages.

`/exit fork` leaves the fork session and restores root or parent state when
available. The kernel remains the authority for actual fork data.

## Blackboard Display

Blackboard data can arrive through metadata, snapshots, or subscription events:

- Turn metadata may create blackboard context rows in the left transcript.
- `blackboard.snapshot` becomes a synthetic display turn.
- The parser still understands compatible blackboard runtime events if the
  kernel emits them, but the CLI does not currently subscribe to provisional
  blackboard event type names.
- `/blackboard` copies the latest blackboard summary into the right-panel status
  line.

The right-side lower panel no longer renders a separate blackboard stream as an
independent section. Tests protect the current right-panel section set: TODO,
Model / Status, Context Window, and Fork / Memory.

## Status Model

The visible status model combines:

- `HistoryStatus`: loading, connected/live, mock/offline, or error.
- `InteractionMode`: ACT, PLAN, or YOLO.
- Pending work: pending turns, streaming footers, and pending fork creation.
- Model/provider fields from `StatusSnapshot`.
- Context-window telemetry from `StatusSnapshot`, local fallback estimation, and
  optional environment fallback.
- Clipboard and socket error messages surfaced through the right-panel status.

Status labels are display hints. They should not be used as durable state or
protocol authority.

## Right Panel

The right panel has a sticky layout:

- TODO occupies the flexible top area and owns the right scroll state.
- A separator divides TODO from the fixed lower status area.
- The lower area displays Model / Status, Context Window, and Fork / Memory.

Left/right arrows move focus across copyable right-panel sections. Pressing `y`
copies the active selection when one exists; otherwise it copies the focused
right-panel section. The TODO list is intentionally separate from the lower
scroll/copy model.

## Hot Memory and Fork Memory

The Context Window section shows hot context usage. It prefers kernel-provided
context telemetry, falls back to local estimation from recent turns and active
fork id, and can use `FLYFLOR_CONTEXT_WINDOW` when the kernel does not provide a
maximum.

The Fork / Memory section shows up to five recent fork summaries and a
`brain.db` label from `fork.memory` data. The `brain.db` label is display-only.
`brain.db` is kernel-side ledger/query/replay/audit/detail storage, not a CLI
prompt container.
