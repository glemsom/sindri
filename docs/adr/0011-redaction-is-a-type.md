# Redaction is a type, not a habit

Secrets enter at exactly two doors: configuration (`api_key`, every `extra_headers` value) and the
Copilot token store. They want out through four windows: the record, the JSON, the verbose stream,
and provider error text that echoes the request back.

`SecretSet` holds the literal strings (no regex — a pattern that misses is worse than a literal
that matches). `Redacted` carries the `SecretSet` it scrubs through, rather than a bare `String`
handed its set at every print: a value that has to be given a set is a value that can be printed
without one. `Display`, `Debug` and `Serialize` all scrub, so forgetting is a **type error** rather
than a code review, and the verbose stream — one of the four windows — is covered. The set is a
constructor argument of the provider seam's client, so a client that knows no secrets says
`SecretSet::default()` where it is built instead of inheriting that state by omission.

§14's window is closed the same way: a status is built from a `Told` — the state, the turn, a stop
reason — and its line is one fixed phrase per variant, so a stop reason's own detail stays in the
record where the redaction reaches it.

The test that keeps it honest walks every `Entry` variant with a known secret planted in each
field and asserts the serialized bytes never contain it; another plants a refused request's text
in a stop reason and asserts no word of it survives into the status.
