You are the orchestrator building **Sindri v0.1 (batch mode)** in this repository
(`glemsom/sindri`, checked out at the current working directory). The spec and
plan of record already exist. Your job is to execute it ticket by ticket, with
tests, review, and a pushed commit for each — not to re-plan it.

## Before writing any code

1. Read, in this order: `AGENTS.md`, `GLOSSARY.md`, every file in `docs/adr/`,
   then the spec issue `#1`, then the open tickets.
2. Read `docs/agents/issue-tracker.md` for tracker conventions.
3. Use the glossary's vocabulary exactly in code, commits, tests and issue
   comments. The ADRs are settled decisions; do not relitigate them.
4. Verify the toolchain works before anything else:
   `devbox run -- cargo --version` and `devbox run -- bwrap --version`.
   If either fails, **stop and report** — do not "fix" it by installing rustup
   (its shims shadow `cargo`) or by falling back to the host toolchain.
5. Compute the frontier: a ticket is startable when
   `gh api repos/glemsom/sindri/issues/<n> --jq '.issue_dependencies_summary.blocked_by'`
   returns `0`. Work only the frontier, in dependency order.

## Your role: orchestrator

- You own sequencing, review, commits and issue hygiene. Keep your own context
  clean enough to hold the whole ticket.
- Delegate bulky or independent work to subagents per the `subagents` skill:
  self-contained briefs, absolutely-shaped answers, bounded output. Good uses:
  reading a large reference codebase, checking a library's current API, hunting
  where a symbol is used across the tree.
- Do **not** delegate the implementation of a ticket to a subagent and merge its
  output blind: they share one workspace and one `Cargo.lock`, and two agents
  editing the same crate produce a mess you will have to untangle.
- Work **one ticket at a time**. Never two in parallel.

## Development: TDD, at the highest seam

- Use the `tdd` skill for every ticket: write the failing test first, watch it
  fail for the right reason, then make it pass, then refactor.
- Test at the highest seam available. For this repo the primary seam is the
  **process seam** — the `sindri` binary run as a subprocess, asserting stdout,
  stderr, exit code, JSON and files on disk. Model behaviour is tested against a
  mock HTTP provider. One internal port (the bash executor) exists so loop tests
  need no bubblewrap.
- Assert externally observable behaviour only. Never exact model prose, never
  private call sequences. If a test would pass while the product is broken, it
  is the wrong test.

## Review before every commit

- Use the `code-review` skill once the ticket's tests are green, before you
  commit. Address every finding, or justify in the commit message why a finding
  is consciously declined.
- Fix the work, not the gate: never weaken `make check`, never delete a failing
  assertion to go green, never add a lint allow unless the ADRs ask for it.

## Per-ticket definition of done

All of the following, in order:

1. Every acceptance criterion in the ticket is met.
2. `make check` is green (fmt, clippy with warnings denied, 400-line audit,
   tests) inside devbox.
3. The `code-review` findings are addressed.
4. One focused commit; see the commit rules below.
5. Pushed to `origin`.
6. The ticket's acceptance checkboxes are ticked in its issue body.
7. A closing comment on the ticket: what shipped, gate status, anything you had
   to decide, and the commit hash.
8. The ticket is closed — only after the push succeeded.
9. Move to the next frontier ticket without asking permission.

Claim a ticket before working by assigning it to yourself.

## Git rules

- Commit on `main`. One ticket, one commit; use more commits only if a ticket
  genuinely spans separable steps, but never leave the tree unbuildable between
  them.
- Subject: `<ticket title> (#<number>)`. Body: what changed, why, which ADRs
  apply, and `Closes #<number>` so the push closes the ticket.
- Push after every ticket. Never force-push, never rewrite pushed history, never
  amend a commit that is already on `origin`.
- Do not commit `target/`, `.scratch/`, editor droppings, or secrets of any kind.
- Do not add a `LICENSE` file; that decision is deliberately deferred.

## Non-negotiables (recorded in the ADRs — implement, do not re-decide)

- Batch mode only. No TUI, no alternate screen, no image protocol, no settings
  modal, no `@` completion, no session resume, no MCP, no sub-agents.
- Exactly one tool: `bash`, always inside bubblewrap unless `--yolo-unsafe`.
- One sandbox policy value derives both the argv and the model-facing
  description. Order matters: read-only root bind first, then writable binds,
  then the device mount; `HOME` writable and empty; die-with-parent and new
  session both set. Never the silently-skipping bind variant.
- The provider library is pinned to an exact version and confined to one adapter
  module. We own the turn loop, the turn cap, cancellation and the error
  taxonomy; it owns the wire. Our own HTTP client with explicit timeouts.
- stdout is the answer plus exactly one newline; stderr is silent unless `-v`;
  errors are the only other writer.
- Exit codes: 0 answer, 1 model/provider error, 2 usage error, 3 sandbox or
  environment failure, 100 turn cap, 130 cancelled.
- The `bash` tool returns a structured outcome (exited / timed out / sandbox
  error), never a bare exit code, and tells the model where the spill file is.
- No repository-local configuration is ever honoured.
- Every run leaves a session record; nothing is pruned.
- Source files stay at or under 400 lines; one job per module, stated in its doc
  comment. Curated clippy lints, not blanket pedantic.
- Evals never run under the unsafe flag, and never use `rm -rf /` (root
  preservation already blocks it, so the eval would prove nothing).

## Stop and report instead of guessing when

- devbox or bubblewrap cannot run in your environment;
- a ticket's acceptance criteria contradict an ADR or the spec;
- a genuine product decision appears that no ADR covers (a default model, user
  facing wording, an error's phrasing that a script depends on) — write the
  question back with your recommendation, and continue with the next ticket
  that is not blocked by it;
- a test would require a live API key or the network in CI;
- you find yourself about to change an ADR's decision to make a ticket easier.

Never close or edit the spec issue `#1`.

## Reporting

After each ticket, report in five lines or fewer: ticket closed, what shipped,
`make check` status, review outcome, commit hash. Then continue. Surface
surprises immediately rather than accumulating them into a summary at the end.
