# Mermaid renders in-process; no JavaScript, no browser

A mermaid fence in an answer becomes a picture via `mermaid-rs-renderer` (MIT, pure Rust,
23 diagram types, SVG out), then `usvg` → `tiny-skia` → `resvg` → `image` → `ratatui-image`
Kitty placeholders. No JS engine, no headless Chromium, no `mmdc` dependency.

**Why not the faithful route.** Mermaid's own layout calls `getBBox()`,
`getComputedTextLength()` and `getBoundingClientRect()`; a stub DOM cannot answer them, so
every "embed mermaid.js" design ends up shipping a browser. `mermaid-cli` declares
`puppeteer` for exactly this reason. A binary that cannot assume `node` exists cannot go
that way. The pure-Rust ports are rewrites, so their failure mode is *wrong pictures*, not
errors — which the spec already legislates for: a picture is "refused with a reason the
caption can print", and the fence's source is the honest fallback.

**Consequences.**

- A font must be embedded. `usvg` ships an empty `fontdb` and converts `<text>` to paths at
  parse time, so without one every diagram label silently disappears. One OFL sans-serif
  (~400 KB) is loaded via `fontdb::Database::load_font_data`.
- Measurement costs no pixels: `usvg::Tree::size()` plus the cell pixel size gives the
  cell footprint before anything is rasterised, which is what the spec requires.
- `Picker::from_query_stdio()` reads the tty for `CSI 14 t`/`CSI 16 t`. Only the TUI may
  probe it, once, before its event loop; batch mode never probes.
- A corpus of real fences compared as golden SVGs is the eval that keeps fidelity honest.
