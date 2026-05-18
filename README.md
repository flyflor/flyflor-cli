# flyflor-cli

Mock Flyflor TUI built on OpenTUI. This is a first-pass prototype for validating the hard part: a large, cell-aware virtual chat viewport with fixed side panels and terminal input chrome.

## Run

```bash
bun install
bun run dev
```

Keys:

- `j` / `Down`: scroll down
- `k` / `Up`: scroll up
- `PageDown` / `Space`: page down
- `PageUp`: page up
- `Home`: top
- `End` / `Ctrl+E`: bottom
- Mouse wheel over the left chat pane scrolls the virtual viewport
- Drag the left scrollbar thumb to jump through the virtual list
- Type a message and press `Enter` to append a mock user/assistant turn
- `Esc` or `Ctrl+C`: quit

## Verify

```bash
bun run check
bun run snapshot
```

The snapshot script uses OpenTUI's official test renderer and mock input/mouse helpers to prove message submission, bottom anchoring, mouse wheel scrolling, and scrollbar dragging move the visible window.

## Implementation Notes

The left chat area is a custom `VirtualChatRenderable`, not a plain `ScrollBox` full of thousands of child renderables. It measures each block at the current terminal cell width, caches wrapped rows by block, stores block heights in a Fenwick tree, and renders only the visible rows plus a small overscan region.
