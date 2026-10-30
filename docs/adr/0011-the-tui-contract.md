---
status: accepted
date: 2026-10-30
---

# 0011 — The terminal UI: a renderer over the core's event stream

## Decision

1. **A front end, not a second loop.** `crates/tui` renders `Event` values and returns a `Report`: no loop, no retry policy, no sandbox code. Core imports no front end (ADR-0001). Bare `sindri` on a TTY is the frame (`crates/cli/src/frame.rs`); piped stdin is refused, exit 2 (ADR-0004).
2. **The core streams; batch does not notice.** `provider/rig_wire.rs` streams; `Event` gains `TextDelta` and `ReasoningDelta` beside `ToolCall`. Batch accumulates deltas, so its stdout stays byte-identical. No rig type reaches the TUI (ADR-0003).
3. **Libraries.** `ratatui` + `crossterm`, `ratatui-image`, `pulldown-cmark` + `syntect`; a render loop fed by a channel, drawing when there is something to say, and on the clock while something is moving (§4).
4. **Layout.** Transcript column left, laid out as a document: two blank rows between turns and none inside one, a two-column gutter a turn's answer and calls hang off, and the prompt at full strength on its own band; rail 32 columns (`RAIL_COLUMNS`), set one column off the transcript, visible at ≥100 columns (`RAIL_MIN_WIDTH`), a one-line strip below that, never behind a keystroke; composer 2–6 text rows (`MIN_ROWS`/`MAX_ROWS`), a **queue panel** of 3–8 rows between the transcript and the composer when the queue is not empty, and one footer hint line. The idle frame is not drawn: every frame ends by placing the terminal cursor in the composer, since a repaint restarts its blink. A resize is a message.
    **Never repaint for nothing.** The one thing drawn unaided is the pulse (§19), and a tick exists only while a run is live or a completion is walking: then the loop's wait carries a `pulse::TICK` timeout (~120 ms) and the frame is redrawn on it. While nothing is happening the loop waits on the channel and costs the clock nothing. "Never on a clock" was this rule's earlier, stricter form; what it always meant is that no frame is drawn that says nothing.
5. **The face.** `face.png` (795×989, square crop, 20×20 cells, `ROWS = 20`) draws only under Kitty, with no placeholder; nothing in the layout depends on it.
6. **Markdown.** Headings, nested lists, tables, blockquotes, inline code, bold/emphasis, links, rules, fenced code in truecolour, diffs line-coloured. No images, no raw HTML.
7. **Mouse, selection, clipboard.** Mouse is captured; a selection copies the rendered rows via OSC 52, then `wl-copy`/`xclip`; `Shift+drag` passes through to the terminal. Copy semantics: ADR-0013.
8. **One conversation per launch.** Turns accumulate; `/new` opens a new GUID with an empty transcript; resume is not in v1.
9. **Tool calls.** One line per call plus a live tail of its last five dim lines, collapsing to its status, its duration and the count of what is behind it (`✓ exit 0 · 1.4s · 400 lines`). Any call opens in place on `Enter` with an empty composer — the call the reader last touched, or the newest — or on a click on the call's own row, and the same gesture folds it again. The entry keeps a bounded tail of its whole output (`FULL_LINES` = 400, capped the way the spill file is), so a call that never spilled is still openable: the spill is an optimisation, not the feature. The spill file still opens with `Ctrl-O` or a click on its own row (ADR-0013, issue #44).
10. **Cancel and queue.** Ctrl-C is batch semantics: shared flag, process-group kill, nothing on stdout. The draft survives; a prompt typed mid-run is queued (steering is not in v1).
    The queue is drawn: the **queue panel** above the composer holds the queued prompts
    whole, titled `queued` when one waits and `N queued` when more, cut at six rows with
    that count standing for the rest. `↑` takes the newest one back into the composer, above any draft typed since,
    and `Ctrl-X` drops it; the footer carries no count it cannot fit. A cancel is answered
    the moment the key lands rather than when the request does: the footer reads
    `cancelling…` with the draft still in the composer, and the state clears when the run has
    actually stopped. An in-flight request is not interrupted (ADR-0004), so the frame's
    only honest answer to the key is to say what it is waiting for.
11. **Reasoning.** Streams as a dim block, collapses to a one-line "reasoned" marker, expands on demand — the key and the click of §9, opening the text it held.
12. **`/settings`.** Edits the one config file through the core's `Document`; a save applies to the next prompt with no restart (ADR-0014).
13. **Compatibility.** 24-bit truecolour assumed, no 256-colour fallback; must work in `tmux` and over SSH; one dark theme (§23).
14. **Keys.** `Enter` sends, `Shift+Enter` newlines, `@` completes paths, `/` names commands, `Esc` closes, `Ctrl-C` cancels or clears, `↑` takes back a queued prompt, `Ctrl-X` drops one, `Ctrl-Q` quits, `PgUp`/`PgDn` scroll, `Home`/`End` go to the first turn and back to the tail, `?` opens help, `Tab` accepts a completion, `Ctrl-O` opens a spill file, `Ctrl-Y` copies, `Ctrl-G` copies the session id, and `Enter` on an empty composer opens the call it last showed (§9).
15. **Failure is content.** Boot runs the doctor's checks before the first frame; a failure is a card naming the check and printing the doctor's report, never a half-drawn frame. The card holds that report whole: each line of it on a row of its own, the block sized from the rows the words wrapped into and centred on the screen. A report taller than the screen is paged with `PgUp`/`PgDn` and marked on the border, the last row saying both the key that leaves and `sindri doctor` as the command that checks again, and only a key leaves it — a resize lays it out again, so a `tmux` client attaching cannot dismiss a report before it has been read (issue #50). Mid-run failures land in the transcript where they happened, with the stop reason in the rail; toasts are for success only.
16. **`@` completion.** A capped workspace walk on its own thread, inserted as a workspace-relative path; skips `.git` only. Cap, ranking and threading: ADR-0013.

## Amendment

17. **A missing credential opens the modal, not a card.** §15's card is for a check that fails. A value the run has none of is the one failure the frame answers with a field: a bare `sindri` with no key opens with the settings modal staged on `api_key`, saying where the key comes from, where it is written, that the file is `0600` and that the value is never shown again (ADR-0004). `Esc` leaves a frame that still reads and browses, the missing key said once in the transcript, the composer stating what it cannot send; a prompt typed while it is missing is refused and the draft stays. Batch mode is untouched.

18. **Reading the record.** `/sessions` lists what the state directory holds — time, model,
    turns, stop reason and first prompt, newest first, read from `session.json` — and
    `Enter` reads one back in the spill-style box, read-only and scrollable. `/export
    [path]` writes the current thread as markdown, says the path before writing and where
    it went after, and refuses to replace a file that is already there until the same
    `/export` is asked twice. The rail's GUID is copied with `Ctrl-G` or a click on its
    row, so `sindri sessions show` still takes it. §8 is untouched: reading is not
    resuming, and no path from the picker writes a record (ADR-0006).

19. **The pulse.** A run keeps a clock, and the tick's visible marker is its
    pulse. Three things move, and all three settle rather than continue: the
    newest answer carries a braille frame at its head in `PROMPT` with the
    run's elapsed time beside it, and stops on `✓` — or `✗` where the turn
    failed — beside the duration it stopped at; every call carries its own
    duration, counting up while it runs and frozen at the one it ended on; the
    rail's output and context figures move on what has streamed, at the only
    rate the frame can honestly guess (four characters a token), each accented
    for a moment as it grows and both trued up by the provider's report when
    the run ends. The guess never outlives the run that made it; §4 has the
    rule the tick answers to.

20. **The window has a position.** The transcript window is anchored to the entry
    being read, not to an offset from the tail: a run that streams while the reader is
    scrolled back does not move the text under them, `End` returns to the tail and
    follows it again, and `Home` reaches the first turn. Rows that arrive below a
    reader who is off the tail are counted in the footer (`↓ 4 new rows`) and the
    count clears when they return; a thread longer than the window marks its position
    in the transcript's last column. `PgUp`/`PgDn` and the wheel are unchanged
    (ADR-0013, issue #45).

21. **The first frame is not a blank page.** A thread with nothing in it yet draws
    three facts and an invitation instead of an empty column: one line naming the wire,
    the model, the workspace and the sandbox — all read from `Launch`, which carries the
    provider beside the model — one dim sentence of the contract (one tool, `bash`,
    sandboxed: the host readable, only the chosen paths writable — ADR-0002, ADR-0010),
    and three things that can be typed exactly as they read, two prompts and a command,
    with `/help` beside them. The composer carries a placeholder
    (`Ask, or drop a log here · @ path · / command`) while it holds no text at all, so a
    draft is never shadowed. A thread shorter than the column is drawn on its floor, so
    the newest row is the one above the composer rather than the top row of a screenful
    of blank ones. Nothing in the empty frame claims a fact the launch did not establish
    (issue #46).

22. **The chrome has a budget.** A narrow terminal shortens what the frame says; it never
    cuts a key or a fact in half. The footer states the hints in the one order they are
    worth saying — `Enter send`, `? keys`, `Ctrl-Q quit`, `@ paths`, `/ commands`,
    `Shift+Enter newline`, `PgUp/PgDn scroll` — showing all seven from 120 columns up,
    five from 100, three from 60, and never fewer than the two that send and open the
    list: the set shrinks rather than a word being clipped, and the room a live run's
    `working · ` or a `↓ 4 new rows` badge needs is taken before the hints are chosen.
    The rail is read by one priority order, `rail::priority`, which both the column and
    the strip ask and neither sorts facts of its own; it ends with the posture and puts
    the numbers the run is spending above what the run *is*, so a column too short for
    its facts gives them up from the bottom of that order — the session temp, the branch,
    the record, then the model and the workspace the transcript's first line already
    names — and keeps a row to say how many it lost (`│ +7 more`) rather than losing them
    in silence. On the strip the facts it may never lose (the posture, the model, the
    workspace) are given their room first; where even those three do not fit, the order
    decides what each gets, the posture first, shortened to its head (`⬢ bwrap`). A fact
    too long for its room is shortened, and dropped only when nothing of it can be said
    (issue #48).

23. **The surface: one theme, three tiers.** `theme.rs` names every colour the
    frame paints and `PALETTE` lists them; no renderer writes a colour down.
    `tests/theme.rs` holds both halves: a rendered frame is walked cell by cell
    and every foreground and background must be a colour the theme names, and
    the source tree is scanned for a colour literal outside `theme.rs`. Syntax
    highlighting is the one exception, and it is syntect's to own: it arrives as
    a `Color::Rgb` built from a theme's own bytes. Inside the one dark theme
    there are three tiers of nearness. The **canvas** is the terminal's own
    black, painted by nothing. `PANEL` (`#171a21`) is a surface the frame draws:
    the composer, a fenced code block, the queue panel, the band a turn's
    prompt sits on. `RAISED` (`#232733`) is a step above a panel: an overlay,
    and the chip the rail states its sandbox posture in. The rail draws no
    surface at all: it is chrome on the canvas beside the words, and its rule
    and its chip are the only paint it spends. So the composer is the one panel
    that takes the keyboard and the one panel shaped like an input, and nothing
    competes with it for that weight.
    `theme::surface()` and `theme::raised()` are the constructors; a renderer
    asks for a tier, never for a background. The composer is a `PANEL` with a
    `BorderType::Rounded` border that takes `PROMPT` while it holds the focus
    and `FAINT` while a run has it: both are readable, so the border says where
    a keystroke lands, and says it while the user is typing into a run. An overlay draws one `Modifier::DIM` pass
    over the frame before its own `Clear`, so what it covers recedes instead of
    reading as a hole, and is itself a `RAISED` panel drawn at full strength. No
    meaning is carried by hue alone: a running call's stdout is marked `│` and
    its stderr `⚠` (§9), and a diff says `+`/`-` before it says green or red.
    `FAINT` is body text and clears 4.5:1 against both the canvas and the
    panel (`#8f98a8`), a ratio a test computes rather than trusting. A light
    theme is out of scope and a 256-colour fallback does not exist; the rounded
    borders and fills a buffer cannot prove were checked in `tmux`, where the
    escapes reach a real pty as they do over SSH.

24. **The prose carries its own laws.** A fence is a panel: drawn rounded across
    the column, titled with its language, with a gutter down its left edge, and a
    line too wide for it is cut and ends in `…` — a fence that quietly swallowed
    the rest of a line would lie about the code. Inline code keeps `CODE_BG`,
    which sits above a panel as well as on the canvas. The heading scale is a
    function of the level alone: H1 and H2 are `HEADING` in bold over a rule
    beneath them, the rule spanning the column divided by the level so H1's is
    twice H2's; H3 and deeper are `HEADING` with nothing added, because past H2
    there is nothing left to take away — a level added tomorrow is ranked
    without an arm being written. A link keeps its target: its words are
    `PROMPT` and the URL follows them dimmed in `‹›`, and a link that names
    nowhere is its words as plain text. A diff row is washed as well as
    coloured — an added row `DIFF_ADDED` over `DIFF_ADDED_BG`, a removed one
    `DIFF_REMOVED` over `DIFF_REMOVED_BG` — so the row survives a terminal that
    cannot tell green from red; the gutter and the fill of a tinted row carry
    its wash, so the row is a row and not a coloured word on a panel. A note's
    marker takes its columns out of the wrap width before the words do, so a
    wrapped note is never cut mid-word (§6).

## Consequences

- The core changed twice for the TUI: an `Event` variant pair and a streaming path in the pinned adapter. Streaming assembly through the mock provider server is the regression net.
- The render tree stays bounded even when the transcript does not: windowed layout, per-entry caps, spill files for the rest.
- The TUI writes exactly one file of user state, through the schema that already exists.
- `ratatui` buffer snapshots are the net for the frame; the process seam (exit codes, refusals) stays the net for launch.
- Colour is the theme's, not a renderer's: a new surface or colour is a change to
  `theme.rs` and its `PALETTE`, or `tests/theme.rs` refuses it. The golden frames move
  with the surfaces — the composer's border, the fence's panel and the rail's gutter all
  change the cells those tests read.
