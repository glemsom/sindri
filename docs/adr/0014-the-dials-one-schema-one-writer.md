---
status: accepted
date: 2026-10-30
---

# 0014 — The dials: one schema, one writer, a thread that survives

## Decision

**One schema.** `sindri_core::config::{Document, Key}` (`crates/core/src/config/document.rs`) with `editable`, `is_set`, `set`, `save`; the frame parses no TOML. Every `set` runs the run's own checks — `Provider::parse`, the turn-cap floor, the timeout ceiling, `WritableSet`, the header rules — with the same words.

**Atomic, private save.** Bytes go to a temporary beside the target, `0600`, then rename. Parse errors keep position and message, never the line (it may be the credential).

**A secret is replaced, never handed back.** `editable` returns `""` for `api_key` and `extra_headers` whatever the file holds, so the modal can say *set in the file (hidden)* or *from environment* and nothing more. Typing is the explicit act that writes it to disk and turns on the plaintext warning naming the path. A key is masked while typed, never echoed, never in the transcript, never in the record — asserted on the drawn buffer, the transcript and every file the run left.

**`Esc` writes nothing; `Enter` writes only what was touched.** Staging goes onto a *copy* of the document; the file is written only if every value took. A refusal keeps the modal open and the file byte-identical. Untouched key fields are never staged (an empty write would delete the credential); emptying the field returns to the environment; `extra_headers` is replaced whole.

**A saved setting applies because the front end re-reads.** `Driver::reload()` re-resolves config from the same CLI overrides (a flag still wins, ADR-0005), rebuilds the policy over the running session (`Session::rebase`), and answers with the rail's new facts. A config that cannot start a run is said in the transcript.

**The thread belongs to the conversation.** `Agent` owns `history` and `forget`, one agent per launch, dials changed between prompts; `Message::ToolResults` and the assistant turns that asked for them stay. At run end the thread is cut back to the last completed model turn, so no unanswered half-exchange is replayed (ADR-0012's gap, closed).

**The face yields to the facts.** Rail rows are laid out first; the image takes what is left and nothing below 20 rows.

## Consequences

- The TUI gained no parser; `crates/core` still imports no front end.
- A key in the file may not be the key a run uses: `SINDRI_API_KEY` or the provider's variable is read above the file. The modal says what the *file* holds.
- A key typed mid-launch is not in the record's redaction set, which was built from the launch credential; nothing the record carries holds one. A credential pasted into a *prompt* remains the one path, and predates this slice.
- The conversation is bounded by the provider's context window, not by a decision here; trimming is a later issue and the rail's meter warns.
- `/settings` is the only writer the frame has outside the record; its tests point at a temporary directory.
