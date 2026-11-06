# Redaction is a type, not a habit

Secrets enter at exactly two doors: configuration (`api_key`, every `extra_headers` value)
and the Copilot token store. They want out through four windows: the record, the JSON, the
verbose stream, and provider error text that echoes the request back.

`SecretSet` holds the literal strings (no regex — a pattern that misses is worse than a
literal that matches). `Redacted(String)` carries every string that leaves core, and its
`Serialize` and `Display` both scrub through the set, so forgetting is a **type error**
rather than a code review. The `Transport` seam scrubs before returning, which is the one
place a provider's angry 401 would otherwise smuggle a key into a transcript.

§14's "`msg` is never the command, the prompt or the answer" uses the same trick: the status
message is built from an enum of permitted facts, never from a formatted string.

The test that keeps it honest walks every `Entry` variant with a known secret planted in each
field and asserts the serialized bytes never contain it.
