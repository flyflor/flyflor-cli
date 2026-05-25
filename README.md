# flyflor-cli

Rust TUI prototype for the Flyflor chat client.

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

## Current scope

- Static mock UI based on the design draft
- Full-history chat viewport with keyboard scrolling
- Right-side status panel and bottom composer shell

## Controls

- `/exit`: quit
- `Ctrl+C` / `Ctrl+V` / `Cmd+V`: paste from clipboard in the composer
- `F2` or `Ctrl+D`: toggle dev mode
- `Up` / `k`: scroll up
- `Down` / `j`: scroll down
- `PageUp`: scroll one page up
- `PageDown` or `Space`: scroll one page down
- `g`: jump to top
- `G`: jump to bottom

## Dev mode

- Shows layout rectangles, viewport size, and scroll state in an overlay
- Keeps product UI intact and layers debug information on top
- Intended for spacing, sizing, and scroll-behavior iteration while rebuilding the design
