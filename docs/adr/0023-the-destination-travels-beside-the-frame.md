# The destination travels beside the frame, and the backend writes it

A run of a row's cells can be a way to somewhere. `sindri::link::Link` is that run — the columns it
covers, measured from the row's left edge, and the destination it opens — and `Row::linking` is how a
row names one. The renderer's cell holds a glyph and a look and has no room for a destination, so a
frame's ways travel beside it: `View::paint` lays them in `link::Links` as it draws the rows that name
them, `View::links` hands them back, and the backend that writes the frame — crossterm's own, in
`term::Backend` — writes each one around the cells its run covers as the terminal's own hyperlink:
`OSC 8`, no params, the destination, `ST` before the run's first cell, and the same sequence with no
destination after its last.

**A destination is not a cell's, because a cell is what a copy takes.** A frame is a buffer of cells
diffed against the buffer before it, and a cell's glyphs are exactly what a reader's copy of the frame
holds. A destination carried inside a cell's own run would be copied out as words nobody read, and a
destination the frame compared cell by cell would have to be drawn somewhere — which is a column
spent on an address. So the words say nothing about where they go, and the address is nowhere in the
frame at all.

**The frame's ways are the frame's, and the bytes are the backend's.** Nothing in the renderer knows
what a hyperlink is, so the destinations are laid beside the cells the paint placed, at the same seam
the paint itself is: a row names the columns it is a way to as it is drawn, and the frame holds what
it named. Only the backend knows which of those cells are actually written, because a canvas writes
the diff and not the frame — so it is the backend, and never the painter, that decides where one way
ends and the next begins.

**A run names a destination, and its look does not.** A look is an ink and the faces it wears, and one
destination can be said in two looks; a destination kept in a look would be a second spelling of the
same thing, and it would follow that look into every row drawn in it. A run is the narrower claim: the
columns of one row, and what a cell of them opens.

**A run is one row's.** A link is measured from the row's left edge, so a run wider than the band is
each row's own name and no row carries a destination beyond itself. What the backend tracks is where
the destination changes: two runs side by side that name one destination are written as one link, and
a cell naming none closes the one before it.

**A frame closes what it opens.** Every pass ends with the closing sequence, and a way is opened for
the cells the diff carries rather than for the run they were laid as: a cell repainted alone in the
middle of a link is still a way there, and no cell a later pass writes is a way to a run it never
belonged to.

**A terminal that places no hyperlink is handed the words and no sequence.** `link::speaks` reads the
names a terminal gives itself, resolved once when the launch settles what its terminal can do, and
`term::Backend` holds that answer: a terminal nobody can vouch for is handed the same frame and no
bytes but the words.

**A band under a card names nothing.** A screen is drawn over the frame it was opened from and the
band under it takes no key, so a card's paint covers the ways the frame behind it laid: no cell a
reader cannot reach is a way to somewhere.

## What a reader is held to

- A row that names no destination is drawn exactly as it was, and a destination costs the frame no
  column.
- A destination opens before the run's first cell and closes after its last; two runs naming one
  destination side by side are one link, and a run naming none closes the one before it.
- A run is one row's: a run wrapped onto the next row names it there, and no destination crosses a
  row.
- A terminal that places no hyperlink is handed the words and no sequence.
- A frame with a screen open over it is no way to anywhere.
- A destination is written as the terminal's hyperlink and never as glyphs, so a copy of the frame
  takes the words and no address.
