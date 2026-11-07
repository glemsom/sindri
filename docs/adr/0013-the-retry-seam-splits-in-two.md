# The retry seam splits: attempts in the transport, streamed-ness above it

§4 retries "the verdict the wire classified, not the status code" and forbids retrying "a
reply that already streamed text". A13 and ADR-0004 put both halves in one place: rig-core's
`Transport` trait, implemented once in core. rig-core 0.46 does not allow that, and the
library is the authority here rather than the note about it.

**What 0.46's `Transport` is.** `Transport<W>` is generic over a *wire*: `send` takes the
wire's own payload and opens the wire's own frames. Those frames are the framed reply —
event-stream bytes for Anthropic, JSON chunks for OpenAI — not text. A transport that wanted
to know whether text had streamed would have to decode the dialect itself, which is the work
`rig-core` exists to do. `HttpClientExt` is the trait that work is built on: every
`HttpClientExt` is a `Transport<W>` by rig's own blanket impl, and it sees one HTTP request
and one HTTP reply. So the transport keeps **attempts and headers**, and the question of
whether text had already landed is answered one layer up, by the seam that folds frames into
`Answer` and deltas.

**The shape that keeps #13 expressible.** `Failure` carries three facts and no policy:
`verdict` (the closed set: timeout, rate limit, server fault, transport, client, refusal,
cancel, broken, unconfigured), `retryable` — the wire's own classification, straight from
rig's `is_retryable`, never re-derived from a status code — and `streamed`, set the moment a
text or reasoning fragment has gone out. §4's rule is then one line whoever writes #13:
retry when `retryable && !streamed`. Nothing in the provider seam retries, waits or redacts.

**What this buys.** The transport is replaceable in two independent ways: a fixture
(`transport::Replay`) hands back recorded bytes so the *real* wire encoder and the *real*
event-stream decoder run in every test with no socket and no key, and `Http` wraps any
transport to add one run's headers. Both are `Transport`s by construction, so nothing above
the seam can tell them apart.

**What it costs.** One allow for `clippy::result_large_err`: rig's HTTP error is 128 bytes
and appears in the trait's own signature. And route selection is rig's, not ours — Appendix
A10 survives unchanged: `OpenAIConfig::completion` routes a name to Chat Completions or
Responses and the recorded Responses fixture proves it by addressing `/v1/responses`.

**Not yet.** `rig-agent` is not a dependency: its `AgentRunner` is what #17 drives, and
nothing in it is needed to ask one question. The seam is a `Model` per wire and a fold, which
is the smallest rig surface that answers a prompt.
