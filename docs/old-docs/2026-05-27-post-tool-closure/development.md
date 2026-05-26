# Development

## Commands

Run checks from the `flyflor-cli` workspace:

```bash
cargo check
cargo test
cargo run
```

Connect to a non-default kernel port explicitly:

```bash
FLYFLOR_WS_URL=ws://127.0.0.1:8788/ws cargo run
```

The compiled binary name is `flyflor`.

## Environment

- `FLYFLOR_WS_URL`: overrides the default `ws://127.0.0.1:8787/ws`.
- `FLYFLOR_HISTORY=0|false|FALSE|off|OFF`: disables socket/history usage.
- `FLYFLOR_CONTEXT_WINDOW`: optional local fallback for context-window display when the kernel does not provide a maximum.

## Debugging

Use `tmux` or separate terminals:

1. Run the Bun kernel socket server in the `flyflor` repository.
2. Run the CLI with `FLYFLOR_WS_URL` pointing to the kernel.
3. Watch CLI logs and the TUI Run timeline for socket, event, ASK, tool, process, worker, and subagent visibility.

The CLI should be debugged as a thin client. If behavior requires kernel state, inspect the kernel `/ws` contract and read-model snapshots rather than adding local authority to the CLI.

## Documentation Rule

Active docs are bilingual. If an English doc changes, update the matching `.zh.cn.md` doc in the same change. If an existing doc needs semantic rewriting, archive it under `docs/old-docs/` first and then recreate the active path.
