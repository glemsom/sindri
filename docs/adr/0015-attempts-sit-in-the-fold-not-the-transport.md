# Attempts sit in the fold, not in the transport

§4 retries "the verdict the wire classified, not the status code" and forbids retrying "a reply
that already streamed text". Neither half can be decided by the transport.

**What a transport can see.** `HttpClientExt::send_streaming` hands back a `Response` for a 429
and for a 503: the status rides on the reply, not on an `Error`, and the body that names
`rate_limit_error` is only read by the dialect's decoder one layer up. A transport that retried on
`status == 429` would be deciding from the number — the one thing §4 forbids — and a transport
that retried only its own `Error`s would retry connection resets and nothing else, leaving rate
limits and server faults unanswered.

**What the fold can see.** `Failure` carries the wire's own `retryable`, and `streamed`, which
exists only once a text or reasoning fragment has gone out. Those two fields plus the `Cancelled`
verdict are the whole policy:

    retry when retryable && !streamed && verdict != Cancelled

So the seam splits. The transport **dresses** the request — the run's extra headers, `{session}`
expanded to the session id, and the failure of a header the HTTP framing cannot carry. The fold
above it, `provider::retry`, **decides**, and it is still inside the one module tree that may name
rig (§10).

**What this buys.** One policy in one function, `retry::until_final`, with three attempts and
200 ms doubling; a test drives it with constructed failures — a cancel the wire calls retryable, a
5xx-shaped verdict the wire calls final — that no recorded reply can produce. The wait is a
`Backoff` seam, so the sequence `0, 200, 400` is asserted rather than slept through, and `Sleep`
is the only implementation that spends time. Nothing above the seam can tell: `Client::ask` is
still one call that answers or fails.

**What it costs.** The transport is not the single place an attempt is counted, and rig's HTTP
error is 128 bytes in the trait's own signature, so one `clippy::result_large_err` allow is spent.
