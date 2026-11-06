# Entries are facts; streaming is not written down

The transcript's unit (`Entry`) is a tagged enum holding **what happened**, never how it
looked: the front end re-renders from facts, so the TUI and any reader cannot disagree.

Streaming deltas are deliberately not entries — a 250-turn run would write a million lines
for one prompt. Durability is **turn granular** — one entry when a turn starts, one per tool
call, one per result, one assistant entry when the turn ends — so a `kill -9` loses at most
the in-flight turn, which is what "a cancelled run still reads" actually promises.

Forward compatibility is two rules and no versioning ceremony: a reader skips an unknown
`kind` and ignores unknown fields. A newer `sindri` may always write what an older one can
read past.
