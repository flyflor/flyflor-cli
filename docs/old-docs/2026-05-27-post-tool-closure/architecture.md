# Architecture

## Responsibility Boundary

`flyflor-cli` is a Rust TUI shell for visualizing and interacting with the Flyflor kernel. It is not the kernel, not the authoritative state machine, not the tool executor, and not the ledger owner.

The CLI is responsible for:

- Drawing the terminal UI with `ratatui` and `crossterm`.
- Accepting keyboard, paste, mouse, selection, copy, and slash-command input.
- Sending user intent to the kernel through `flyflor.ws.v1` WebSocket envelopes.
- Rendering snapshots and events returned by the kernel.
- Sending structured user decisions for ASK, plan decisions, fork creation, and one-turn tool approval.
- Keeping local presentation state such as scroll offsets, menus, focused right-panel section, pending turn placeholders, and copy selections.

The Flyflor kernel is responsible for:

- Conversation authority and durable history.
- Prompt layering and model calls.
- Memory, Crystal, Scope, codename, ContextFork, and ASK semantics.
- Task planning state and plan decisions.
- Blackboard, recall, audit, query, replay, and ledger data.
- Capability catalog, tool execution, sandbox, approval, quota, and audit.
- Model/provider status and context-window telemetry.

The CLI should treat kernel data as source-of-truth input. Local state is for display continuity and interaction ergonomics only.

## Kernel Layering Seen By CLI

The CLI does not implement Flyflor philosophy layers, but it should render them coherently:

- Route decisions appear in the Run timeline.
- Blackboard appears as transcript context rows, snapshots, and `blackboard.*` events.
- ASK appears as continuation rows and menus.
- Hot memory and fork memory appear as context-window and fork/memory panels.
- `brain.db` appears only as kernel-provided labels or history/fork snapshots.
- Executive tools appear as capability snapshots, tool events, execution jobs, loop pauses, and explicit one-turn approval state.

## Current Source Layout

The current binary entrypoint is `src/main.rs`, configured by `Cargo.toml` as the `flyflor` binary. It still owns app loop, high-level state transitions, snapshot parsers, and drawing glue.

Feature ownership is split into convention directories:

- `src/tui/terminal`: raw mode, alternate screen, mouse capture, clipboard fallback, and panic/log setup.
- `src/tui/gateway`: `flyflor.ws.v1` envelope factory, fixed subscription list, startup bootstrap, and command builders.
- `src/tui/ask`: ASK menu state, parser, view helpers, and continuation answer metadata.
- `src/tui/plan`: plan menu state and `task.plan.decide` payloads.
- `src/tui/fork`: active fork state, fork command payloads, and labels.
- `src/tui/run_timeline`: event timeline parsing/state/view.
- `src/tui/subagent`: subagent batch/child parsing/state/view.

New features should enter these convention directories and compose into `App`; do not add parallel root-level runtime or WebSocket implementations.

## Non-Goals

The CLI must not become:

- A prompt container for the kernel.
- The authoritative plan executor.
- The durable memory ledger.
- A replacement for kernel query, replay, audit, or detail APIs.
- A second implementation of kernel tool rules.
- A local writer for `brain.db`.

`brain.db` is kernel-side storage for ledger/query/replay/audit/detail workflows. The CLI may display a `brain.db` availability or size label from `fork.memory` data, but it must not treat `brain.db` as a local prompt container or write target.
