# flyflor-cli

Flyflor 的 Rust TUI。CLI 是 Flyflor WebSocket gateway 之上的薄交互 shell：它通过 `flyflor.ws.v1` 命令发送用户意图，渲染 kernel snapshot/event，但不拥有 kernel state。

英文入口：[README.md](README.md)。

## 运行

```bash
cargo run
```

开发模式：

```bash
cargo run -- --dev
```

或：

```bash
FLYFLOR_DEV=1 cargo run
```

热重载开发模式：

```bash
cargo install cargo-watch
npm install
npm run dev
```

默认 dev script 使用 Rust `cargo-watch`，当 Rust 文件或 Cargo 配置变化时重启 `cargo run -- --dev`。

日志：

```bash
npm run logs
```

Dev runner 和 Rust TUI 都会把诊断追加到 `.flyflor-cli/logs/dev.log`。

## 当前范围

- WebSocket bootstrap 和固定 runtime event subscription。
- Streaming transcript、history snapshot 和 fork-scoped history refresh。
- ASK menu、固定选项、`Other` 自由输入 continuation reply。
- 普通 typed ASK answer 会复用最新 pending continuation metadata。
- Plan confirmation/revision/abandon command。
- Context fork creation 和 active fork session display。
- Route、recall、blackboard、tool、ASK、plan、fork、loop、subagent event 的 Run timeline。
- 右侧 TODO、Run、model/status、context-window 和 fork-memory section。
- `i18n/zh-CN.json` 与 `i18n/en-US.json` 外挂 JSON 文案目录。

## 控制

- `/exit`：退出。
- `/help`：显示命令帮助。
- `/approve`：只批准下一次发送的 kernel MCP/user tool call。
- `/undo`：打开回滚菜单并发送 `gateway.message.undo`。
- `/yolo`：切换高权限 metadata。
- `/status`：刷新 gateway status。
- `/history`：刷新 history。
- `/model`：显示 provider/model/context-window status。
- `/todo`：刷新 TODO，或回答 pending plan confirmation。
- `/ask`：打开最新 pending ASK menu。
- `/fork`：从最近结构化 assistant turn 创建 context fork。
- `/blackboard`：展示最新 blackboard summary。
- `/memory`：刷新 fork memory。
- `Ctrl+C` / `Ctrl+V` / `Cmd+V`：在 composer 粘贴。
- `Shift+Enter`：在 composer 换行。
- `Esc` 后再次 `Esc`：中断 active kernel turn。
- `F2` 或 `Ctrl+D`：切换 dev mode。
- `Up` / `k`：向上滚动。
- `Down` / `j`：向下滚动。
- `PageUp`：向上滚动一页。
- `PageDown` 或 `Space`：向下滚动一页。
- `g`：跳到顶部。
- `G`：跳到底部。

## Dev mode

- 通过 `--dev`、`FLYFLOR_DEV=1`、`F2` 或 `Ctrl+D` 打开内部 dev flag。
- 诊断写入 `.flyflor-cli/logs/dev.log`。
- 不再渲染旧 floating diagnostics overlay。
