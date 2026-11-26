# Native compaction

Full replay and client summarization remain defaults. Select `native_compaction = true`
and `native_compaction_verified = true` (also uppercase `SINDRI_` environment variables).
Verification is an operator assertion for the exact endpoint, route and served model,
including beta availability, not an automatic live probe or a universal model promise.
Chat and Copilot refuse even with verification. Unsupported selections refuse without
feature fields; deselect native compaction to use the existing client-summary fallback.
A known configured context window triggers compaction at the existing conservative
three-quarter admission threshold. Unknown capacity does not trigger reduction.

Responses uses standalone `POST /responses/compact`, not threshold compaction. Its
input still must fit the configured window. The complete returned `output` array is
retained as JSON, including opaque and unknown fields and retained user items. The
next input starts with that exact ordered canonical window, followed by protected
material and new work. It is never reduced to only the compaction item. Instructions
are resupplied. No storage setting is added by compaction; ordinary Responses requests
retain the configured/inherited storage policy.

Anthropic uses `POST /messages`, `compaction: {type: "summarize"}` and the official
`compact-2026-09-04` beta. Successful `stop_reason: "compaction"` returns complete
content blocks, retained unchanged inside an assistant message. The beta is also sent
when replaying those blocks. System and tools are supplied to the summarizer as to the
ordinary request. Native selection with thinking enabled or retained displayed
reasoning refuses: preserved-thinking prefix/signature transitions are not verified
by this implementation. Client summarization remains available by deselection.

Both paths protect the last two retained turns, skill-call exchanges, standalone
instruction snapshots and unmatched exchanges outside the summarized prefix. Native
summaries remain provider-written; exact older evidence and side-effect fidelity are
not guaranteed. No tool runs in the compaction call. The continuation must also pass
admission. Failed HTTP, malformed/empty returns, missing compaction items and failed
continuation leave the original recoverable material/cache unchanged. Target switches
use the existing visible-context loss boundary; reset invalidates the cached prefix.

Standalone selection refuses when Responses continuation is selected, including local
handle recovery. No server chain is independently pruned or resubmitted alongside full
historical input. Server threshold/chained native compaction is not implemented.

## Official contracts inspected with curl

- https://developers.openai.com/api/docs/guides/compaction
  distinguishes standalone canonical output from threshold output; explicitly says
  not to prune a `previous_response_id` chain.
- https://platform.claude.com/docs/en/build-with-claude/compaction
- https://platform.claude.com/docs/en/build-with-claude/compaction-on-demand
  documents the beta above, `summarize`, signed content blocks, matching system/tools,
  successful stop reason and model/platform-specific availability. The fetched page
  names Opus 4.6 and Sonnet 4.6 among supported models; this is not a capability claim
  for every older Claude identifier, gateway or cloud platform.

The locked rig 0.46 serializer can merge raw input/messages via additional parameters.
A narrow core encoding/HTTP seam preserves native JSON without converting a canonical
window into the adapter's narrower assistant union. Captured real launches test both
round trips, subsequent prompts, reset boundaries, protected instructions, refusal and
rollback. Existing continuation tests cover no duplicate chain input and recovery.
These are canned HTTP tests, not authorized live-provider experiments.

Required verification:

```sh
devbox run -- cargo test --workspace -- --test-threads=1
devbox run -- cargo clippy --workspace --all-targets -- -D warnings
devbox run -- cargo fmt --all -- --check
devbox run -- cargo test -p sindri-core --test eval_wire -- --ignored --nocapture --test-threads=1
```
