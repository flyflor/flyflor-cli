# flyflor-cli Documentation

This directory is the synchronized English and Chinese documentation set for the `flyflor-cli` Rust TUI workspace.

`flyflor-cli` is a thin external shell for the Flyflor Bun kernel. It renders `/ws` snapshots and events, sends user intent and user decisions, and keeps local presentation state. It is not the kernel, ledger, tool executor, or prompt owner.

## Document Map

- [Architecture](architecture.md): CLI responsibility boundaries, current source layout, and the kernel/CLI ownership split.
- [Protocol](protocol.md): WebSocket envelope wiring, startup messages, snapshots, subscriptions, event parsing, and current closure state.
- [TUI Model](tui-model.md): ASK, plan, fork, blackboard, Run timeline, status, right-panel, hot memory, fork memory, and tool visibility.
- [Development](development.md): run commands, `cargo check`, dev mode, logs, and tmux-friendly inspection.

## Chinese Edition

The Chinese document set mirrors this structure:

- [文档总览](README.zh.cn.md)
- [架构](architecture.zh.cn.md)
- [协议](protocol.zh.cn.md)
- [TUI 模型](tui-model.zh.cn.md)
- [开发](development.zh.cn.md)

## Current Alignment Notes

- The kernel socket docs use `ws://127.0.0.1:8788/ws` for local smoke examples; the CLI default remains `ws://127.0.0.1:8787/ws` unless `FLYFLOR_WS_URL` is set.
- The kernel exposes `server.hello` and `capability.catalog.get`; CLI startup requests the visible capability catalog.
- The kernel context input supports `toolApprovals.mcpToolCalls` and `toolApprovals.userToolCalls`; the CLI exposes `/approve` for one-turn non-YOLO approval, plus YOLO mode and tool/run visibility.
- ASK typed-answer continuation is closed for plain composer replies.
- `/undo` sends `gateway.message.undo`; rollback authority and memory abandonment remain kernel-side.
- Context-window maximums are rendered from kernel snapshots when present, with local `FLYFLOR_CONTEXT_WINDOW` only as a display fallback.

## Synchronization Rule

Every English `.md` document in this set has a Chinese `.zh.cn.md` counterpart. Section order and technical claims should stay aligned. When protocol or UI behavior changes, update both files in the same change.

Superseded docs are archived under `docs/old-docs/` before rewriting active paths.
