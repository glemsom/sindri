# Tool surfaces acceptance — #599 / #593

## Assembly

Verified on `ticket/593-review`; `integration/593-tool-surfaces` is an ancestor
of HEAD (`git merge-base --is-ancestor` passed). Both supplied reviews and the
#593 spec were read. No parent or child issues were edited.

## Automated evidence

The existing confirmed frame-rendering and emitted Kitty-protocol seams were used.
Added cross-feature coverage for reader-opened compact results, Exchange opening,
scrolling, resize, capability changes and exact Unicode/blank-line/indented copied
source. Added a dense 60-invocation transcript mixing success, failure, no process
exit and live unmatched work. Both exercise all six capability paths at 120×40,
80×24, 40×24 and 12×24. Dense scrolling checks tool placement containment, decoded
PNG raster bounds, empty unchanged passes and idempotent cleanup; no-colour paths
emit no planes. An initial containment assertion incorrectly included the Composer
plane; narrowing it to tool planes made the acceptance test pass without a product
change. The source-preservation test passed on first execution.

Existing regressions additionally cover multiline commands, standalone results,
empty output, failure diagnostics, required narrow-width head facts, byte detail,
pointer and keyboard Fold targets, remeasured counts, Exchange Fold persistence,
compact/full pixels, Prompt/Answer rendering, contrast, actual-end clipping,
placement-only motion, replacement/deletion and capability/reset cleanup.
The existing 100,000-row opened-result test decodes transmitted PNGs and checks
bounded windows across scrolling and resizing. These are protocol/raster bounds,
not measured peak RSS or text-reflow bounds. Not every mixed-entry variant is
combined into every cross-feature sequence.

## Commands and results

Rust/cargo 1.99.0 from the available Nix profile; writable Cargo cache at
`/home/gso/.cache/sindri-595-cargo`. Package minimum is 1.100, so compilation
commands explicitly used `--ignore-rust-version`.

- `cargo test --workspace --all-targets --ignore-rust-version`: passed,
  2,134 tests passed, 5 ignored by the existing suite.
- `cargo check --workspace --all-targets --ignore-rust-version`: passed.
- `cargo fmt --all -- --check`: passed.
- `cargo clippy --workspace --all-targets --ignore-rust-version -- -D warnings`:
  passed without warnings. Named `SurfaceWindow` and `ToolStyle` parameter groups
  replace the argument-count warning; no lint suppression was added.
- `git diff --check`: passed.

Review fixes preserve recorded generic invocation arguments when copying, hide
byte detail under closed result Folds, and retain complete footprints and skips
above 65,535 rows. New frame regressions cover generic JSON copying and closed/open
byte disclosure. The opened-result raster regression now uses 100,000 rows and
requires visible tool placements while checking bounded decoded images through
scroll and resize. Byte disclosure and large-result regressions failed before
their fixes; the first generic-copy test invocation timed out during compilation,
so a completed red run for that regression is not claimed.

Outcome styling now uses explicit recorded state rather than rendered ink/glyphs.
Source sentinels and entry classifications are named centrally; bounded planes
carry explicit owning identities. Existing Fold-count assertions and two reviewed
frame snapshots reflect withheld detail rows. Keyboard mark tests grow upward
from the Fold, now the final closed row. The clipping test explicitly returns to
the head before checking placement-only motion.

Initial full-suite runs exposed obsolete Fold expectations and snapshots; these
were updated, and the final complete suite passed without bypassing tests.

## Manual acceptance and delivery limitations

No interactive Kitty-compatible terminal review was performed. Actual terminal,
font and displayed capabilities were not established. Manual review of twenty or
more invocations, Answer prominence, quiet rims/highlights, rapid scroll/resize,
text-only/no-colour aesthetics, native terminal selection/copy and a supported
multiplexer path remains unavailable and outstanding. Automated cells, programmatic
copy, protocol bytes and decoded images are not terminal aesthetic verification.

Changes remain uncommitted: Git metadata and the checked-out integration worktree
are outside this session's writable boundary. No integration merge was attempted.
Manual visual acceptance is not claimed complete.
