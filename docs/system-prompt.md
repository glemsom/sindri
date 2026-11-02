# The system prompt, measured

The prompt in `crates/core/src/provider/rig_wire.rs` is re-sent on every turn of
every run, so it is a tax the run pays per request and a switch that decides how
the model works. Both are measurable, and neither is a matter of taste: a prompt
that makes the model search before it opens a file spends a fraction of what one
that says nothing spends, on the same tasks and with the same answers.

So: thirteen prompts were run against four tasks — two readings, two edits — on
three models, and every run was scored. The machine is in
`scripts/bench-system-prompt`; the numbers below came out of it.

## What is counted

*Input tokens, summed over the turns of a run.* Not the length of the prompt —
that is only one term. A run that reads a 760-line file into the conversation
pays for those lines again on every later turn, so the sum is what the work
actually cost, and the prompt's advice shows up in it or nowhere.

*Right or wrong per task, by a rule, not by eye.* T1 asks for every line that
mentions a symbol, `path:line`, plus a value from the function; T2 asks what a
760-line module does with an unknown config key, quoting the line; T3 adds a
`--dry-run` flag until the fixture's test suite is green; T4 renames a helper
everywhere it is used — with `bucket_keys`, a longer name the old one is a
prefix of, in the tree as a trap.

One run per task per repetition, each in a fresh copy of the fixture repo. The
variants differ only in the prompt.

## The variants

| variant | what it adds to the shipped prompt | words |
| --- | --- | --- |
| v00 | nothing — the prompt as it was | 94 |
| v01 | the mermaid sentence cut, the rest tightened | 59 |
| v02 | "Read the fewest bytes that answer: search first (`rg -n`), then open only the lines you need (`sed -n 'A,Bp'`); never `cat` a whole file, and never run a search or a listing twice." | 93 |
| v03 | "Orient once: one command that lists and searches is enough. Never repeat a lookup you have already run." | 77 |
| v04 | "Change as few bytes as the change needs: read only the lines you will touch, patch them in place, and never rewrite a file to alter a few lines." | 96 |
| v05 | "Prove a change with the narrowest command that can fail — the one test, the one invocation — not the whole suite twice." | 82 |
| v06 | "Answer in the shortest form that carries it: no echoed file contents, and no code the user did not ask for." | 80 |
| v07 | "Do several steps in one turn: a search, the read it implies and the edit it justifies belong in one script." | 89 |
| v08 | "A result you have read is evidence: do not re-read a file, re-run a test or re-search a pattern to confirm what it already said." | 84 |
| v09 | the whole prompt rewritten short: search, open ranges, change little, prove narrowly | 58 |
| v10 | v01 plus a worked example of a good read and a good edit | 97 |
| v11 | v02, plus the minimal patch of v04 and the narrow proof of v05 | 121 |
| v13 | v02, plus "A fence marked `mermaid` is drawn as a picture: draw what prose cannot." — this one landed | 106 |
| v14 | v13, plus "Put the steps of one task in one script: filter there, print only what you concluded, and keep intermediates in `$TMPDIR`, which outlives the call." — shipped unmeasured, see below | 131 |

## The ranking, `deepseek-v4.1-flash`, two repetitions each

Mean input tokens per task; the sum is one complete pass of all four.

| prompt | T1 | T2 | T3 | T4 | total | turns | right |
| --- | --- | --- | --- | --- | --- | --- | --- |
| v00 | 10577 | 8120 | 36175 | 67012 | 121885 | 6.2 | 8/8 |
| v01 | 4876 | 7858 | 36424 | 84770 | 133928 | 6.4 | 8/8 |
| v02 | 3911 | 7512 | 18588 | 45146 | 75156 | 6.1 | 8/8 |
| v03 | 3876 | 4584 | 29800 | 55944 | 94206 | 5.6 | 8/8 |
| v04 | 3964 | 5794 | 38860 | 55964 | 104582 | 5.2 | 8/8 |
| v05 | 4903 | 7682 | 29324 | 49104 | 91014 | 6.5 | 8/8 |
| v06 | 4172 | 3206 | 29148 | 49894 | 86420 | 5.1 | 8/8 |
| v07 | 4098 | 3536 | 31522 | 35648 | 74804 | 5.4 | 8/8 |
| v08 | 3872 | 5628 | 43094 | 65000 | 117593 | 5.4 | 7/8 |
| v09 | 5064 | 8219 | 31212 | 56107 | 100602 | 6.2 | 8/8 |
| v10 | 4442 | 9584 | 26558 | 84762 | 125346 | 6.8 | 8/8 |
| v11 | 5819 | 5856 | 16270 | 40622 | 68566 | 6.2 | 8/8 |
| v12 | 4018 | 6136 | 14630 | 30461 | 55246 | 5.9 | 8/8 |

## The four that mattered, on two models

Median tokens for one pass of all four tasks, over six repetitions (four for
v13), so that a single lucky run cannot carry the claim.

| prompt | deepseek-v4.1-flash | vs v00 | claude-haiku-5-5 | vs v00 |
| --- | --- | --- | --- | --- |
| v00 | 132564 | — | 69135 | — |
| v02 | 65569 | 0.49× | 28158 | 0.41× |
| v11 | 54316 | 0.41× | 36450 | 0.53× |
| v13 | 55486 | 0.42× | 32818 | 0.47× |

Right, over the same runs: 23/24 for v00 on deepseek (two over-eager rename
edits), 22/24 for v11, 24/24 for v02 and v13; 24/24 and 16/16 for all four on
haiku. The cheap prompts are not cheaper because they answer less.

## What the numbers say

- **The lever is the reading instruction, not the prompt's length.** v02 is 93
  words against the shipped 94 and spends half the tokens; v10, the longest,
  spends more than the baseline. What changes is the shape of the first command:
  the baseline opens `wc -l src/ledger.py; grep -n "" src/ledger.py` — the whole
  file, with line numbers — where v13 runs `rg -n normalise_config` and then
  `sed -n '366,400p'`. On T2, the median run falls from 18962 tokens to 5440
  on haiku.
- **Telling it what not to repeat is most of the rest.** v03 and v08 are the same
  idea in weaker words, and both cut turns; v08 on its own also loosened the
  rename enough to cost an answer, which is why the sentence landed inside v02's
  rather than beside it.
- **A prompt does not buy precision.** The rename trap caught `glm-5.3-flash`
  in a quarter to a third of its runs under every variant, v12's "match a name
  whole" included. A weak model over-replaces the longer name whether or not it
  was warned; that is a task for a test, not for prose.
- **Editing advice is not free.** v04's minimal patch and v05's narrow proof each
  cost more than they saved alone — a model told to work in small pieces takes
  more turns to do it — and only earned their place inside v02's reading rule.
- **The mermaid sentence costs nothing measurable.** No task here asks for a
  diagram, so what is measured is only that cutting it changed no answer. It was
  kept, compressed from 31 words to 13, because the frame draws those fences and
  a diagram nobody emits is no feature.

## What landed

`PREAMBLE` keeps the shipped sentences and gains the reading rule and the
shortened diagram sentence:

> Read the fewest bytes that answer: search first (`rg -n`), then open only the
> lines you need (`sed -n 'A,Bp'`); never `cat` a whole file, and never run a
> search or a listing twice. A fence marked `mermaid` is drawn as a picture:
> draw what prose cannot.

`make check` is unchanged and green under it: the offline evals replay their
fixtures and assert the same eight things, because the prompt is not the answer.

## The batching sentence, shipped before it was measured

`PREAMBLE` now also carries v14's sentence. It was written from v07 — *"Do several
steps in one turn"*, 74,804 tokens against v00's 121,885 with no answer lost — and
compressed, because the same doctrine is already stated once in the `bash` tool's
description and a second full sentence in the prompt would be the same advice paid for
twice per request. The two words v07 did not need are here instead: *filter there*, and
`$TMPDIR`, which names the one channel a result can cross the turn in without entering
the conversation.

It is in `scripts/bench-system-prompt/variants.py` as `v14` and **has not been run**.
The claim it rests on is v07's, not its own: a prompt that puts the steps of a task in
one script buys turns, and a prompt that says what a result costs buys bytes. Run it
before believing the second half.

## What this does not prove

Two model families, two readings, two edits, one fixture repo of 862 lines — a
bench that measures a prompt's shape, not a benchmark of coding ability. The
token sums are the provider's own accounting, and the six repetitions put the
spread of one prompt's bill at roughly ±20%, which is why the claims above are
ratios of two-to-one rather than percentages. And a prompt written to make a
model read less can make it read too little: T1 and T2 were chosen to catch that
and did not, but a task whose answer lives in the file's last thousand lines
would test it harder than anything here.

To reproduce, see `scripts/bench-system-prompt/README.md`.
