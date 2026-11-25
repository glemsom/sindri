# Tool surfaces acceptance — #599 / #593

## Assembly

Verified on `ticket/593-599`, starting at `f3f15aaa`, the integration merge of
#598. The implementation tips for #594–#598 are all ancestors of this start.
No parent or child issues were edited.

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
The existing 60,000-row opened-result test decodes transmitted PNGs and checks
bounded windows across scrolling and resizing. These are protocol/raster bounds,
not measured peak RSS or text-reflow bounds. Not every mixed-entry variant is
combined into every cross-feature sequence.

## Commands and results

Rust/cargo 1.99.0 from the available Nix profile; writable Cargo cache at
`/home/gso/.cache/sindri-595-cargo`. Package minimum is 1.100, so compilation
commands explicitly used `--ignore-rust-version`.

- `cargo test --workspace --all-targets --ignore-rust-version`: passed,
  2,132 tests passed, 5 ignored by the existing suite.
- `cargo check --workspace --all-targets --ignore-rust-version`: passed.
- `cargo fmt --all -- --check`: passed.
- `cargo clippy --workspace --all-targets --ignore-rust-version`: passed with
  one remaining `too_many_arguments` warning in `plane.rs:781` (`tool`, 9/7).
- Strict Clippy (`-- -D warnings`) failed on that assembled-feature lint and
  an unnecessary lazy Option fallback; the latter is now fixed. The argument
  count remains disclosed rather than suppressed or refactored in acceptance.
- `git diff --check`: passed.

Initial compilation was blocked by the default read-only Cargo cache; the writable
cache resolved it. One later workspace test run failed only because the cached
binary version lacked Git's new `-dirty` suffix. Touching the existing build script
forced regeneration; the complete workspace suite then passed without skipping it.
The #598 fixture's unused-function warnings were removed by scoping `dead_code`
allowance to its import in the frame test binary, matching shared harness practice.

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
