---
status: accepted
date: 2026-10-29
---

# 0010 — The read-only host root is readable, not invisible

## Decision

`--ro-bind / /` (ADR-0002) bounds writes and hides nothing. Under it, `~/.ssh/id_ed25519`, `~/.aws/credentials`, `~/.config/gh/hosts.yml`, `~/.netrc` and every other file under the host root stay readable by any command the model runs. Only `HOME` is ours: empty, in the session temp.

The tradeoff is accepted: no credential-masking blocklist. `describe()` (`crates/core/src/sandbox/describe.rs`) says "readable but not writable", derived from the policy, so enforcement and description stay one value.

User story 23 (issue #1, "credentials are never exposed") is therefore not literally true; issue #13 decided this.

## Consequences

- **Writes blocked, reads not.** With shared network, reads plus egress mean exfiltration is possible. The sandbox protects the machine from modification, not secrets from disclosure.
- Isolation beyond that boundary is the user's choice of model, provider and prompt; an untrusted endpoint is not made safe by the sandbox.
- In-sandbox `ssh` and `git push` keep working, and nothing can rot as a blocklist entry goes stale.
- Anyone reopening this — a trust rail, a landing page, the README — implements masking or points here. The promise may not be made in prose alone.

## Rejected

- Mask a credential blocklist (`--tmpfs` or empty ro-bind over `~/.ssh`, `~/.aws`, `~/.gnupg`, `~/.netrc`, `~/.config/gh`, `~/.docker/config.json`, `~/.git-credentials`): never complete, and breaks `ssh` / `git push` inside the sandbox.
- Mask the whole host home: also removes `~/.cargo`, `~/.local/share` and toolchains outside the workspace.
