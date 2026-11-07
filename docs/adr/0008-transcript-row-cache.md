# The transcript caches rows per entry, keyed by width

Each `Entry` renders to its own `Vec<Row>`, cached beside a copy of the width it was laid out at.
Heights therefore come free, tail-follow is arithmetic, and a streaming tick re-renders exactly
one entry instead of the history. A width change invalidates the cache and reflows once.

Three rules come with it:

- **A fence that is still streaming is not rasterised.** It renders as a code block until its
  closing fence arrives, so a half-written mermaid diagram never costs a raster pass and never
  flickers.
- **A picture is cached by `(source, cols, rows)`**, so scrolling past a diagram cannot
  re-render it.
- **A resize re-rasterises only what is visible.** Off-screen pictures keep their old raster
  until they are on screen again.
