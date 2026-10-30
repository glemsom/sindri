---
status: accepted
date: 2026-10-30
---

# 0011 — The terminal UI: a renderer over the core's event stream

## Decision

1. **A front end, not a second loop.** `crates/tui` renders `Event` values and returns a `Report`: no loop, no retry policy, no sandbox code. Core imports no front end (ADR-0001). Bare `sindri` on a TTY is the frame (`crates/cli/src/frame.rs`); piped stdin is refused, exit 2 (ADR-0004).
2. **The core streams; batch does not notice.** `provider/rig_wire.rs` streams; `Event` gains `TextDelta` and `ReasoningDelta` beside `ToolCall`. Batch accumulates deltas, so its stdout stays byte-identical. No rig type reaches the TUI (ADR-0003).
3. **Libraries.** `ratatui` + `crossterm`, `ratatui-image`, `pulldown-cmark` + `syntect`; a render loop fed by a channel, drawing when there is something to say, never on a clock.
4. **Layout.** Transcript column left; rail 32 columns (`RAIL_COLUMNS`), visible at ≥100 columns (`RAIL_MIN_WIDTH`), a one-line strip below that, never behind a keystroke; composer 2–6 text rows (`MIN_ROWS`/`MAX_ROWS`) plus one footer hint line. The idle frame is not drawn: every frame ends by placing the terminal cursor in the composer, since a repaint restarts its blink. A resize is a message, and nothing else is drawn unaided.
5. **The face.** `face.png` (795×989, square crop, 20×20 cells, `ROWS = 20`) draws only under Kitty, with no placeholder; nothing in the layout depends on it.
6. **Markdown.** Headings, nested lists, tables, blockquotes, inline code, bold/emphasis, links, rules, fenced code in truecolour, diffs line-coloured. No images, no raw HTML.
7. **Mouse, selection, clipboard.** Mouse is captured; a selection copies the rendered rows via OSC 52, then `wl-copy`/`xclip`; `Shift+drag` passes through to the terminal. Copy semantics: ADR-0013.
8. **One conversation per launch.** Turns accumulate; `/new` opens a new GUID with an empty transcript; resume is not in v1.
9. **Tool calls.** One line per call plus a live tail of its last five dim lines, collapsing to the exit status; the spill file opens with `Ctrl-O` or a click (ADR-0013).
10. **Cancel and queue.** Ctrl-C is batch semantics: shared flag, process-group kill, nothing on stdout. The draft survives; a prompt typed mid-run is queued (steering is not in v1).
11. **Reasoning.** Streams as a dim block, collapses to a one-line "reasoned" marker, expands on demand.
12. **`/settings`.** Edits the one config file through the core's `Document`; a save applies to the next prompt with no restart (ADR-0014).
13. **Compatibility.** 24-bit truecolour assumed, no 256-colour fallback; must work in `tmux` and over SSH; one dark theme.
14. **Keys.** `Enter` sends, `Shift+Enter` newlines, `@` completes paths, `/` names commands, `Esc` closes, `Ctrl-C` cancels or clears, `Ctrl-Q` quits, `PgUp`/`PgDn` scroll, `?` opens help, `Tab` accepts a completion.
15. **Failure is content.** Boot runs the doctor's checks before the first frame; a failure is a full-screen card naming the check and printing the sandbox description, never a half-drawn frame. Mid-run failures land in the transcript where they happened, with the stop reason in the rail; toasts are for success only.
16. **`@` completion.** A capped workspace walk on its own thread, inserted as a workspace-relative path; skips `.git` only. Cap, ranking and threading: ADR-0013.

## Amendment

17. **A missing credential opens the modal, not a card.** §15's card is for a check that fails. A value the run has none of is the one failure the frame answers with a field: a bare `sindri` with no key opens with the settings modal staged on `api_key`, saying where the key comes from, where it is written, that the file is `0600` and that the value is never shown again (ADR-0004). `Esc` leaves a frame that still reads and browses, the missing key said once in the transcript, the composer stating what it cannot send; a prompt typed while it is missing is refused and the draft stays. Batch mode is untouched.

## Consequences

- The core changed twice for the TUI: an `Event` variant pair and a streaming path in the pinned adapter. Streaming assembly through the mock provider server is the regression net.
- The render tree stays bounded even when the transcript does not: windowed layout, per-entry caps, spill files for the rest.
- The TUI writes exactly one file of user state, through the schema that already exists.
- `ratatui` buffer snapshots are the net for the frame; the process seam (exit codes, refusals) stays the net for launch.
