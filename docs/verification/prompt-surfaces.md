# Prompt surface verification

Verification for glemsom/sindri#592 under #588, on `feat/588-prompt-surfaces`, following
`dfc64003`, `e1907afd` and `60b5278b`. Both ticket bodies and their comments were fetched
with `gh issue view --repo glemsom/sindri --json body,comments`; neither returned comments.

## Automated matrix

The existing frame harness covers six capability paths: graphics present/absent crossed
with truecolour, ANSI and no colour. No-colour with graphics capability deliberately emits
no planes. Literal-row equality covers all six paths at 80×24, 120×40, 40×24 and 12×24:
24 combinations, with multiline paste, blank lines, wide Unicode, literal fences and two
Exchanges. Short Prompt/Answer alignment, ownership gutters and row density are asserted.

The additional Fold/copy matrix covers six paths at 80×24, 120×40 and 40×24 (18 combinations).
It checks original copied words, 12/28 hidden-row counts, opening, persistence through
narrow reflow and closing. Existing pointer tests supplement keyboard Fold checks.

Existing lifecycle tests cover first transmission, unchanged redraw, placement-only movement,
partial clipping with original corners, Fold replacement, width/height resize, changed cell
pixels, capability/palette fallback, draft edits, reset and terminal cleanup/order.

Long-Conversation verification runs 100 and 1,000 Exchanges through 80 page/resize passes each,
cycling 80×24, 120×40 and 40×24. A model built only from emitted `a=T`, `a=p`, `a=d`
operations checks releases precede replacement, placements refer to live images, and live
resources equal visible placements, bounded by band rows plus four band decorations.
Uploads never exceed current placements; every unchanged redraw emits zero bytes. Cleanup
releases exactly the observable live images. No cache collection or creation counter is read,
no testing API added, and no timing threshold imposed. This proves protocol resource and
upload bounds, not terminal-driver memory usage or perceptual smoothness.

All palette inks meet the existing 4.2:1 floor on flat surfaces via palette tests. Frame-level
pixel checks cover truecolour and ANSI Prompt, Answer-band and Rail planes, including all
Prompt edge/light pixels, and target 4.5:1 for ordinary Prompt ink. Decorative Seam cells
are excluded: they carry no ordinary prose. Existing Rail/Answer snapshots and tests pass.
No production defect was found. Initial test failures were corrected test assumptions:
non-pointer paths use keyboard Fold; decorative Seams are not text-bearing surfaces.

## Commands and results

Commands run through `devbox run -- bash -c '…'`, providing Rust 1.100.0:

- `cargo check --workspace`: passed repeatedly.
- `cargo test -p sindri --test painted prompt`: baseline 13 passed; expanded Fold matrix passed.
- `cargo test -p sindri --test painted every_ink`: 1 passed after correcting Seam coverage.
- `cargo fmt --all` and `cargo fmt --all -- --check`: passed.
- `cargo test -p sindri`: all surface/frame and terminal suites passed, including 55 painted
  tests and 77 TUI integration tests; two baseline tests are ignored. First broad run hit
  an unrelated stale version binary: clean-built `60b5278b` versus current `60b5278b-dirty`.
  Post-commit clean-checkout rerun passed: 1,680 tests passed, zero failed, two ignored,
  including both version tests. Typechecking and formatting also passed.

## Manual availability and acceptance limits

No manual visual terminal pass was performed, including no text-only or multiplexer pass.
`tty` returned `not a tty`; stdin resolves to `/dev/null`, stdout to a pipe. `/dev/tty`
can be opened and `stty -a` reports 209×54, but this tool interface does not expose its
rendered screen or interactive visual observation. Environment advertises
`TERM=xterm-ghostty`, `COLORTERM=truecolor`, `DISPLAY=:0`, `WAYLAND_DISPLAY=wayland-0`.
`ghostty` and `tmux` executables exist; `ps -eo pid,comm,args` found no Ghostty/tmux process.
These facts do not prove accessible graphics placement or a viewable text terminal.
No screenshot/image observation tool is available in this agent interface. Spawning a
terminal or allocating a synthetic PTY would not constitute manual visual verification;
no nested verifier was delegated because it would inherit that observation limitation.

Outstanding unavailable coverage: actual terminal aesthetics, rapid-scroll perceptual jumps,
real resize/display-scale alignment, native selection and a supported multiplexer rendering
path. Automated cells/pixels/protocol assertions are not claims that those manual checks ran.
Under #592's explicit requirement to record unavailable coverage, the issue is closable with
these limitations disclosed, if the owner accepts environment-unavailable manual coverage.
No ticket was closed, pushed or merged by this verification.

## P1 bounded-raster correction

Independent review found the earlier resource-count evidence insufficient: an opened
Prompt still rasterized its full height before placement clipping. The new 60,000-row
regression first failed on a 3,552,059,200-byte allocation attempt. The test executable
ran with a 1,500,000-KiB virtual-memory limit (no unrestricted multi-GB red run).
The initial exact-name invocation selected zero tests; the corrected invocation selected
one and aborted at allocation, before any image upload.

Prompt images now cover a quantized window of at most three Transcript-band heights,
limited by the remaining actual Prompt rows. Full-footprint coordinates determine corners,
rim, light, shadow and dithering; window edges do not become surface ends. Crop coordinates
are relative to this bounded image. Motion inside a window is placement-only; crossing a
window boundary replaces the bounded image under the existing owning identity, releasing
old data before uploading replacement. Small complete surfaces retain their prior reuse.
Height resize can change a large surface's window; unchanged redraw sends no bytes.

For Prompt width W, band height B and cell pixels cw×ch, one raster is bounded by
4×W×cw×3×B×ch bytes, and visits at most W×cw×3×B×ch pixels. At 120 columns,
40 band rows and 10×20 pixels this is at most 11,520,000 RGBA bytes, not 5.76 GB.
PNG encoding currently clones RGBA and holds encoded/base64/protocol buffers as well:
these remain proportional to the bounded raster, not to full Prompt height. This is
not a measured peak-RSS claim or a bound on transcript text/reflow work.

The regression uses existing frame and terminal-output seams, no cache counters or new
API. Sixty scroll/page/resize passes span 120×40, 80×24, 40×24 and 120×48. It checks
placement containment and frame image bounds, assembles emitted Kitty payload chunks,
decodes their PNGs and checks transmitted dimensions and payload bounds. Every unchanged
redraw emits zero bytes. Existing actual-end corner, clipping, placement-reuse, identity,
transition and all-ink contrast regressions remain passing.

`cargo check --workspace` and formatting checks passed. All 56 painted tests passed.
The first complete package run reached 1,680 passed, two ignored and one unrelated
version failure: the stale binary reported clean `0249a951` while git reported dirty.
A clean post-commit run is recorded below. Manual availability limitations above remain;
no visual terminal, multiplexer or peak-memory measurement was performed.
