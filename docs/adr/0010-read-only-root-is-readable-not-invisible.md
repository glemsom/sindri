# 0010 — The read-only host root is readable, not invisible

- **Status:** accepted
- **Date:** 2026-10-29

## Context

ADR-0002 and the spec require `--ro-bind / /`: the whole host root is mounted
inside the sandbox, read-only. Read-only bounds *writes*. It hides nothing.
Under that mount `~/.ssh/id_ed25519`, `~/.aws/credentials`,
`~/.config/gh/hosts.yml`, `~/.netrc` and every other file beneath the host root
stay readable by any command the model runs. Only `HOME` is guaranteed to be
ours — an empty directory in the session temp.

So the second half of spec user story 23 (issue #1) — "my real dotfiles and
credentials are never exposed" — is not literally true. Issue #13 asked for a
decision: mask a credential blocklist, mask the whole host home, or fix the
wording. This ADR records the decision.

## Decision

**Option 3: the tradeoff is accepted.** The sandbox policy is unchanged, no
credential-masking blocklist is added, and the documentation says what is
actually true.

The reasons, in order of weight:

1. Read-only is a boundary on writes, not on reads. The host root has to stay
   readable for real work: toolchains, system binaries, the libraries the
   workspace links against.
2. A mask blocklist is never complete. `~/.config/git/credentials`,
   `~/.password-store`, a key in `~/work/.env`, an ssh-agent socket — the list
   of shapes a credential takes is open-ended, so a blocklist buys an
   incomplete guarantee at the price of a false one.
3. A blocklist also breaks legitimate in-sandbox work: `ssh` to a build host,
   `git push` through the user's credential helper.
4. The guarantee the sandbox *can* make is worth stating precisely instead of
   overstating: an empty writable `HOME`, and "readable but not writable" for
   the rest. `sindri doctor` already prints exactly that, derived from the
   policy (`crates/core/src/sandbox/describe.rs`), so enforcement and
   description stay one value (ADR-0002).

## Consequences

- **Writes are blocked. Reads are not.** Any command the model runs can read
  every credential under the host root and — the network is shared — send it
  anywhere. The sandbox protects the machine from modification, not the user's
  secrets from disclosure.
- Isolation beyond that boundary is the user's choice of what to run against:
  which model and provider, and whether a prompt or a session transcript may
  leave the machine at all. A run against an untrusted endpoint is not made
  safe by the sandbox.
- The model is told the truth rather than a comfortable version of it: the
  description says "readable but not writable" and never implies invisibility.
- No masking machinery exists, so nothing can rot when a blocklist entry goes
  stale, and in-sandbox `ssh` and `git push` keep working.
- Anyone reopening this — a TUI trust rail, a landing page, a README — must
  either implement masking or point at this ADR. The promise may not be made in
  prose alone.

## Alternatives rejected

- **Option 1 — mask a credential blocklist.** `--tmpfs` (or an empty read-only
  bind) over `~/.ssh`, `~/.aws`, `~/.gnupg`, `~/.netrc`, `~/.config/gh`,
  `~/.docker/config.json`, `~/.git-credentials`. It directly honours user story
  23 and is one line in the description, but a blocklist is never complete and
  a wide mask breaks legitimate work inside the sandbox (`ssh` to a build host,
  `git push`).
- **Option 2 — mask the whole host home.** Strongest and simplest to describe
  ("your home is not mounted at all"), but it also removes `~/.cargo`,
  `~/.local/share` and any toolchain installed outside the workspace: a far
  larger behaviour change than the disclosure it closes, paid by every user to
  protect some.

## References

- Issue #13 (this decision), issue #1 user story 23 (the spec promise).
- ADR-0002 (one `bash` tool, policy as single source of truth),
  ADR-0005 (config surface).
