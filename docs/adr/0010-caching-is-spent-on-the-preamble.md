# Caching is spent on a preamble that never moves

The rail promises `cache hits`, so something has to be cached. Per wire: **Anthropic** gets
two explicit `cache_control` breakpoints — one closing the preamble block, one rolling over
recent history — out of the four the API allows. **OpenAI-family and Copilot** cache prefixes
automatically, so nothing is spent and `cached_tokens` is only reported. A wire that answers
with no cache figures makes the rail **drop the fact** rather than print a zero.

The consequence is a hard rule the rest of the design already needed: **nothing volatile may
enter the preamble.** No timestamp, no session GUID, no turn count. §5's "the preamble stays
byte-identical" and §10's `{session}` header expansion are not tidiness — they are what makes
the first breakpoint hit on turn 2 instead of never.
