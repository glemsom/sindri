---
status: accepted
date: 2026-10-30
---

# 0016 — A picture is drawn where its rows are

## Decision

ADR-0011 §6 refused images when the answer was words only, and the ground rules already
give the face the Kitty graphics protocol. A markdown image asks the face's question about
a file the model named, so the answer is written down here rather than discovered in a
diff: a picture is drawn where its rows are. The code that carries it follows in issues
#56–#59; this record is written first.

1. **Kitty only, by the frame's one answer.** A picture is drawn where the terminal speaks
   Kitty graphics, through the single `Picker` the frame already queries for the face
   (ADR-0012, issue #56): one terminal, one question, the protocol and the cell size
   shared by both users, a launch that spends the query at most once. No sixel, no
   half-blocks, no 256-colour fallback — the ground rules' posture, unchanged. A terminal
   that announces nothing is not asked, and a picture without an answer is its words
   (§24's link rule).
2. **A picture is a block.** It ends the paragraph it sits in, takes the rows it needs at
   the answer's column width, and is captioned by its alt text with `‹src›` beside it,
   exactly as a link keeps its target (§24): `a chart ‹plot.png›`. A picture whose words
   are absent is its target alone. A picture inside a table cell or a list item is not a
   block: it stays the words it was given.
3. **Local files only.** `src` resolves against the workspace, an absolute path is taken
   as it is, `~` is not expanded, and no scheme is fetched: `https:`, `http:` and `data:`
   sources are refused by name. v1 draws no remote picture. The frame is not a browser,
   and its first answer to a URL is the URL.
4. **Bounded on three sides.** The source is capped in bytes (16 MiB), the decode is
   capped in pixels and refused before it happens — a decompression bomb is one small
   file — and the block is capped in rows at `MAX_ROWS` = 20, the face's own allowance
   (§5). A picture never upscales past the column; one narrower than the column keeps its
   width.
5. **A cache, not a reload.** Rows are measured from the file's header alone
   (`ImageReader::with_guessed_format().into_dimensions()`), never by decoding pixels: the
   frame measures every entry it lays out, and a header read is what that can afford. The
   protocol is built once per `(path, length, mtime, columns, rows)` and at most eight are
   live, because each holds pixels. A redraw of the same answer costs nothing; a file an
   agent rewrote under a new answer is read again.
6. **Whole in the window, or not at all.** A picture partly scrolled out of the transcript
   is not drawn at its edge: cropping is out of scope, and its caption and blank rows say
   where it is. It appears whole once it is whole in the window, and `StatefulProtocol` is
   what makes the repaint of a live run cheap.
7. **Nothing behind an overlay.** An overlay dims cells, and a graphic is not a cell:
   while one is open, pictures are not drawn, and closing it draws them again.
8. **What cannot be drawn says why.** A refusal is a reason, never a panic and never a
   placeholder box: no graphics value, a missing file, a directory, non-image bytes, the
   byte or pixel cap, an unsupported format or a URL each reserve no rows and are
   captioned with the reason in the `‹›` where the path would be
   (`a chart ‹plot.png: no such file›`). The frame says what the model wrote and what it
   could not do about it.
9. **A picture's rows are rows.** The window's anchoring, the new-rows badge and the
   position marker are row-counted and need no change (ADR-0013); a picture reserves its
   rows in the same count a paragraph does, and the copy of a block yields its caption
   once rather than a run of blank rows.

## Consequences

- ADR-0011 §6 is amended by one clause and a pointer here; §13 gains what a multiplexer
  does to a graphic, because whether it reaches the screen is the terminal's business.
- One question per launch, not one per picture: `face.rs` and `picture.rs` borrow the same
  value, and the layout tests build it from a made-up cell size (`Face::kitty()`), so a
  picture is asserted on its rect and never on its pixels.
- Nothing in the layout depends on a picture being drawn: a terminal without Kitty, a
  missing file and a refused format all cost the same rows as the caption, so a frame's
  layout is not a function of what the terminal can do.
- The decoders come with `ratatui-image`'s `image-defaults`; PNG, JPEG, GIF (first frame)
  and WebP are what it links, and a format it cannot read is a captioned reason.
