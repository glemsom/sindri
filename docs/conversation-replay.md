# Conversation replay

Conversation requests replay retained client-side material, including native assistant
blocks and Responses output items. They do not use `previous_response_id`.
Responses requests leave `store` unset, preserving the pre-existing endpoint storage
policy rather than silently switching it on or off. Full client replay does not itself
promise stateless operation or disable the endpoint's default persistence.

Reasoning requests ask for encrypted reasoning content through the locked adapter.
Returned native artifacts are retained alongside display text. Stateless encrypted
replay is not selected by this integration; an explicit storage-policy configuration
and endpoint compatibility validation would be needed to offer that mode.

Native replay uses the same truthful tool-result completion as ordinary replay:
failed results remain failures and unexecuted calls receive explicit error outcomes.
Activated skill snapshots remain in model-facing requests. Configured capacity counts
the serialized completion request, including native artifacts, instructions, tools,
and retained results, plus output and expected-tool-result headroom. It never silently
truncates retained material.

Verification uses canned streaming responses and real launch request capture, not
live-provider compatibility evidence. Run the wire goldens explicitly with:

```sh
cargo test -p sindri-core --test eval_wire -- --ignored --nocapture --test-threads=1
```
