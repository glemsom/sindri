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

**Window** — `window()` accumulates entry heights from the newest entry back until a screenful plus the offset, and lays out only what it walked. Heights are cached per entry per width, dropped when an entry changes; the offset is clamped where the walk stopped; no total is kept. Wheel = 3 rows, page = a screenful.

**Keys** — wheel is `PgUp`/`PgDn`, spill is `Ctrl-O`, drag is `Shift+Up`/`Shift+Down` from the newest row up, `Ctrl-Y` copies the selection or, with none, the last answer.

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
