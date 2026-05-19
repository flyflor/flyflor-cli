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

## Current scope

- Static mock UI based on the design draft
- Full-history chat viewport with keyboard scrolling
- Right-side status panel and bottom composer shell

## Controls

- `q` or `Ctrl+C`: quit
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
