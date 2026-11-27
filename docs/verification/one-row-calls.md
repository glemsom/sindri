# One-row call verification

Verification for #626 under #620 on `implement/626`, starting at integration commit
`cf4e5d04`. Both issue bodies and comments, repository instructions and domain docs
were read. `git merge-base --is-ancestor integrate/620-one-row-call HEAD` passed
before edits; the branch started at the integration tip.

## Superseded decisions

This record replaces the withdrawn acceptance document for #599 under #593;
that document remains available in git history, not as current product evidence.
#620 reverses #593's bounded invocation/result footprint, compact settled-success
style and the child ticket's Kitty decoration beneath calls. Closed calls now use
one log row (plus a recorded diagnostic on failure); opened calls use one gutter
rule at the band's width. Calls own no background or image placement.
Native text, recorded exits, capability fallbacks, copying and reader-owned Folds
remain constraints. Prompt, Rail, Composer and instrument planes are not retired.
The reversal follows the reader's judgement that decoration did not earn its rows;
the earlier automated evidence had never established manual visual acceptance.

## Automated matrix

The existing frame harness is the principal approved seam: it exposes frame text,
looks, terminal placements and pointer targets. `tool_heads.rs` crosses graphics
present/absent with truecolour, ANSI and no colour at 120×40, 80×24, 40×24 and
12×24. No-colour suppresses planes even when graphics capability is present.
Opened short calls and reader-opened calls exercise all 24 combinations, including
Exchange opening, scrolling, reflow and capability changes. These are assertions
of stated frame facts, not a claim that every cell of every possible transcript
has a literal golden at every capability.

`painted.rs::calls_take_no_picture_placements_across_terminal_changes` checks
120×40, 80×24, 40×24 and 120×48 across pictured truecolour, changed 8×16 cell
pixels, pictured ANSI, pictured no-colour, unpictured ANSI and unpictured
truecolour. It compares transcript text across those transitions, permits only
remaining plane identities, rejects the retired call image-ID range in emitted
protocol bytes and checks empty unchanged redraws and idempotent cleanup.
The six-path tests above additionally cover unpictured no-colour. This evidence
asserts placements and emitted bytes, not terminal-driver memory or perceived
scroll smoothness.

Closed-head command/count priority is asserted at widths 120, 100, 80, 60, 50
and 40. Other tests cover success without invented exit words, absent exits,
unmatched work, failure's last nonempty recorded diagnostic, non-bash names,
withheld byte detail and width-dependent Fold counts. Opened 100,000-row results
remain bounded through scrolling at 120×40, 80×24, 40×24 and 120×48 without
call pictures. Prompt resource bounds, contrast, fallbacks and lifecycle tests
remain separate regression evidence; they do not imply decoration beneath calls.

## Pointer, keyboard and copied source

`call_head_press_toggles_without_moving_the_head_and_body_presses_do_nothing`
asserts opening/closing and anchoring at 80×12, and unchanged outcomes for presses
on command, output and byte-detail rows. `a_laid_call_path_press_leaves_the_fold_to_the_heads_other_columns`
checks a laid path target at 80×24: its press leaves the Fold unchanged, while
another head column opens it. It does not assert an external browser rendered a
page. These pointer sequences are not crossed with every capability and width.

`keyboard_call_fold_survives_resize_without_pointer_reports` checks the existing
Fold key on a no-colour path at 40, 80 and 120 columns. Keys-screen fixtures name
calls alongside work and Prompts. Result opening survives reflow and reopening an
Exchange. Copy tests assert original command whitespace and indentation, Unicode,
result columns and blank lines, and recorded generic JSON arguments rather than
clipped display glyphs; programmatic copy is not native terminal selection.

## Fixtures

`frames.rs` fences `tools`, `calls`, `fold`, `fold-open`, `fold-answer`, `gutter`,
`rail`, `face`, `face-off` and `keys` with recorded text and looks. Most call
fixtures are 120×40; gutter also covers 80×24, and fold/fold-open cover 40×24
and 80×24. Rail fixtures span 40 through 160 columns. The integration work
re-recorded these fixtures; this documentation change does not bless snapshots.
The fixture diffs are automated design evidence, not a reader's visual approval.

## Commands and results

Commands run from the worktree through `devbox run -- bash -c '…'` with the
pinned Rust 1.100.0 toolchain. The withdrawal check failed before replacement:
the old record existed and the new record did not. The same check passed after
replacement.

- `cargo fmt --all -- --check`: passed after formatting the renamed test.
- `cargo check --workspace`: passed.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo test -p sindri --test tool_heads --test painted --test mark --test keys`:
  passed, 116 tests (16 tool-head, 72 painted, 7 mark and 21 keys).
- `cargo test -p sindri --test frames --test tool_heads`: passed, 139 tests
  (123 frame and 16 tool-head), with no ignored tests.
- `git diff --check`: passed.

The initial frame run passed 121 tests and failed two stale `card` overlay
snapshots. Their only changes are the Fold description adding “call” and the
narrow overlay's resulting kept-row count. They were re-recorded through
`INSTA_UPDATE=always cargo test -p sindri --test frames card_`, not hand-edited.
Two legacy test names were updated to name opened calls, not a deleted compact
style. The complete workspace test suite was not run for this record.

## Visual judgement not claimed

No manual visual acceptance was performed. Automated assertions establish the
frames, placements and press outcomes described above, across their stated
matrices and individual paths. They do not establish that the result reads well
in a real terminal, on real fonts, at real cell sizes. That judgement belongs to
the reader. Real-terminal aesthetics, native selection, multiplexer rendering and
perceptual rapid-scroll/resize behaviour remain unverified here. No visual claim
from the withdrawn record is carried forward. No push is part of this work.

## Review fixes: constrained disclosure

Closed heads now share outcome classification with opened heads. Command clipping
measures glyph cells before keeping even the first glyph, including a zero-cell
prefix and a leading double-width glyph. Judged closed paths retain their native
hyperlink underline as well as their destination.

Full `⋯ N rows kept back` wording cannot fit in a 12-column frame's transcript
measure, even with no command, name or duration. Fixed-fact overflow therefore
falls back to disclosure alone, then `⋯ N`, then the decimal count alone, preserving
numeric information before other facts. The existing disclosure glyph is reused;
there was no existing shorter transcript wording to reuse. This does not claim
that full wording, tool identity or exit wording fits at physically impossible
widths. If even the decimal digits exceed the supplied columns, full numeric
information is itself impossible in one native-cell row; the reader must widen
or open the call. No manual visual acceptance is claimed.

Legacy row and PTY assertions now check closed heads, opening calls explicitly
where the behavior under test is full command/output or wrapped path links.
Snapshots for closed-path underlines were regenerated by the frame harness.
Helper inventory tests were moved unchanged to the sibling test module.

Review-fix validation: `devbox run make check` passed every stage, including
workspace tests, all 123 frame fixtures, audit, evals, no-relink and release.
An intermediate version assertion saw stale build-time describe metadata after
unstaged edits; staging refreshed the watched Git index and the unchanged test
passed on the full rerun. No assertion was weakened or skipped for that failure.
