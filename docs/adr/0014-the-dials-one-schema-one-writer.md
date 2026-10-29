# 0014 — The dials: one schema, one writer, and a thread that survives

- **Status:** accepted
- **Date:** 2026-10-30

## Context

ADR-0011 §12 promised `/settings`: a modal over the one config file, applying on
save with no restart, with the API key defaulting to "from environment" and
never echoed, recorded or transcribed. Building it raised three questions the
contract did not answer, and closed a gap ADR-0012 recorded as known.

1. **Where does writing live?** The frame is a renderer; the schema is the
   core's. A second parser in the TUI is a second source of truth, which is the
   one thing ADR-0005 exists to prevent.
2. **How does a secret stay a secret** when a screen must say whether one is
   set, and a save must write it?
3. **How does a saved setting apply to the next prompt** without a restart —
   and, because that is the same question, how does a conversation survive a
   change of model mid-thread?

## Decision

**A `Document` in the core, and the TUI holds one.** `sindri_core::config`
gains `Document` and `Key`: the config file, staged for editing, with
`editable(key)`, `is_set(key)`, `set(key, value)` and `save()`. Every `set` runs
the check the run itself runs — `Provider::parse`, the turn cap's floor, the
timeout's ceiling, `WritableSet` for the extras, the header rules — so a value
the modal accepts is a value a run accepts, and a value it refuses is refused
with the same words. The file's own shape (which keys exist, what is optional)
stays one `#[serde(deny_unknown_fields)]` struct: an unknown key is still an
error, and a key the modal never touches is still in the file after a save.

**Saving is atomic and private.** The bytes are written to a temporary beside
the real file, created `0600`, then renamed over it: a reader never sees half a
file, and a credential in it is never momentarily world-readable. The reader
never meets a parse error's source snippet — the position and the message are
kept, and the line itself, which may be the credential, is not.

**A secret is never handed back, so it is replaced rather than edited.** `Document::editable` returns an empty string
for `api_key` and `extra_headers` whatever the file holds, so no field, no
frame and no error message can print one by accident: the modal can say *set in
the file (hidden)* or *from environment*, and that is all it may know. Typing a
value into one of those fields is the explicit act that writes it to disk, and
typing is what turns on the plaintext warning, which names the path the value
will land in. The value is masked while typed, never echoed, never in the
transcript, and never in the session record — three claims a test asserts from
outside: on the drawn buffer, on the transcript, and on every file the run left.

**`Esc` writes nothing, `Enter` writes at once — and only what was touched.**
The modal stages fields in memory and touches the disk only on save. `Enter`
stages the fields a keystroke has been in, through the schema, onto a *copy* of
the document, and only a document that took every value is written: a refusal
keeps the modal open, says which key refused, and leaves the file
byte-identical. A field nobody touched is not written at all — which is the
same rule that keeps a secret alive, because a secret is the one value the
document never hands back: staging an untouched key field would write an empty
one and silently delete the credential the file held. Editing a key means
typing one; emptying the field is the explicit act that returns the run to the
environment, and `extra_headers` is replaced whole when it is typed at all —
there is no way to be shown a value in order to edit it in place, and that is
the price of never drawing one.

**A saved setting applies because the front end reads the file again.** The
frame raises a reload; `Driver::reload()` resolves the config once more from the
same CLI overrides — a flag still wins over the file after a save (ADR-0005) —
rebuilds the sandbox policy over the session that is already running, and
answers with what the rail should now show. A config that cannot start a run is
said in the transcript, where the error belongs, and the frame stays usable.

**The thread belongs to the conversation, not to the model.** `Agent` owns the
messages the model was shown, so a second prompt is a follow-up (ADR-0012's
known gap, closed here). The front end keeps one agent for the launch and
changes its dials between prompts — model, tool, turn cap, thinking — so a
model saved in `/settings` answers the *next* turn on the thread the last one
built. `Message::ToolResults` and the assistant turns that asked for them stay
in the thread; `/new` forgets it, because a new session is a new conversation.

**A prompt cut short leaves no half-exchange.** A run stopped by the turn cap, a
cancel or a failure can end between an assistant's tool call and its answer, and
the next request would carry a question nobody answered. So at the end of every
run the thread is cut back to the last model turn that ended a thought — a
prompt with no reply is not replayed into the next request.

**The face yields to the facts.** The rail's rows are laid out first, and the
image takes what is left of the column, and nothing at all if that is less than
its own 20 rows: a terminal too short for both keeps the sandbox posture, the
branch and the record, and loses the ornament (ADR-0011 §5: nothing in the
layout depends on the face). The rule down the rail's left edge now runs through
the blank row that separates the session's facts, so the panel reads as one
panel at 100 columns and at 160.

## Consequences

- The TUI gained no parser and no schema: `/settings` writes through the same
  code a run reads, and `crates/core` still imports no front end.
- A key in the file is not always the key a run uses: `SINDRI_API_KEY` or the
  provider's own variable is read above the file (ADR-0005), so a run with one
  exported ignores a key saved here. The modal says what the *file* holds, which
  is what it can see; the precedence is the run's, and the README's table is
  where it is stated.
- A key typed mid-launch is not in the record's redaction set, which was built
  from the credential the launch started with. Nothing the record carries holds a
  key (the recorded text is prompts, answers and commands), and the modal never
  puts one in the transcript; a credential pasted into a *prompt* is the one
  path, and it was already the path before this slice.
- The conversation grows without us pruning it: it is bounded by the provider's
  context window, not by a decision here. Trimming old turns is a later issue,
  and the rail's meter is what tells a user they are near the limit.
- `/settings` is the only writer the frame has outside the session record, and
  the modal's own tests point it at a temporary directory so no test can write a
  real user's config.
