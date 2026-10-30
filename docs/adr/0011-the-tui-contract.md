# 0011 — The terminal UI: a renderer over the core's event stream

- **Status:** accepted
- **Date:** 2026-10-29

## Context

ADR-0001 deferred the TUI until batch mode was finished, on the promise that it
would then be a rendering exercise. Batch mode is finished (v0.1). This ADR
records the decisions that make good on that promise, and the two places where
it does not hold: the core does not stream, and the TUI is the first front end
that writes to the user's disk.

## Decisions

### 1. The TUI is a front end, not a second loop

`crates/tui` renders `Event` values and returns a `Report`. It holds no loop,
no retry policy, no sandbox code. Core still imports no front end; a CI check
keeps it that way (ADR-0001). Bare `sindri` on a TTY is the TUI — today it
prints help and exits 2 — and piped stdin is still refused (ADR-0004).

### 2. The core gains streaming, and batch mode does not notice

`provider/rig_wire.rs` calls the model's streaming entry point; `Event` gains
`TextDelta` and `ReasoningDelta` alongside `ToolCall`. Batch accumulates deltas
into the same string it prints today, so its stdout stays byte-identical and
ADR-0004 is untouched. A screen frozen for the length of a model call is the
one failure no styling can hide, which is why this is not deferred.

### 3. Libraries, so we render rather than invent

`ratatui` + `crossterm` for the frame, `ratatui-image` for the Kitty face,
`pulldown-cmark` + `syntect` for markdown and code, a render loop fed by a
channel that draws a frame when there is something to say and never on a
clock. Provider types stay confined to their adapter (ADR-0003); the TUI
consumes `Event`, never `rig`.

### 4. Layout

Transcript column on the left — the hero. A 32-column rail on the right, always
visible at 100 columns or more, collapsing to a one-line strip below that and
never hidden behind a keystroke: trust you must summon is not trust. A composer
of 2–8 rows pinned at the bottom, one footer line of hints under it.

An idle frame is not drawn. Every frame ends by showing the terminal's cursor and
moving it to the composer, and a terminal restarts its cursor blink on that move
— so a loop that redrew an unchanged frame would leave the composer's cursor
blinking at the frame rate. The loop waits for a message; the only frame it draws
without one is the first. A resize is a message, so the frame still repaints
when the terminal changes size.

### 5. The face, and terminals that cannot show it

`face.png` renders only where the Kitty graphics protocol is available. There is
no fallback image and no placeholder: on every other terminal the image area
closes up and the rail starts at the cache meter. Nothing in the layout depends
on the face. The asset is 795×989; it is rendered from a square source crop at
20×20 cells.

### 6. Markdown

Headings, lists, nested lists, tables, blockquotes, inline code, bold and
emphasis, links, rules, and fenced code with truecolour syntax highlighting,
diffs colour-coded by line. No images in the transcript, no raw HTML. Code is
what an agent emits most, so highlighting is where the interface is won.

### 7. Mouse, selection and the clipboard

The TUI captures the mouse: in-app selection copies exactly the rendered rows,
ANSI stripped, via OSC 52, falling back to `wl-copy`/`xclip` through the shell.
Shift+drag passes through to the terminal's own selection. A copy that the
terminal refused is reported as refused, not as done. Mouse is an accelerator;
every action is reachable from the keyboard.

### 8. What a session is

One growing conversation per launch: each prompt appends a turn, and later turns
see earlier ones and their tool results until the context runs out. `/new`
starts a fresh session GUID with an empty transcript. Resuming a past session is
not in v1.

### 9. Tool calls in the transcript

One line per call, plus a live tail of the last five dim lines while it runs,
collapsing to the exit status when it ends, expandable in place by click or
Enter. An output too large to display is capped and becomes an affordance to
open the spill file the executor already wrote (ADR-0007) — megabytes are shown
honestly without being held in the render tree.

### 10. Cancel and queue

Ctrl-C cancels the run: the shared flag, the process-group kill, nothing on
stdout — the same semantics as batch. The composer survives the cancel with its
draft. A prompt typed while a run is in flight is queued and sent when the run
ends; steering the model mid-run is not in v1.

### 11. Reasoning

When the configured provider offers reasoning, it streams into the transcript as
a dim block, collapses to a one-line "reasoned" marker when the turn ends, and
expands on demand. It is the visible price of the thinking toggle.

### 12. `/settings`

It edits the same file from the same schema as everything else (ADR-0005). Save
writes immediately and applies to the next run without a restart; Esc discards.
The API key field's default state is "from environment"; writing a key into the
file is an explicit opt-in that says, on screen, that it is stored in plaintext.
A key is never echoed, never recorded, never in the transcript.

### 13. Compatibility

24-bit truecolour is assumed; there is no 256-colour fallback to maintain. It
must work inside `tmux` and over SSH, which rules out assuming a local clipboard,
a specific `TERM`, or image support. One dark theme, no light mode in v1.

### 14. Keybindings

Conventional, not vim: `Enter` sends, `Shift+Enter` newlines, `@` completes
paths, `/` names commands, `Esc` closes, `Ctrl-C` cancels or clears, `Ctrl-Q`
quits, `PgUp/PgDn` scroll, `?` opens help, `Tab` accepts a completion.

### 15. Failure is content

Boot runs the same checks as `sindri doctor` before the first frame; a failure
is a full-screen card naming the check and printing the sandbox description, not
a half-drawn interface. Mid-run failures appear in the transcript at the point
they happened, with the stop reason in the rail, and the composer stays usable.
Toasts are reserved for success — copied, saved.

### 16. `@` completion

A filesystem walk of the workspace, fuzzy-filtered as you type, about eight rows,
`Tab`/`Enter` accepts, dotfiles only when a dot is typed, inserted as a
workspace-relative path. `.gitignore` is deliberately **not** honoured: the walk
skips only `.git`, runs in the background so the composer never stalls, caps what
it scans, and ranks by proximity to the typed text. In a repository with a large
build directory this is the accepted cost, and the cap is what keeps it honest.

## Consequences

- The core changes twice for the TUI: an `Event` variant pair and a streaming
  path in the pinned adapter. Streaming assembly through the mock provider
  server (the model seam) is the regression net.
- Batch output is unchanged by construction, and the tests that prove it stay
  green — if they do not, the delta accumulator is wrong.
- The render tree is bounded even when the transcript is not: windowed layout,
  per-entry display caps, spill files for the rest.
- The TUI is the first front end that writes user state. It writes exactly one
  file, through the schema that already exists, and it says so on screen.
- Buffer snapshots (`ratatui`'s test backend) are the regression net for the
  frame; the process seam stays the net for launch, refusal and exit codes.
