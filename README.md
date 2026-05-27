# flyflor-cli

Rust TUI for the Flyflor chat client. The CLI is a thin interactive shell over
the Flyflor WebSocket gateway: it sends user intent through `flyflor.ws.v1`
commands and renders kernel snapshots/events without owning kernel state.

Chinese companion: [README.zh.cn.md](README.zh.cn.md).

## Run

```bash
cargo run
```

Global npm install:

```bash
npm i -g flyflor-cli
flyflor -h
flyflor gateway -h
flyflor
```

The npm package installs a `flyflor` bin wrapper and uses the bundled
`dist/<platform>-<arch>/flyflor` binary when present. If that platform binary is
missing, `postinstall` builds it from the included Rust sources with
`cargo build --release --bin flyflor`.

Packaging smoke:

```bash
npm run smoke:npm:local
FLYFLOR_NPM_SMOKE_HELP=1 npm run smoke:npm:local
```

Cross-build package binaries:

```bash
npm run build:binary -- --target x86_64-unknown-linux-gnu
npm run build:binary -- --target aarch64-apple-darwin
npm run build:binary:all
```

Dev mode:

```bash
cargo run -- --dev
```

or

```bash
FLYFLOR_DEV=1 cargo run
```

Hot-reload dev mode:

```bash
cargo install cargo-watch
npm install
npm run dev
```

The default dev script uses Rust's `cargo-watch` to restart `cargo run -- --dev` when Rust files or Cargo config change.

Logs:

```bash
npm run logs
```

The dev runner and Rust TUI both append diagnostics to `.flyflor-cli/logs/dev.log`.

## Current Scope

- WebSocket bootstrap with fixed runtime event subscription
- Streaming transcript, history snapshots, and fork-scoped history refresh
- ASK menu with fixed choices and `Other` free input continuation replies
- Pending ASK does not capture ordinary composer input; only explicit ASK menu actions send continuation metadata
- Citizen permission answers send structured metadata, not plain message tokens
- Plan confirmation/revision/abandon commands
- Context fork creation and active fork display
- Run timeline for route, recall, blackboard, tool, ASK, plan, fork, loop, and
  subagent events
- Exo timeline rows avoid `unknown`, auto-expand the latest Exo, and dedupe detail requests
- Right-side TODO, Run, model/status, context-window, and fork-memory sections
- JSON-backed i18n catalogs in `i18n/zh-CN.json` and `i18n/en-US.json`

## Controls

- `/exit`: quit
- `/help`: show command help
- `/approve`: approve kernel MCP/user tool calls for the next send only
- `/undo`: open rollback menu and send `gateway.message.undo`
- `/yolo`: toggle high-privilege mode metadata
- `/status`: refresh gateway status
- `/history`: refresh history
- `/model`: show provider/model/context-window status
- `/todo`: refresh TODO or answer pending plan confirmation
- `/ask`: open the latest pending ASK menu
- `/fork`: create a context fork from the latest structured assistant turn
- `/blackboard`: surface the latest blackboard summary
- `/memory`: refresh fork memory
- `Ctrl+C` / `Ctrl+V` / `Cmd+V`: paste from clipboard in the composer
- `Shift+Enter`: insert a newline in the composer
- `Esc`, then `Esc` again: interrupt the active kernel turn
- `F2` or `Ctrl+D`: toggle dev mode
- `Up` / `k`: scroll up
- `Down` / `j`: scroll down
- `PageUp`: scroll one page up
- `PageDown` or `Space`: scroll one page down
- `g`: jump to top
- `G`: jump to bottom

## Dev mode

- Enables the internal dev flag with `--dev`, `FLYFLOR_DEV=1`, `F2`, or `Ctrl+D`
- Keeps diagnostics in `.flyflor-cli/logs/dev.log`
- Does not render the old floating diagnostics overlay
