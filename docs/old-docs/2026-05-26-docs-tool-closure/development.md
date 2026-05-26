# Development

## Workspace Checks

Before changing this workspace, confirm location and branch:

```bash
pwd
git branch --show-current
git status --short --branch
```

For the current tmux-managed integration worktree, the expected branch is
`feat/cli-ws-event-tui-integration`.

## Run Commands

Run the TUI:

```bash
cargo run
```

Run with dev mode enabled:

```bash
cargo run -- --dev
```

or:

```bash
FLYFLOR_DEV=1 cargo run
```

Connect to a non-default kernel socket:

```bash
FLYFLOR_WS_URL=ws://127.0.0.1:8787/ws cargo run
```

Disable socket/history integration and use mock/offline history:

```bash
FLYFLOR_HISTORY=0 cargo run
```

## Dev Mode

Dev mode can be enabled at startup with `--dev` or `FLYFLOR_DEV=1`. Inside the
TUI, `F2` or `Ctrl+D` toggles the internal dev flag.

The previous floating diagnostics overlay is disabled so it does not obscure the
TUI. Use `.flyflor-cli/logs/dev.log` and focused tests for diagnostics.

## Hot Reload

Install dependencies before using the npm scripts:

```bash
npm install
```

The default dev script uses `cargo watch`:

```bash
npm run dev
```

The Node-based runner is also available:

```bash
npm run dev:node
```

Both run the TUI in dev mode and write diagnostics to
`.flyflor-cli/logs/dev.log`.

## Logs

Tail development logs:

```bash
npm run logs
```

The log file is:

```text
.flyflor-cli/logs/dev.log
```

The Rust TUI writes entries such as startup, exit, panic, socket connect,
envelope send, parser side effects, copy failures, and socket disconnect. The
Node dev runner also writes restart and child-process lifecycle entries.

## tmux Inspection

For tmux observers, keep commands explicit and low-noise:

```bash
pwd
git branch --show-current
git status --short --branch
npm run logs
```

When the TUI is running in one pane, use another pane to watch logs:

```bash
tail -n 200 -f .flyflor-cli/logs/dev.log
```

If the terminal is left in an alternate-screen or mouse-capture state after a
crash, restart the dev runner or run a terminal reset from the shell before
continuing manual testing.

## Verification

Run the Rust type/build check after changes:

```bash
cargo check
```

Useful broader checks:

```bash
npm run check
cargo test
```

For documentation-only changes, confirm the diff is Markdown-only:

```bash
git diff --name-only
git diff --stat
```

No Rust source, script, or configuration file should change for documentation
work.
