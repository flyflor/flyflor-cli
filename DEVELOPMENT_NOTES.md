# Flyflor CLI Development Notes

This document records the current TUI architecture, interaction rules, and
the implementation details that are easy to break during iterative work.

It is intentionally code-level and specific to this repository.

## 1. Project Structure

Current source layout:

```text
src/
  app.rs
  state.rs
  main.rs
  layout/
    mod.rs
    header.rs
    footer.rs
    context.rs
    state.rs
  context/
    mod.rs
    conversion/
      mod.rs
      index.rs
      state.rs
    bulletin-board/
      mod.rs
      index.rs
      state.rs
      todo/
        state.rs
        index.rs
```

This is not cosmetic. The split is structural:

- `main.rs`
  - terminal bootstrap
  - keyboard enhancement setup
  - mouse capture setup / teardown
  - shared rendering helpers
  - theme
  - mock data types
- `state.rs`
  - app-global state
  - focus state
- `app.rs`
  - app-level event dispatch
  - key handling
  - mouse handling
  - top-level draw composition
- `layout/*`
  - top bar, footer, panel split
- `context/conversion/*`
  - left panel
  - chat transcript
  - input box
  - transcript selection / copy / block hitboxes
- `context/bulletin-board/*`
  - right panel
  - todo area
  - blackboard/model/token/context area
  - right-panel selection / copy

## 2. State Layering

### 2.1 Global app state

Defined in [src/state.rs](/Users/yi./Desktop/yi/flyflors/flyflor-cli/src/state.rs).

Important fields:

- `focus`
  - current focus target
  - one of:
    - `ConversionScroll`
    - `ConversionInput`
    - `BulletinBoard`
- `native_selection_mode`
  - disables app-level mouse behavior
  - used only for raw terminal selection fallback
- `copied_notice`
  - transient UI feedback for successful copy
- `should_quit`
  - final app exit signal

### 2.2 Layout state

Defined in [src/layout/state.rs](/Users/yi./Desktop/yi/flyflors/flyflor-cli/src/layout/state.rs).

- `HeaderState`
  - purely render-facing
  - includes top status and footer text
- `ContextLayoutState`
  - owns the left and right panel states
  - stores panel areas for downstream render / hit-testing

### 2.3 Left panel state

Defined in [src/context/conversion/state.rs](/Users/yi./Desktop/yi/flyflors/flyflor-cli/src/context/conversion/state.rs).

This owns:

- transcript rendered lines
- transcript plain lines
- block hitboxes
- thought hitboxes
- transcript scroll state
- input state
- text selection state

### 2.4 Right panel state

Defined in [src/context/bulletin-board/state.rs](/Users/yi./Desktop/yi/flyflors/flyflor-cli/src/context/bulletin-board/state.rs).

This owns:

- todo state
- questions / details source data
- right-panel rendered lines
- right-panel plain lines
- right-panel scroll state
- right-panel text selection state

## 3. Render Layer Rules

## 3.1 Header / Footer / Panels

Top-level composition happens in [src/app.rs](/Users/yi./Desktop/yi/flyflors/flyflor-cli/src/app.rs).

Layout is:

- 1 line header
- body
- 1 line footer

Rendered by:

- [src/layout/header.rs](/Users/yi./Desktop/yi/flyflors/flyflor-cli/src/layout/header.rs)
- [src/layout/context.rs](/Users/yi./Desktop/yi/flyflors/flyflor-cli/src/layout/context.rs)
- [src/layout/footer.rs](/Users/yi./Desktop/yi/flyflors/flyflor-cli/src/layout/footer.rs)

Important rule:

- footer text comes from right-panel data source, but must render in global
  layout footer, not inside the right panel body.

## 3.2 Left panel rendering

Rendered in [src/context/conversion/index.rs](/Users/yi./Desktop/yi/flyflors/flyflor-cli/src/context/conversion/index.rs).

Panel contains:

- tip line
- transcript area
- input area

Important rule:

- do not casually change the sent-message bubble style
- the user explicitly locked this styling

## 3.3 Right panel rendering

Rendered in [src/context/bulletin-board/index.rs](/Users/yi./Desktop/yi/flyflors/flyflor-cli/src/context/bulletin-board/index.rs).

Panel contains:

- todo block
- separator
- blackboard/details scroll block

Important rule:

- do not merge todo lines into the lower scroll body unless you fully redesign
  selection and scroll ownership
- this already caused duplicated rendering once

## 4. Scroll Model

Shared helpers live in [src/main.rs](/Users/yi./Desktop/yi/flyflors/flyflor-cli/src/main.rs):

- `ScrollState`
- `update_scroll_state`
- `update_scroll_state_from_rendered`
- `apply_scroll_delta`
- `drag_scroll`
- `compute_scrollbar`

There are two different scroll update modes:

- `update_scroll_state_from_rendered`
  - used by left transcript
  - line list is already fully expanded
- `update_scroll_state`
  - used by wrapped right-panel content

Important rule:

- virtual scroll behavior is sensitive
- do not rewrite `scroll`, `max_scroll`, `total_visual_lines`, or
  `stick_to_bottom` logic casually

## 5. Selection and Copy Architecture

This section is the one most likely to regress.

### 5.1 Why this is tricky

The app supports:

- app-owned selection
- app-owned copy
- iTerm-compatible `Cmd+C`
- mouse capture
- scrollbar drag

These all interact.

The failure mode we hit repeatedly was:

- visually selecting text inside the TUI
- but still triggering terminal-native copy
- which made iTerm show:
  - "Looks like you're trying to copy to the pasteboard, but mouse reporting
    has prevented making a selection"

The fix is not "just copy differently".
The fix is a full chain:

1. app must own selection
2. app must intercept the copy shortcut
3. app must write to clipboard itself
4. app must not fall back to copying stale selection
5. keyboard enhancement flags must be enabled so `Cmd+C` reaches the app

### 5.2 Left panel selection

Implemented in [src/context/conversion/state.rs](/Users/yi./Desktop/yi/flyflors/flyflor-cli/src/context/conversion/state.rs).

Core APIs:

- `start_selection`
- `update_selection`
- `selected_line_bounds`
- `selection_text`
- `copy_selection`
- `has_active_selection`
- `clear_selection`

Selection uses:

- `SelectionPoint { line, column }`
- `TextSelection { anchor, focus }`

Important rule:

- selection coordinates are based on plain rendered lines inside the current
  transcript viewport
- `selection_point_at` depends on `content_area` and `scroll.scroll`

### 5.3 Right panel selection

Implemented in [src/context/bulletin-board/state.rs](/Users/yi./Desktop/yi/flyflors/flyflor-cli/src/context/bulletin-board/state.rs).

It mirrors the left panel conceptually:

- `start_selection`
- `update_selection`
- `selected_line_bounds`
- `selection_text`
- `copy_selection`
- `has_active_selection`
- `clear_selection`

Important rule:

- right-panel selection currently applies only to the lower content area
- not to the todo block
- do not silently mix todo lines into the same selection space unless you also
  redesign render ownership and hit-testing

### 5.4 App-level copy dispatch

Implemented in [src/app.rs](/Users/yi./Desktop/yi/flyflors/flyflor-cli/src/app.rs).

Current behavior intentionally follows `deepseek-tui` style priorities:

- copy only when there is an active selection
- prefer actual active panel selection
- after successful copy:
  - clear left selection
  - clear right selection
  - set copied notice

This is implemented in:

- `is_copy_shortcut`
- `copy_active_selection`

Important rule:

- never "guess-copy" from another panel if no panel has an active selection
- stale-selection copy is a real regression we already hit

### 5.5 Mouse drag selection

App-level mouse handling is in [src/app.rs](/Users/yi./Desktop/yi/flyflors/flyflor-cli/src/app.rs).

Drag modes:

- `Scroll(Conversion|BulletinBoard)`
- `ConversionSelection`
- `BulletinBoardSelection`

Important rule:

- scrollbar drag always has priority over text selection drag
- left and right selections must clear each other when a new panel selection
  begins

### 5.6 Auto-copy on mouse-up

Current behavior mirrors `deepseek-tui`:

- selection drag ends on left-button up
- if selection is active, copy immediately

This happens in [src/app.rs](/Users/yi./Desktop/yi/flyflors/flyflor-cli/src/app.rs).

Important rule:

- mouse-up should not copy if there is no active range
- a click-without-drag is not a copy

## 6. iTerm / Cmd+C / Mouse Capture

This is the most important compatibility section.

### 6.1 The real cause of the iTerm warning

The warning is not caused by our clipboard write itself.

It happens when:

- mouse reporting is active
- terminal-native text selection is prevented
- user triggers terminal-native copy path

So if `Cmd+C` is not intercepted by the app, iTerm may treat it as a terminal
copy action and show the warning.

### 6.2 The key fix we borrowed from deepseek-tui

The crucial piece is keyboard enhancement flags.

Implemented in [src/main.rs](/Users/yi./Desktop/yi/flyflors/flyflor-cli/src/main.rs):

- `push_keyboard_enhancement_flags`
- `pop_keyboard_enhancement_flags`

Startup:

- `PushKeyboardEnhancementFlags(DISAMBIGUATE_ESCAPE_CODES)`

Why this matters:

- without it, macOS/iTerm may not deliver `Cmd+C` as a `SUPER`-modified key
  event to crossterm
- then the app never sees the copy shortcut
- then iTerm falls back to terminal-native copy behavior
- then the warning appears

### 6.3 Mouse capture rules

Mouse capture is enabled by default during normal interaction.

Setup / teardown is in [src/main.rs](/Users/yi./Desktop/yi/flyflors/flyflor-cli/src/main.rs).

Important rule:

- do not casually rewrite this behavior when fixing unrelated layout issues
- we already regressed it multiple times by changing nearby code for visual
  tasks

### 6.4 Native selection mode

There is also `native_selection_mode`.

Purpose:

- allow raw terminal selection when explicitly toggled

Important rule:

- this is a fallback mode only
- it is not the main copy path
- primary path must remain app-owned selection + app-owned copy

## 7. Style Locks and Non-Negotiable UI Constraints

These came from repeated user feedback and should be treated as locked unless
explicitly changed again.

### 7.1 Left sent-message bubble

Locked:

- blue block accent
- gray background
- content placement
- vertical padding behavior
- left offset alignment relative to separator

Do not restyle this casually.

### 7.2 Footer placement

The runtime footer like:

```text
00:16:42 | 3 turns | local | healthy
```

must render in global layout footer, not inside the right panel body.

### 7.3 Right panel scrollbar

The user is highly sensitive to:

- distance from content
- distance from panel edge
- visual right alignment

Change this only when explicitly asked.

### 7.4 Bottom whitespace

The lower layout must stay compact.

Important rule:

- reducing whitespace is acceptable
- changing overall layout proportions unexpectedly is not

## 8. Known Safe Edit Zones

Relatively safe to edit:

- `src/layout/footer.rs`
- `src/layout/header.rs` for copy/status text only
- `src/context/bulletin-board/todo/index.rs` for todo text styling
- right-panel internal widths / margins if explicitly requested

High-risk edit zones:

- `src/main.rs` terminal bootstrap
- `src/app.rs` mouse / key dispatch
- `src/context/conversion/state.rs` selection and scroll interaction
- `src/context/bulletin-board/state.rs` selection and line-model behavior

## 9. Before Changing Anything

Checklist:

1. Is this a visual-only change, or does it affect selection / scroll / mouse?
2. Does it touch terminal bootstrap or keyboard enhancement flags?
3. Does it change a locked visual?
4. Does it alter which lines are rendered vs which lines are copied?
5. Does it change panel hit-testing areas?

If the answer to 2, 4, or 5 is yes, review:

- [src/main.rs](/Users/yi./Desktop/yi/flyflors/flyflor-cli/src/main.rs)
- [src/app.rs](/Users/yi./Desktop/yi/flyflors/flyflor-cli/src/app.rs)
- [src/context/conversion/state.rs](/Users/yi./Desktop/yi/flyflors/flyflor-cli/src/context/conversion/state.rs)
- [src/context/bulletin-board/state.rs](/Users/yi./Desktop/yi/flyflors/flyflor-cli/src/context/bulletin-board/state.rs)

## 10. Verification Commands

Use these after behavior changes:

```bash
cargo check
cargo test
```

Manual checks that matter:

1. Left transcript drag-select then `Cmd+C`
2. Right blackboard drag-select then `Cmd+C`
3. Scrollbar drag still works
4. Mouse wheel routing still goes to the hovered panel
5. iTerm no longer shows native copy warning during app-owned copy

## 11. DeepSeek-TUI Copy Reference

This section lists the exact upstream reference points we copied from or
matched conceptually. These are here specifically to avoid "I vaguely remember
the behavior" regressions.

Repository used for reference:

- `/tmp/DeepSeek-TUI`

### 11.1 Copy shortcut predicate

Reference:

- `/tmp/DeepSeek-TUI/crates/tui/src/tui/key_shortcuts.rs:12-24`

Key excerpt:

```rust
/// Copy-to-clipboard: `Cmd+C` on macOS or `Ctrl+Shift+C` elsewhere.
pub(super) fn is_copy_shortcut(key: &KeyEvent) -> bool {
    let is_c = matches!(key.code, KeyCode::Char('c') | KeyCode::Char('C'));
    if !is_c {
        return false;
    }

    if key.modifiers.contains(KeyModifiers::SUPER) {
        return true;
    }

    key.modifiers.contains(KeyModifiers::CONTROL) && key.modifiers.contains(KeyModifiers::SHIFT)
}
```

Why it matters:

- `Cmd+C` must be app-owned on macOS
- if this is not intercepted, iTerm may fall back to terminal-native copy

Our local counterpart:

- [src/app.rs](/Users/yi./Desktop/yi/flyflors/flyflor-cli/src/app.rs)
  - `is_copy_shortcut`

### 11.2 UI-level copy dispatch

Reference:

- `/tmp/DeepSeek-TUI/crates/tui/src/tui/ui.rs:2713-2728`

Key excerpt:

```rust
KeyCode::Char('c') | KeyCode::Char('C')
    if key_shortcuts::is_copy_shortcut(&key) =>
{
    copy_active_selection(app);
}
KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
    match ctrl_c_disposition(app) {
        CtrlCDisposition::CopySelection => {
            copy_active_selection(app);
            app.viewport.transcript_selection.clear();
        }
```

Why it matters:

- copy should be selection-driven
- copy is not a generic fallback action
- after copy, the active selection is cleared

Our local counterpart:

- [src/app.rs](/Users/yi./Desktop/yi/flyflors/flyflor-cli/src/app.rs)
  - `copy_active_selection`
  - copy-shortcut handling inside `handle_key`

### 11.3 Mouse-driven selection lifecycle

Reference:

- `/tmp/DeepSeek-TUI/crates/tui/src/tui/mouse_ui.rs:78-133`

Key excerpt:

```rust
MouseEventKind::Down(MouseButton::Left) => {
    app.viewport.transcript_scrollbar_dragging = false;
    app.viewport.selection_autoscroll = None;

    if mouse_hits_transcript_scrollbar(app, mouse) {
        app.viewport.transcript_scrollbar_dragging = true;
        return Vec::new();
    }

    if let Some(point) = selection_point_from_mouse(app, mouse) {
        app.viewport.transcript_selection.anchor = Some(point);
        app.viewport.transcript_selection.head = Some(point);
        app.viewport.transcript_selection.dragging = true;
    } else if app.viewport.transcript_selection.is_active() {
        app.viewport.transcript_selection.clear();
    }
}
MouseEventKind::Drag(MouseButton::Left) => {
    if app.viewport.transcript_scrollbar_dragging {
        scroll_transcript_to_mouse_row(app, mouse.row);
        return Vec::new();
    }

    if app.viewport.transcript_selection.dragging {
        update_selection_drag(app, mouse);
    }
}
MouseEventKind::Up(MouseButton::Left) if app.viewport.transcript_selection.dragging => {
    app.viewport.transcript_selection.dragging = false;
    app.viewport.selection_autoscroll = None;
    if selection_has_content(app) {
        copy_active_selection(app);
    }
}
```

Why it matters:

- scrollbar drag must beat selection drag
- selection starts on mouse-down, grows on drag, copies on mouse-up
- click outside a valid selection zone clears the existing selection

Our local counterpart:

- [src/app.rs](/Users/yi./Desktop/yi/flyflors/flyflor-cli/src/app.rs)
  - `DragMode`
  - `handle_mouse`

### 11.4 Selection data model

Reference:

- `/tmp/DeepSeek-TUI/crates/tui/src/tui/selection.rs:7-63`

Key excerpt:

```rust
pub struct TranscriptSelectionPoint {
    pub line_index: usize,
    pub column: usize,
}

pub struct TranscriptSelection {
    pub anchor: Option<TranscriptSelectionPoint>,
    pub head: Option<TranscriptSelectionPoint>,
    pub dragging: bool,
}

impl TranscriptSelection {
    pub fn clear(&mut self) {
        self.anchor = None;
        self.head = None;
        self.dragging = false;
    }

    pub fn is_active(&self) -> bool {
        self.anchor.is_some() && self.head.is_some()
    }
```

Why it matters:

- selection is an explicit state object
- drag state belongs to selection lifecycle, not just arbitrary mouse flags

Our local analogs:

- [src/context/conversion/state.rs](/Users/yi./Desktop/yi/flyflors/flyflor-cli/src/context/conversion/state.rs)
  - `SelectionPoint`
  - `TextSelection`
- [src/context/bulletin-board/state.rs](/Users/yi./Desktop/yi/flyflors/flyflor-cli/src/context/bulletin-board/state.rs)
  - `SelectionPoint`
  - `TextSelection`

### 11.5 Clipboard writing strategy

Reference:

- `/tmp/DeepSeek-TUI/crates/tui/src/tui/clipboard.rs:92-146`

Key excerpt:

```rust
pub fn write_text(&mut self, text: &str) -> Result<()> {
    if let Some(clipboard) = self.clipboard.as_mut()
        && clipboard.set_text(text.to_string()).is_ok()
    {
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    if write_text_with_pbcopy(text).is_ok() {
        return Ok(());
    }

    write_text_with_osc52(text)
        .map_err(|err| anyhow::anyhow!("Clipboard unavailable: {err}"))
}
```

Why it matters:

- clipboard must be app-owned
- macOS fallback through `pbcopy` is a deliberate compatibility choice

Our local implementation is simpler but follows the same intent:

- [src/context/conversion/state.rs](/Users/yi./Desktop/yi/flyflors/flyflor-cli/src/context/conversion/state.rs)
  - `copy_selection`
- [src/context/bulletin-board/state.rs](/Users/yi./Desktop/yi/flyflors/flyflor-cli/src/context/bulletin-board/state.rs)
  - `copy_selection`

Current local difference:

- we directly use `arboard::Clipboard`
- we do not yet include a `pbcopy` fallback layer
- if future macOS clipboard instability appears again, this is the next place
  to copy more literally from DeepSeek-TUI

### 11.6 Keyboard enhancement flags

Reference:

- `/tmp/DeepSeek-TUI/crates/tui/src/tui/ui.rs:6413-6465`

Key excerpt:

```rust
fn push_keyboard_enhancement_flags<W: Write>(writer: &mut W) {
    #[cfg(not(windows))]
    if let Err(err) = execute!(
        writer,
        PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
    ) {
        tracing::debug!(?err, "PushKeyboardEnhancementFlags ignored");
    }
}

pub(crate) fn pop_keyboard_enhancement_flags<W: Write>(writer: &mut W) {
    #[cfg(not(windows))]
    let _ = execute!(writer, PopKeyboardEnhancementFlags);
}
```

Why it matters:

- without this, `Cmd+C` may not be delivered as a `SUPER`-modified event
- then the app never sees the copy shortcut
- then iTerm may show the mouse-reporting copy warning

Our local counterpart:

- [src/main.rs](/Users/yi./Desktop/yi/flyflors/flyflor-cli/src/main.rs)
  - `push_keyboard_enhancement_flags`
  - `pop_keyboard_enhancement_flags`

### 11.7 Hard rule for future edits

If copy regresses again, inspect these in this order:

1. [src/main.rs](/Users/yi./Desktop/yi/flyflors/flyflor-cli/src/main.rs)
   - keyboard enhancement setup
   - mouse capture setup
2. [src/app.rs](/Users/yi./Desktop/yi/flyflors/flyflor-cli/src/app.rs)
   - `is_copy_shortcut`
   - `copy_active_selection`
   - mouse-up selection copy path
3. [src/context/conversion/state.rs](/Users/yi./Desktop/yi/flyflors/flyflor-cli/src/context/conversion/state.rs)
   - left selection start / update / extraction
4. [src/context/bulletin-board/state.rs](/Users/yi./Desktop/yi/flyflors/flyflor-cli/src/context/bulletin-board/state.rs)
   - right selection start / update / extraction

Do not start by changing visual layout or scrollbar spacing when the symptom is
"copy is broken". That has already caused repeated unrelated regressions.
