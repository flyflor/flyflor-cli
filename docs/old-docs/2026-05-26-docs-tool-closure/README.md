# flyflor-cli Documentation

This directory is the synchronized English and Chinese documentation set for the
`flyflor-cli` Rust TUI workspace.

## Document Map

- [Architecture](architecture.md): CLI responsibility boundaries, the current
  `src/main.rs` aggregate, and the target split.
- [Protocol](protocol.md): WebSocket envelope wiring, startup messages,
  snapshots, subscriptions, and event parsing.
- [TUI Model](tui-model.md): ASK, plan, fork, blackboard, status, right-panel,
  hot memory, and fork memory display behavior.
- [Development](development.md): run commands, `cargo check`, dev mode, logs,
  and tmux-friendly inspection.

## Chinese Edition

The Chinese document set mirrors this structure:

- [文档总览](README.zh.cn.md)
- [架构](architecture.zh.cn.md)
- [协议](protocol.zh.cn.md)
- [TUI 模型](tui-model.zh.cn.md)
- [开发](development.zh.cn.md)

## Synchronization Rule

Every English `.md` document in this set has a Chinese `.zh.cn.md` counterpart.
Section order and technical claims should stay aligned. When protocol or UI
behavior changes, update both files in the same change.
