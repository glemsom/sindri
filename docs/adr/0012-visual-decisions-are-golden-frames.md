# A visual decision is frozen as a golden frame, not described in prose

The TUI's look was the one part of the spec no test could reach: "reads as a ledger",
"degrades deliberately", "carries elapsed time" are adjectives, and an adjective cannot be
closed — least of all by an agent, which can build anything it can grade.

So every visual decision becomes an artefact. A frame renders through ratatui's headless
`TestBackend` at a fixed size into `tests/fixtures/<frame>-<cols>x<rows>.txt`, and `insta`
compares it. Colour is asserted as an **`Ink` name** (`dim`, `muted`, `accent`, `ok`,
`err`), never as RGB: a palette change must not break a fixture, and a fixture must not
smuggle in a theme.

What this buys, and what it costs: a human approves a fixture **once** instead of reviewing
renders on every change, and the four visual tickets move from `ready-for-human` to
`ready-for-agent` — the review becomes the approval of a golden, not the writing of code.

The rule that keeps it from ossifying: changing a number in a ticket's contract means
changing the golden in the same commit, deliberately. A fixture updated to match whatever
the code now does is not a test, it is a receipt.

## Amendment

The rule stands; one filename in it does not. The comparison is `insta`'s — it compares a
rendered frame against the accepted one and it blesses a deliberate change — and `insta`
1.49 hard-codes the `.snap` extension when it writes a file snapshot
(`insta-1.49.0/src/runtime.rs:298`: `"{}.snap"`). A fixture at
`<frame>-<cols>x<rows>.txt` would have been a file nothing ever wrote and nothing ever read
back.

So a frame fixture is `tests/fixtures/<frame>-<cols>x<rows>.snap`. What the file holds is
what this ADR decided, and is still the point of it: the frame as text, one line per row —
the row's number, its glyphs between two bars, and the ink of each run of cells named
(`plain`, `dim`, `muted`, `accent`, `ok`, `err`, and `mark` for a cell a reader's
selection covers — the one ink that spends a modifier and no colour), never a colour. The metadata fence at the
top of the file is `insta`'s; the frame is everything below it, and `insta`'s own diff
prints beneath the harness's line naming the first cell that differs.

The filename is the whole of what the deviation costs.
