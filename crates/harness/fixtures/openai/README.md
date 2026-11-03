# OpenAI-compatible replies

Bodies in the shape an OpenAI-compatible server answers with: Chat Completions, as served
by Ollama and LM Studio at `http://localhost:11434/v1` and `http://localhost:1234/v1`, and
by aggregators such as OpenRouter at `https://openrouter.ai/api/v1`.

Fixtures, not captures: no local model and no credential were available where this was
built, so the bodies were written to the wire format those servers document. The live smoke
run is issue #11.
