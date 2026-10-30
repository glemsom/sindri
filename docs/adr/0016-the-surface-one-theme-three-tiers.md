---
status: accepted
date: 2026-10-31
---

# 0016 — The surface: one theme, three tiers

## Decision

1. **One theme owns colour.** `theme.rs` names every colour the frame paints, and
   `PALETTE` lists them; no renderer writes a colour down. `tests/theme.rs` holds
   both halves to it: a rendered frame is walked cell by cell and every foreground and
   background must be a colour the theme names, and the source tree is scanned for a
   colour literal outside `theme.rs`. Syntax highlighting is the one exception, and it
   is syntect's to own: it arrives as a `Color::Rgb` built from a theme's own bytes.
2. **Three tiers of nearness.** The canvas is the terminal's own black, painted by
   nothing. `PANEL` (`#171a21`) is a surface the frame draws: the composer, a fenced
   code block, the rail. `RAISED` (`#232733`) is a step above a panel: an overlay.
   `theme::surface()` and `theme::raised()` are the constructors; a renderer asks for a
   tier, never for a background.
3. **The composer floats.** It is a `PANEL` with a `BorderType::Rounded` border that
   takes `PROMPT` while it holds the focus and `FAINT` while a run has it. Both are
   readable: the border says where a keystroke lands, and says it while the user is
   typing into a run.
4. **A fence is a panel.** A fenced block is drawn as a rounded panel across the
   column, titled with its language, with a gutter down its left edge. A line too wide
   for the panel is cut and ends in `…`: a fence that quietly swallowed the rest of a
   line would lie about the code. Inline code keeps `CODE_BG`, which sits above a panel
   as well as on the canvas.
5. **A modal is a modal.** An overlay draws one `Modifier::DIM` pass over the frame
   before its own `Clear`, so what it covers recedes instead of reading as a hole. The
   overlay itself is a `RAISED` panel, drawn at full strength.
6. **The rail is a panel with an edge.** It is filled with `PANEL` and set one column
   off the transcript, so the frame has an edge between the words and the facts.
7. **No meaning by hue alone.** A running call's stdout is marked `│` and its stderr
   `⚠`: the two sides differ in a terminal that has lost its colour.
8. **`FAINT` is body text and clears 4.5:1** against both the canvas and the panel
   (`#8f98a8`). A test computes the WCAG ratio rather than trusting the number.

## Consequences

- The golden frames re-record: the composer's border, the fence's panel and the rail's
  gap all move the cells the frame tests read.
- The transcript column gives one column to the rail's gap, and a note's marker now
  takes its columns out of the wrap width, so a wrapped note is no longer cut mid-word.
- Rounded borders and fills are the one thing a buffer cannot prove: they were checked
  in `tmux`, where the escapes reach a real pty as they do over SSH, before the rest of
  the track built on them (ADR-0011 §13): `╭╮╰╯`, the panel and raised fills and the
  dim pass all survive the hop.
- A new surface or colour is a change to `theme.rs` and its `PALETTE`, or the tests
  refuse it.
