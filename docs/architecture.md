# Architecture

## Responsibility Boundary

`flyflor-cli` is a Rust TUI shell for visualizing and interacting with the
Flyflor system. It is not the kernel, not the authoritative state machine, and
not the ledger owner.

The CLI is responsible for:

- Drawing the terminal UI with `ratatui` and `crossterm`.
- Accepting keyboard, paste, mouse, selection, copy, and slash-command input.
- Sending user intent to the Flyflor kernel through WebSocket envelopes.
- Rendering snapshots and events returned by the kernel.
- Keeping local presentation state such as scroll offsets, menus, focused
  right-panel section, pending turn placeholders, and copy selections.

The Flyflor kernel is responsible for:

- Conversation authority and durable history.
- Task planning state and plan decisions.
- ASK continuation semantics.
- Context fork creation and fork session data.
- Blackboard, recall, memory, audit, query, replay, and ledger data.
- Model/provider status and context-window telemetry.

The CLI should treat kernel data as source-of-truth input. Local state is for
display continuity and interaction ergonomics only.

## Layering With the Kernel

The boundary between the CLI and the kernel is the `flyflor.ws.v1` envelope
protocol. The CLI sends commands such as `gateway.message.send`, `history.list`,
`task.list`, `gateway.status.get`, `fork.memory.get`, `event.subscribe`,
`task.plan.decide`, and `fork.create`.

Kernel responses arrive as snapshots or events. The CLI parses them into local
`SocketEvent` variants and applies those variants to the in-memory `App` state.
When the kernel is unavailable, the CLI can keep a mock/offline display, but it
must not invent authoritative history, plan, fork, or ledger state.

## Current `src/main.rs` Aggregate

The current binary entrypoint is `src/main.rs`, configured by `Cargo.toml` as
the `flyflor` binary. It is intentionally broad today and contains several
concerns in one file:

- Terminal lifecycle: raw mode, alternate screen, mouse capture, bracketed
  paste, panic logging, and cleanup.
- Event loop: socket drain, clipboard drain, draw pass, cursor update, keyboard
  events, paste events, and mouse events.
- App state: transcript turns, scroll state, right-panel data, fork session,
  pending turns, menus, status snapshots, and interaction mode.
- Rendering: header, body layout, left transcript, right panel, composer,
  command/ASK/plan menus, Markdown-ish answer rendering, and
  mermaid ASCII rendering.
- Protocol wiring: socket worker, command channel, envelope constructors, and
  envelope parsers.
- Data shaping: history snapshots, context rows, task/todo rows, status
  snapshots, fork memory rows, ASK options, and plan state.
- Tests: parser, envelope, render, selection, right-panel, and state behavior.

This aggregation makes the current behavior easy to inspect in one place, but
it also makes unrelated edits risky because terminal, protocol, render, and
state logic are adjacent.

## Future Split Target

Future refactors should keep behavior stable while separating ownership:

- `terminal`: raw mode, alternate screen, mouse capture, bracketed paste,
  clipboard fallback, and panic/log setup.
- `app`: `App`, `SocketCommand`, `SocketEvent`, interaction state, key/mouse
  dispatch, and top-level state transitions.
- `protocol`: `flyflor.ws.v1` envelope constructors and parsers.
- `render`: layout, transcript rendering, right panel, composer, menus,
  Markdown rendering, and scrollbars.
- `domain`: turn metadata, context rows, ASK options, plan state, todo shaping,
  status snapshots, and fork memory shaping.
- `fixtures` or `mock`: demo/offline data.

The split goal is not to change protocol or UI behavior. It is to make future
changes safer by moving kernel-facing protocol code away from visual layout code.

## Non-Goals for the CLI

The CLI should not become:

- A prompt container for the kernel.
- The authoritative plan executor.
- The durable memory ledger.
- A replacement for kernel query, replay, audit, or detail APIs.
- A second implementation of kernel rules.

In particular, `brain.db` is kernel-side storage for ledger/query/replay/audit
and detail workflows. The CLI may display a `brain.db` availability or size
label from `fork.memory` data, but it must not treat `brain.db` as a local prompt
container or write target.
