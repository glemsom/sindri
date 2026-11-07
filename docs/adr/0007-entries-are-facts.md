# Entries are facts; streaming is not written down

The transcript's unit (`Entry`) is a tagged enum holding **what happened**, never how it
looked: the front end re-renders from facts, so the TUI and any reader cannot disagree.

Streaming deltas are deliberately not entries — a 250-turn run would write a million lines
for one prompt. Durability is **turn granular** — one entry when a turn starts, one per tool
call, one per result, the turn's reasoning as one entry of its own, one assistant entry when
the turn ends — so a `kill -9` loses at most the in-flight turn, which is what "a cancelled
run still reads" actually promises.

A thought is settled at the same granularity and written the same way: what a model
reasoned is a fact of the turn, so it is one `reasoning` entry beside the answer it
reached and never a line per fragment (#38). The interface draws the fragments from its own
value while they stream, and the transcript draws the thought from the entry it settled
into, because the same rows are laid out either way.

Forward compatibility is two rules and no versioning ceremony: a reader skips an unknown
`kind` and ignores unknown fields. A newer `sindri` may always write what an older one can
read past.

## Amendment — a result carries the verdict (#40)

`Entry::Result` gained `exit` when the tool rows landed: `{"code": 0}`, `{"code": 1}`,
`"timed_out"`, `"cancelled"`, or the `"signalled"` a platform reporting no code at all
describes. It is a fact for the reason this record exists — a row reading `✗ exit 1` used
to be parsed out of the text beside it, which is the record lying about itself — and it is
what `Acted` carries out of the executor, so the run, the record and the picture read one
value and the words beside them come from one table.

Two consequences. The text of a result is now what the command *printed*: the verdict is
no longer prefixed to it, because one fact written twice is one fact and one line of prose,
and whoever hands a result back to the model writes that line from `Exit`. And the field is
absent rather than `null` on a line an older build wrote, so `exit` defaults to nothing — a
row that says a call's name and claims nothing about how it went, which is the only honest
picture of a transcript that never recorded a verdict.
