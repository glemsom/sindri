---
status: accepted
date: 2026-10-30
---

# 0013 — The hands: copy, scroll, completion, spill

## Decision

**Copy** (`crates/tui/src/copy.rs`) — `Copied::Sent`, `Copied::Tool(name)` or `Copied::Refused(why)`.

- OSC 52 goes out whenever stdout is a terminal and the selection ≤ `OSC52_LIMIT` = 100 000 bytes.
- Wording is *sent to the terminal (OSC 52), which does not confirm* — never "copied".
- Fallback: `wl-copy`, then `xclip -selection clipboard`; the one that exits cleanly is named.
- A refusal becomes an error note; no path turns a refusal into a success.
- `Clipboard` is a trait, so no test touches the real clipboard.

**Window** — the window has a place in the thread, not an offset from its tail: the
entry and the row into it that its top row shows. A window on the tail is laid out from
the newest entry back to a screenful and follows what arrives; a window a reader has
scrolled back is laid out from its place forwards, so a run that appends below them
never moves the text they are reading. Heights are cached per entry per width, dropped
when an entry changes. Moving by rows measures only the entries it crosses, so a page
costs a screenful plus the offset walked and never the thread; no total is kept. The rows
below the window are counted once and then added to while the only change is at the tail
— a run only appends there — so a window held a page back does not walk the thread on
every frame, and anything that changes the thread elsewhere (an entry opened or folded)
forgets the count. `View::measured` is that cost, and the tests assert it on a
five-hundred-turn thread. Wheel = 3 rows, page = a screenful.

The frame says where the reader is and what they are missing. Rows that arrive below a
window off the tail are counted — `↓ 4 new rows` in the footer, against the right edge,
cleared when the window reaches the tail again — and a thread longer than the window
carries a thin marker (`┃`) in the transcript's last column, over the entries the window
shows, pinned to the top at the first turn and to the bottom on the tail. It is placed by
entry and not by row because the thread's rows are never counted: what the window can
afford to know is which entries it shows (issue #45).

**Keys** — `PgUp`/`PgDn` and the wheel scroll, `Home` is the first turn and `End` the tail (which is followed again), spill is `Ctrl-O`, drag is `Shift+Up`/`Shift+Down` from the newest row up, `Ctrl-Y` copies the selection or, with none, the last answer.

**Selection** — resolved by display columns against the plain text of the last frame; wide glyphs taken whole; only blank rows dropped; the first row's indentation kept.

**Overlay** — it owns the keys: `Esc`/`Enter` closes, `PgUp`/`PgDn` pages, wheel pages, `Ctrl-C`/`Ctrl-Q` always live. Dismissing returns the composer's draft.

**Completion** (`crates/tui/src/completion.rs`) — `@` scans on its own thread, one walk at a time; an answer applies only if its generation is newest, then the query is re-asked.

- `CAP` = 20 000 paths; the `capped` flag and a row in the offer both say so.
- Skips `.git` and nothing else: `.gitignore` is deliberately not honoured.
- Ranking: whole-path prefix, file-name prefix, substring, subsequence. Eight rows. Dotfiles only after a dot.
- Accepting replaces the `@` token only: `look at @src/ma please` → `look at src/main.rs please`.

**Spill** — read once when opened, as bytes with invalid UTF-8 replaced; one screen of lines held, paged by screenfuls. `Ctrl-O`, or a click on that row, opens *that* row's file.

**Expansion** — a click on a call's own row opens or folds that call, exactly as `Enter` does on the newest one (ADR-0011 §9); a click on its spill row still opens the file, because that row is a different row. A gesture an entry has is never the only way to it.

## Amendment

**The overlays own their scroll.** A page is a screenful of the box that has the
screen, never of the transcript it covers: the frame reports what its box showed, and
`PgUp`/`PgDn` and the wheel page it by that. The help overlay wraps each description
under its keys column and sizes its box from the wrapped rows, so a binding is reached
by scrolling rather than being the row that fell off; when the list is longer than the
box it states its position (`3 of 12`). The settings modal scrolls its rows with the
cursor and keeps the `file` line, the warning, a refused save's message and the hints
pinned outside the scroll, so a save never fails silently on a row that is not drawn.
Both mark the edge they run past (`↑ more`, `↓ more`). `Esc` and `Enter` are unchanged.

## Consequences

- No clipboard claim that cannot be verified; over SSH and in `tmux` the wording is the same as locally.
- Frame memory is bounded by the visible window plus cached heights, not by thread length.
- Completion cost is one walk per query change on a thread, up to the cap; the cap and the ranking are the knobs, not the layout.
