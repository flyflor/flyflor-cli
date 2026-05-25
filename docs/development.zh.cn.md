# 开发

## 工作区检查

修改本工作区前，先确认位置和分支：

```bash
pwd
git branch --show-current
git status --short --branch
```

对于 tmux-managed CLI worktree，预期分支是 `codex/tmux-managed-flyflor-cli`。

## 运行命令

运行 TUI：

```bash
cargo run
```

以 dev mode 运行：

```bash
cargo run -- --dev
```

或：

```bash
FLYFLOR_DEV=1 cargo run
```

连接非默认 kernel socket：

```bash
FLYFLOR_WS_URL=ws://127.0.0.1:8787/ws cargo run
```

禁用 socket/history 集成并使用 mock/offline history：

```bash
FLYFLOR_HISTORY=0 cargo run
```

## Dev Mode

Dev mode 可以在启动时通过 `--dev` 或 `FLYFLOR_DEV=1` 开启。在 TUI 内，`F2` 或 `Ctrl+D` 可以切换内部 dev flag。

旧的 diagnostics 悬浮窗已禁用，避免遮挡 TUI。诊断信息请使用 `.flyflor-cli/logs/dev.log` 和 focused tests。

## 热重载

使用 npm scripts 前先安装依赖：

```bash
npm install
```

默认 dev script 使用 `cargo watch`：

```bash
npm run dev
```

也可以使用 Node-based runner：

```bash
npm run dev:node
```

两者都会以 dev mode 运行 TUI，并把诊断写入 `.flyflor-cli/logs/dev.log`。

## 日志

查看开发日志：

```bash
npm run logs
```

日志文件是：

```text
.flyflor-cli/logs/dev.log
```

Rust TUI 会写入 startup、exit、panic、socket connect、envelope send、parser side effects、copy failures 和 socket disconnect 等记录。Node dev runner 也会写入 restart 和 child-process lifecycle 记录。

## tmux 查看

为了方便 tmux 旁观，命令应显式且低噪声：

```bash
pwd
git branch --show-current
git status --short --branch
npm run logs
```

当 TUI 在一个 pane 中运行时，用另一个 pane 观察日志：

```bash
tail -n 200 -f .flyflor-cli/logs/dev.log
```

如果 crash 后终端停留在 alternate-screen 或 mouse-capture 状态，可以重启 dev runner，或先在 shell 中运行 terminal reset，再继续手工测试。

## 验证

变更后运行 Rust type/build check：

```bash
cargo check
```

更完整的检查：

```bash
npm run check
cargo test
```

对于纯文档改动，确认 diff 只包含 Markdown：

```bash
git diff --name-only
git diff --stat
```

文档工作不应修改 Rust source、script 或 configuration 文件。
