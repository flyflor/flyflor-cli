# flyflor-cli

Rust TUI for the Flyflor chat client. The CLI is a thin interactive shell over
the Flyflor WebSocket gateway: it sends user intent through `flyflor.ws.v1`
commands and renders kernel snapshots/events without owning kernel state.

## Run

```bash
cargo run
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
- Plan confirmation/revision/abandon commands
- Context fork creation and active fork session display
- Run timeline for route, recall, blackboard, tool, ASK, plan, fork, loop, and
  subagent events
- Right-side TODO, Run, model/status, context-window, and fork-memory sections

## Controls

- `/exit`: quit
- `/help`: show command help
- `/status`: refresh gateway status
- `/history`: refresh history
- `/todo`: refresh TODO or answer pending plan confirmation
- `/ask`: open the latest pending ASK menu
- `/fork`: create a context fork from the latest structured assistant turn
- `/blackboard`: surface the latest blackboard summary
- `/memory`: refresh fork memory
- `Ctrl+C` / `Ctrl+V` / `Cmd+V`: paste from clipboard in the composer
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
