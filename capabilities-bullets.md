# Project: Sindri
An AI assistant for GNU/Linux

## The idea
 - AI coding and assistant agent for GNU/Linux. **(No need to support Windows or MacOS)**
 - A stunning terminal application, making use of Kitty protocol and animations and colors to make the TUI fell modern, responsive - and with the norse personality of Sindri
- Programming language of Sindri: Rust
- **Terminal-native.** Full-screen alternate-screen TUI for humans, assumes UTF-8 + truecolour, Assumes Kitty protocol for images. Renders answers in `markdown`.
- **Unix composition over bespoke tools.** File access, search, fetch, patch, HTTP all go through the shell.
 - Besides TUI, also supports `batch` input for scripts. (Example "git diff | sindri -b "Give me a summary")
- **Exactly 1 tool.** `bash` (Other tools might arrive later)
- **Sandboxed by default, explicit by design.** Every `bash` call runs in an unprivileged `bubblewrap` with a read-only host root and a writable set (workspace, session temp, configured extras). One writable-path set feeds both enforcement and the model-facing description so they cannot drift. 
-  `--yolo-unsafe` opts out of isolation, and runs with user priviledges

## Where Sindri is different from other AI Agents
Other AI agents will have specific native tools for specific tasks - due to the need to be cross-platform.
Sindri **does not target crossplatform**, hence we do not have the same restrictions.
Sindri has **one main tool**: `bash`
Sindri follows the Unix philosofy of combining the powerfull GNU/Linux tools together, to get a task done. 
This can be either through chaining or piping commands together - or by writing a simple script if more advanced logic or state is required.
Sindri enables the LLM to write small scripts, to for example spawn mutible tool calls in one turn - and thereby heavily reducing the total token usage. (The inspiration for this is CodeMode in Pi.dev - https://pi.dev/docs/latest/codemode)

### Providers support
 - Relying https://rig.rs/docs + https://rig.rs/docs/concepts/agent (Research how to use these tools)
 - Prefer to use other libraries instead of rolling our own

### Tools and sandbox
- `bash` (Always executed in sandbox, unless `--yolo-unsafe`)
 - Default timeout: 120s, max timeput: 3600s.
 - Tell LLM about exit codes
- Sandbox: `--share-net` (so `curl` works — the sandbox bounds the filesystem, not the network), read-only `/`, writable workspace plus session temp plus extras, `TMPDIR/TEMP/TMP` redirected to session temp.
- Boot verifies `bwrap, bash, rg, curl, lynx, patch, python3, git, jq, yq, xdg-open` — so the system prompt can never lie about the environment.

### Batch mode (the Unix-filter contract)
- Text mode: stdout gets exactly the answer plus one newline; reasoning goes to stderr, only with `-v`, only after the run.
- `--format json`: one compact object with fixed field order — `answer, session, turns, stopped, model, duration_ms, tool_calls, usage`; absent means no data.
- Never silently drains stdin: a TUI launch with a pipe is refused.

### Terminal UI
- Right rail as a **trust panel**: cache hit percentage, turns against cap, tokens, live context meter, session GUID, session temp path, git branch, and sandbox posture (`bubblewrap` versus `unsafe`).
    - Right rail also shows the face of Sindri (face.png)
- Composer 2-8 rows
  - Support multiline using shift+enter
  - @ sign to help auto-complete files/folders
- `/settings` modal exposes the whole config.
  - In `settings` we can config:
    - Provider
    - Model
    - Optional key for provider
    - Toggle thinking & reasoning
    - Configure additional writeable paths for Sindri
- Drag-select copies exactly the rendered rows (ANSI stripped) (Could be through clipboard via OSC 52 - other suggestions welcome)

### Diagnostics and build
- Reproducible packaging, version injected at build time, test/CI gate.
- Must use devbox or containers (something where builds can be reproduced)

## Notes
 - If sandbox cannot be created, we about Sindri
 - The user is the non-technical product manager; you are the senior developer