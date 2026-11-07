# Entries are facts; streaming is not written down

The transcript's unit (`Entry`) is a tagged enum holding **what happened**, never how it looked:
the front end re-renders from facts, so the TUI and any reader cannot disagree.

Streaming deltas are deliberately not entries — a 250-turn run would write a million lines for
one prompt. Durability is **turn granular** — one entry when a turn starts, one per tool call, one
per result, the turn's reasoning as one entry of its own, one assistant entry when the turn ends —
so a `kill -9` loses at most the in-flight turn, which is what "a cancelled run still reads"
actually promises.

A thought is settled at the same granularity and written the same way: what a model reasoned is a
fact of the turn, so it is one `reasoning` entry beside the answer it reached and never a line
per fragment. The interface draws the fragments from its own value while they stream, and the
transcript draws the thought from the entry it settled into.

A result carries its verdict as a fact, `Entry::Result`'s `exit`: `{"code": 0}`, `{"code": 1}`,
`"timed_out"`, `"cancelled"`, or the `"signalled"` a platform reporting no code at all describes.
It is a fact for the reason this record exists — a row reading `✗ exit 1` parsed out of the text
beside it would be the record lying about itself — and it is what `Acted` carries out of the
executor, so the run, the record and the picture read one value. The text of a result is what
the command *printed*: the verdict is never prefixed to it, because one fact written twice is one
fact and one line of prose. The field is absent rather than `null` on a line an older build
wrote, so a reader sees a row that claims nothing about how the call went.

Forward compatibility is two rules and no versioning ceremony: a reader skips an unknown `kind`
and ignores unknown fields. A newer `sindri` may always write what an older one can read past.
