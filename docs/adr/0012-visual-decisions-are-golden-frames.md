# A visual decision is frozen as a golden frame, not described in prose

The TUI's look is the one part of the spec no ordinary test can reach: "reads as a ledger",
"degrades deliberately", "carries elapsed time" are adjectives, and an adjective cannot be
closed — least of all by an agent, which can build anything it can grade.

So every visual decision becomes an artefact. A frame renders through ratatui's headless
`TestBackend` at a fixed size into `tests/fixtures/<frame>-<cols>x<rows>.snap`, and `insta`
compares it. Colour is asserted as an **`Ink` name** (`dim`, `muted`, `accent`, `ok`, `err`),
never as RGB: a palette change must not break a fixture, and a fixture must not smuggle in a
theme. What the file holds is the frame as text, one line per row — the row's number, its glyphs
between two bars, and the ink of each run of cells named (`plain`, `dim`, `muted`, `accent`, `ok`,
`err`, and `mark` for a cell a reader's selection covers), never a colour.

What this buys, and what it costs: a human approves a fixture **once** instead of reviewing
renders on every change, and a visual ticket moves from `ready-for-human` to `ready-for-agent` —
the review becomes the approval of a golden, not the writing of code.

The rule that keeps it from ossifying: changing a number in a ticket's contract means changing
the golden in the same commit, deliberately. A fixture updated to match whatever the code now
does is not a test, it is a receipt.
