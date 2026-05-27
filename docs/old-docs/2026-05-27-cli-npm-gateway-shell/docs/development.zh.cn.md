# 开发

## Commands

在 `flyflor-cli` workspace 中运行检查：

```bash
cargo check
cargo test
cargo run
```

连接到非默认 kernel 端口时显式设置：

```bash
FLYFLOR_WS_URL=ws://127.0.0.1:8788/ws cargo run
```

编译后的 binary name 是 `flyflor`。

## Environment

- `FLYFLOR_WS_URL`：覆盖默认 `ws://127.0.0.1:8787/ws`。
- `FLYFLOR_HISTORY=0|false|FALSE|off|OFF`：禁用 socket/history usage。
- `FLYFLOR_CONTEXT_WINDOW`：当 kernel 没有提供最大窗口时，用作 context-window display 的本地 fallback。
- `FLYFLOR_LANG`：选择内置 JSON i18n catalog，例如 `zh-CN`、`zh`、`en-US` 或 `en`。
- `FLYFLOR_I18N_DIR`：从用户提供的目录加载 `<lang>.json` catalog。
- `FLYFLOR_I18N_FILE`：加载用户提供的单个 JSON catalog 文件。

## Debugging

使用 `tmux` 或分开的终端：

1. 在 `flyflor` 仓库运行 Bun kernel socket server。
2. 使用指向 kernel 的 `FLYFLOR_WS_URL` 运行 CLI。
3. 查看 CLI logs 和 TUI Run timeline，确认 socket、event、ASK、tool、process、worker 和 subagent visibility。

CLI 应作为 thin client 调试。如果某个行为需要 kernel state，应检查 kernel `/ws` contract 和 read-model snapshots，而不是给 CLI 增加本地权威。

## Documentation Rule

活跃文档保持双语。英文 doc 变化时，必须在同一次变更中更新对应 `.zh.cn.md`。如果现有 doc 需要语义重写，先归档到 `docs/old-docs/`，再重建 active path。
