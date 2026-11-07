# Mermaid renders in-process; no JavaScript, no browser

A mermaid fence in an answer becomes a picture via `mermaid-rs-renderer` (MIT, pure Rust, SVG
out), then `usvg` → `tiny-skia` → `resvg` → `image` → `ratatui-image` Kitty placeholders. No JS
engine, no headless Chromium, no `mmdc` dependency.

**Why not the faithful route.** Mermaid's own layout calls `getBBox()`, `getComputedTextLength()`
and `getBoundingClientRect()`; a stub DOM cannot answer them, so every "embed mermaid.js" design
ends up shipping a browser. `mermaid-cli` declares `puppeteer` for exactly this reason, and a
binary that cannot assume `node` exists cannot go that way. The pure-Rust ports are rewrites, so
their failure mode is *wrong pictures*, not errors — which the spec already legislates for: a
picture is refused with a reason the caption can print, and the fence's source is the honest
fallback.

Consequences:

- A font is embedded. `usvg` ships an empty `fontdb` and converts `<text>` to paths at parse
  time, so without one every diagram label silently disappears. One OFL sans-serif is loaded
  through `fontdb::Database::load_font_data`.
- Measurement costs no pixels: `usvg::Tree::size()` plus the cell pixel size gives the cell
  footprint before anything is rasterised, which is what §12 requires.
- The protocol is decided from what the terminal calls itself, not queried: `TERM` or
  `TERM_PROGRAM` naming a terminal that speaks Kitty (`kitty`, `ghostty`, `wezterm`), and no
  picture at all for one that names none. The launch asks its terminal exactly once —
  `tcgetwinsize`, then `CSI 16 t` — and hands the frame the cell size it learned. A tty read is
  the thing a filter cannot do, and a second round trip would be a second wait before the first
  frame for a fact that decides only whether a picture is drawn.
- What a terminal that lies costs is the words under the placement: a terminal that names Kitty
  and cannot place a picture shows the protocol's placeholder glyphs in the cells the words were
  written in. What stays whole is the block — its rows are its words whatever is drawn on them,
  so a copy of the block is a copy of its words (§12).
- A corpus of real fences compared as computed SVGs is the eval that keeps fidelity honest.
