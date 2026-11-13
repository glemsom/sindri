# A heading is drawn large, and the row is measured by the cells it leaves

Where the terminal draws text at more than one cell, the answer's two highest headings are drawn at
twice the cell, so a long answer has a shape before it is read. `sindri::size` is that claim: `Sized`
is one run of a row drawn at a scale — the columns it covers on its row, the words it is drawn as, and
the ink they are read in — and `Sizes` is the frame's runs, laid on the rows the paint drew them on,
the way `link::Links` carries the frame's destinations.

**The size is not a cell's, and the row is measured by what it leaves.** A cell is a glyph and a look,
and a heading drawn at twice the cell takes two of each — the footprint — so `Row` keeps the words it
says in its spans and the runs beside them, and `Row::width` is the footprint and not the glyphs. That
is the whole reason a large heading never crosses the band's edge: the row was wrapped at half the
columns before anything was drawn, so no cell of it is cut by the paint. `Row::glyphs` — what a reader
copies — is still the words, because a copy takes the words and never the cells between them.

**A block is two rows, and the row under it is the block's own.** A glyph at twice the cell is drawn
over the row under it as well, so that row is written with the block's gutter and nothing else: a
heading in a quote keeps its bar down both halves, and no cell of the second row is the frame's. That
is also what makes one wrapped line one large row and not two lines' worth of cells.

**The sequence is written by the backend, where the frame's bytes are.** `View::paint` lays the runs
beside the cells it drew and `Sizes::settle` writes the footprint into the buffer once the band is
known — a run with no row of that band under it is the words it says, the same way a picture with no
room for its placement is. `term::Backend` then writes each run as `OSC 66` around the words, in the
ink of the cell the diff handed over, and writes no cell of it: the terminal draws the block, and a
cell under it is the block's. A run is written when any of its cells is in the diff, because a
heading arriving a fragment at a time is not a cell changing — it is the run being redrawn.

**A frame with a screen over it draws no heading large.** The scrim is read as the words it holds, so
no run of it is settled and no sequence is written: a heading the reader cannot reach is not drawn as
a surface they could.

**Which terminals size text is read from the names they give themselves.** `size::speaks` asks `TERM`
and `TERM_PROGRAM` once, at boot, the way `link::speaks` and `window::speaks` do — a query is a read
of the input a filter does not make, and a round trip this launch has already spent on the cell. Only
the terminals known to speak the sizing protocol are handed it, because a heading spaced out by a
terminal that reads no sizing is worse than a small one: **a terminal that draws no large text is
handed exactly the heading it is handed today**, and the goldens of every frame it draws are untouched.

## What a reader is held to

- A terminal that draws no large text is handed the heading it is handed today, cell for cell.
- The two highest headings are drawn at twice the cell and lower levels are not, and neither is the
  prose around them.
- A heading is measured by the cells the sizing leaves: one wider than the band is wrapped, and a word
  too wide for a row is cut at the band's edge and never by a column.
- A large row keeps one line to one large row, the row under it keeps the gutter the block stands in,
  and the rule under the two highest levels is as wide as the cells their words leave.
- The terminal is asked whether it sizes text once, at boot; a terminal that says nothing, and a frame
  with a screen over it, are handed the spelling they had.
- A copy of the frame takes the heading's words and never the cells the sizing left between them.
