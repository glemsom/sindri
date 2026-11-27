# Conversation replay

Conversation requests replay retained client-side material, including native assistant
blocks and Responses output items. By default they do not use `previous_response_id`; explicit verified Responses
continuation is described in [Responses continuation](responses-continuation.md).
Responses requests leave `store` unset, preserving the pre-existing endpoint storage
policy rather than silently switching it on or off. Full client replay does not itself
promise stateless operation or disable the endpoint's default persistence.

Reasoning requests ask for encrypted reasoning content through the locked adapter.
Returned native artifacts are retained alongside display text. Stateless encrypted
replay is not selected by this integration; an explicit storage-policy configuration
and endpoint compatibility validation would be needed to offer that mode.

Native replay uses the same truthful tool-result completion as ordinary replay:
failed results remain failures and unexecuted calls receive explicit error outcomes.
Activated skill snapshots remain in model-facing requests. [Slash skill invocation](skills.md)
starts a turn even when the skill is already loaded; legacy `/skill:name` only loads instructions.
Configured capacity counts
the serialized completion request, including native artifacts, instructions, tools,
and retained results, plus output and expected-tool-result headroom. It never silently
truncates retained material.

Verification uses canned streaming responses and real launch request capture, not
live-provider compatibility evidence. Run the wire goldens explicitly with:

```sh
cargo test -p sindri-core --test eval_wire -- --ignored --nocapture --test-threads=1
```

## Conversation reset

`/new` uses core `Conversation::reset` to clear retained turns (including native
artifacts) and the launch's loaded instruction snapshots/names. Runs receive a
scoped loaded-skill view: reset invalidates their authority to activate instructions.
Retained reports join only their captured conversation generation. The frontend
captures that generation when a run starts, not when a prompt enters the queue.
Late display/session reports do not become retained model-facing material.

Reset leaves launch ownership, session-temp lifetime and confinement unchanged.
Provider continuation handles belong to retained native turns and obey this same
generation check, not a frontend cache.

## Wire target changes

Captured native material is bound to its endpoint address, effective route, exact
served model and inherited storage policy. Model matching is deliberately stricter
than family matching: no cross-model opaque compatibility has been verified.
Changing any of these creates a visible-message boundary: prompts, assistant text,
call IDs, original arguments and actual tool outcomes replay, while incompatible
native artifacts and reasoning are omitted. The terminal reports that loss. Local
retained material remains available; no completed tool is executed by the transition.

`Ask::replay_target`, `Ask::transition` and `Ask::replay_capabilities` expose the
core decision. Explicit verified standalone native compaction is described in [Native compaction](native-compaction.md). Continuation is opt-in and gated
by route, endpoint verification and explicit storage authorization. Same-target native replay remains supported
by the existing adapter capture tests; that is not verification of custom endpoint
continuation features.

## Client-managed compaction

With a validated `context_window`, requests reaching three quarters of the configured
capacity attempt a separate ordinary, tool-free model summary request. Unknown capacity
still replays faithfully. The summary asks for objectives, constraints, exact identifiers,
evidence, actual side effects versus plans, unresolved errors and next actions.
Skill-delivery exchanges, standalone instruction snapshots, incomplete exchanges and the
last two retained turns remain exact; current-run exchanges are never split. Older completed
tool exchanges enter the summary atomically rather than accumulating in the protected prefix.
Repeated skill calls are still protected, so pathological skill-loading conversations can
exhaust admission. Summary fidelity depends on the model; deterministic captures verify the
summary request includes evidence and the supplied summary retains continuation facts.
Native artifacts are not transferred into the new summary prefix. Protected exchanges use the ordinary target-compatibility transition.

Both the summary request and the resulting continuation must pass capacity admission.
Failure leaves original material intact and refuses rather than submitting oversized
input. Fully protected conversations may require a larger configured window or `/new`.
Original session records are not replaced. Successful continuation reports a compaction
status event. This deliberate prefix transition is not provider-native compaction, storage
opt-in or a continuation handle; `replay_capabilities` distinguishes verified native support from client summarization. No live-provider summary-quality guarantee is claimed.
