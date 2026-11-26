# Responses continuation

Full client replay remains the default. Omitted `responses_store` leaves the
provider's inherited storage policy unchanged; it does not send `store`.

To opt in, configure `responses_continuation = true` and
`responses_store = true`. Only the OpenAI Responses route supports this mode.
Custom OpenAI endpoints additionally require
`responses_continuation_verified = true`, an explicit assertion that the operator
has verified that endpoint's response storage and chaining contract. Copilot and
Chat refuse continuation before submitting a request. These settings also have
`SINDRI_`-prefixed uppercase environment variables and config schema entries.
`responses_store = false` explicitly disables Responses storage, but cannot be
combined with continuation.

Each retained native Responses turn owns its response ID and exact endpoint,
route, model and storage policy. Only an unbroken compatible retained sequence
can continue a handle. New prompts send only their new input; tool turns send
only the most recent tool outputs. Instructions and tool definitions are resent
on every request. Handles are never shared through responder-global mutable
state. Conversation reset clears their owners; generation-checked joining keeps
superseded completions from restoring them. Model, route, endpoint or storage
changes establish a local replay boundary with the existing visible-context
transition and loss reporting.

An unstreamed client rejection explicitly identifying the previous response
handle triggers one local replay of the retained native material and actual tool
results, not another execution of tools. That conversation then stays on local
replay until reset or a supported new boundary. Generic errors and partially
streamed failures do not trigger recovery. If replay is rejected or exceeds the
configured capacity, the run reports the failure and retains the material;
recovery is not guaranteed for missing or incompatible opaque artifacts.
Storage authorization and server retention remain endpoint/operator constraints.
No durable session resume or provider-native compaction is implemented here.
Client-managed summary compaction is disabled whenever continuation is selected,
including local recovery: a server chain is never independently pruned. Capacity
admission conservatively counts the full retained replay, not merely the new
input sent with a handle; exceeding it refuses without changing retained material.

Verified with captured mock HTTP requests through real launches; this is not a
live-service capability claim. Run `devbox run -- cargo test -p sindri-core
--test launch` (normal workspace tests include these cases).

API contract: https://developers.openai.com/api/docs/guides/conversation-state
(accessed during implementation): top-level instructions apply only to the
request supplying them, chaining uses `previous_response_id`, and response
storage can be disabled with `store: false`.
