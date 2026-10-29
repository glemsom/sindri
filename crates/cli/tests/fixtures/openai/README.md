# OpenAI-compatible replies

Bodies in the shape an OpenAI-compatible server answers with: Chat
Completions, as served by Ollama and LM Studio at `http://localhost:11434/v1`
and `http://localhost:1234/v1`, and as served by hosted aggregators such as
OpenRouter at `https://openrouter.ai/api/v1`.

They are fixtures, not live captures: the environments this was built in had
no local model and no credential, so the bodies were written to the wire format
those servers document. The live smoke run is documented in issue #11.
