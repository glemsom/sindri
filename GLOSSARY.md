# Glossary

Sindri's vocabulary. Use these words exactly; do not drift to synonyms.

## Agent

**turn** — one round trip to the model: our request, its response (text, and
possibly tool calls), and the tool results we feed back. Bounded by the **turn
cap** (`max_turns`, default 250).

**tool call** — one request from the model to run `bash`, plus the structured
**tool result** we return. A single call may run a whole pipeline or script.

**bash tool** — the only tool. Runs a command string inside the sandbox.

**answer** — the final assistant text of the last turn. The product.

**stopped** — why the loop ended: `final` (the model finished), `max_turns`,
`cancelled`, `error`. Appears in the JSON output.

**cancel** — Ctrl-C (SIGINT): one flag the loop and the bash executor share. The
loop stops between turns; a running command's process group is killed; nothing
reaches stdout; exit 130.

## Sandbox

**sandbox policy** — the value holding writable set, network posture and temp
mapping. The *only* source of both enforcement and description (ADR-0002).

**writable set** — the paths mounted read-write in addition to the session temp:
the workspace plus configured extras.

**extras** — user-configured extra writable paths.

**session temp** — per-run writable scratch directory, mapped to `TMPDIR`,
`TEMP` and `TMP` inside the sandbox, and mounted as `HOME` beneath it.

**description** — the model-facing text derived from the sandbox policy, also
printed by `sindri doctor`.

**yolo-unsafe** — `--yolo-unsafe`: run `bash` with the user's own privileges, no
bwrap. Always visible in the description and the session record.

**doctor** — `sindri doctor`: verifies bwrap, the binary set and the mount plan,
and prints the sandbox description.

## Run

**session** — one run, identified by a GUID, recorded under
`$XDG_STATE_HOME/sindri/sessions/<guid>/` (ADR-0006).

**transcript** — the ordered record of messages, tool calls and tool results of
a session.

**spill file** — where the full text of an over-long tool result is written when
the model-facing copy is truncated head+tail.

**batch mode** — `-b`: the Unix-filter front end. stdout is the answer only;
stderr is silent unless `-v`; `--format json` switches the output shape
(ADR-0004).

**reasoning** — provider-side thinking tokens, off by default, shown on stderr
with `-v`.

## Release

**release artefact** — one static musl binary, named
`sindri-<version>-<target>`; the installer derives exactly that name from
`uname`, so the build and the installer share one naming contract.

**checksums** — `SHA256SUMS` in a release: every artefact, sorted, verified by
the installer before it writes anything.

**installer** — `install.sh`, run as `curl … | sh`: picks the target from the
machine, verifies the checksum, installs into the user's local bin and ends by
pointing at `doctor`.

**changelog** — the notes a release page and its `CHANGELOG.md` carry, derived
from the tickets a release closes: the merge commits since the previous tag,
which is what `TN: … (#issue)` is for.

**release image** — the container generated from the committed `devbox.json`
that builds the release, so dev, CI and release share one toolchain (ADR-0009).
