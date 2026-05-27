# flyflor-cli

Flyflor 的 Rust TUI。CLI 是 Flyflor WebSocket gateway 之上的薄交互 shell：它通过 `flyflor.ws.v1` 命令发送用户意图，渲染 kernel snapshot/event，但不拥有 kernel state。

英文入口：[README.md](README.md)。

## 运行

```bash
cargo run
```

全局 npm 安装：

```bash
npm i -g flyflor-cli
flyflor -h
flyflor gateway -h
flyflor
```

npm 包安装 `flyflor` bin wrapper；若存在 `dist/<platform>-<arch>/flyflor`，优先使用内置平台二进制。缺失时，`postinstall` 会用随包 Rust 源码执行 `cargo build --release --bin flyflor` 作为 fallback。

包安装 smoke：

```bash
npm run smoke:npm:local
FLYFLOR_NPM_SMOKE_HELP=1 npm run smoke:npm:local
```

交叉编译平台二进制：

```bash
npm run build:binary -- --target x86_64-unknown-linux-gnu
npm run build:binary -- --target aarch64-apple-darwin
npm run build:binary:all
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
- pending ASK 不会劫持普通 composer 输入；只有显式 ASK menu 操作才发送 continuation metadata。
- 公民权限 answer 发送结构化 metadata，不写成普通消息 token。
- Plan confirmation/revision/abandon command。
- Context fork creation 和 active fork display。
- Route、recall、blackboard、tool、ASK、plan、fork、loop、subagent event 的 Run timeline。
- Exo timeline rows 不展示 `unknown`，最后一个 Exo 自动展开，并对 detail 请求去重。
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
