# Subagent design

## Purpose and scope

Delegate substantial exploration, implementation, verification or research into fresh contexts.
Parallel execution is an additional benefit, not a requirement. A builtin `subagents` skill teaches
the parent to write a temporary Bash runner and use the existing bash tool. There is no new tool,
scheduler, bundled executable resource or script-materialization API.

Keep trivial command-sized work inline. Every brief is self-contained: workspace, goal, relevant facts, exclusive edit ownership,
verification expectations, answer shape and suggested length. Children cannot see the parent's
conversation or implicitly inherit loaded skills, cannot rely on user clarification, and must not delegate recursively. Shared writable
authority permits authorized edits, not authority beyond the user's request. Avoid overlapping child
edits and concurrent parent edits to child-owned files; dependencies use sequential groups.

## Launch handoff

The launch publishes `SINDRI_EXECUTABLE` (the current executable's absolute path) and
`SINDRI_LAUNCH` (an immutable JSON handoff reference) to bash execution. The recipe starts the executable
with an explicit batch prompt, `--format json`, `/dev/null` stdin, and a clean environment containing
only PATH and the handoff path. It does not forward arbitrary environment variables.

The handoff carries the effective resolved Config, including CLI overrides, the resolved Boundary,
the original skill-discovery workspace, and an allowlisted environment: HOME, XDG_CONFIG_HOME,
PATH and a child XDG_STATE_HOME beneath parent temp. Inherited launches use this configuration
instead of layering ambient config, environment or child flags. The boundary includes parent temp
because the parent already has write access there. A child's changed working directory does not
become an additional writable grant. User and repository skills are discovered against the original
workspace and HOME; each child starts with an empty loaded-skill set and conversation.

Every child creates its own GUID session beneath parent temp. Its scratch is removed when its
launch closes; its session record and the group's stdout/stderr artifacts last only until the parent
launch closes. Durable deliverables belong in authorized workspace files; shared group artifacts
must be outside the child's own disposable temp. This is not a persistent retention service.

## Credentials and confinement

The handoff contains API keys and extra headers; never read it into prompts, argv, logs or tool
output. The parent holds a mode-0600 sealed memfd. Sandboxed bash receives a read-only data mount
at session-temp `launch.json`; unsafe bash reads the parent descriptor through `/proc`.
Readers require complete memfd seals or a read-only launch mount; transport setup refuses on failure.
The parent closes the descriptor during normal launch cleanup. The supported Copilot signing record is copied into a protected session-scoped config
store, also mode 0600. This allows signing machinery to operate under the existing writable temp
rather than trying to update the user's read-only config mount. HOME remains the user skill root.
No credential is placed in the child argv or exported environment. Existing redaction still applies.
This protects against accidental disclosure, not a malicious process running as the same user or
an agent deliberately reading an authorized secret file.

A child inherits bubblewrap or explicitly unsafe enforcement from the boundary. Every sandboxed
launch runs the existing proof, including nested launches; refusal is an error, never an unsafe
fallback. PATH is explicitly retained so the same toolchain's bubblewrap can be found in the nested
launch. The filesystem remains read-only except the inherited resolved writable set and session
temp. No broader write mount, sandbox bypass or permission weakening is introduced.

## Groups, collection and failure

The canonical skill recipe uses a Bash array of selected briefs and one background child per
element; a singleton works unchanged. The parent chooses group size, then waits and collects in
that same bash call. Each child has separate JSON and stderr artifacts. The runner appends the group artifact directory
to each explicit prompt, giving every child a shared accessible location outside child scratch. Collection prints a stable
array-index brief identifier, exit status, artifact path, stop and complete answer when available.
Answers are not hard-truncated; briefs request brevity or file-based bulky deliverables instead.

Missing/invalid JSON, launch refusal, nonzero exit or a non-final `stopped` value is not success.
Keep completed sibling answers identifiable; inspect failure stderr selectively rather than dumping
it. The parent decides retries or inline completion. Traps stop/wait children on runner exit; existing
bash process-group timeout/cancellation and bubblewrap parent-death semantics bind execution to
the tool call. A per-call Linux subreaper retains orphaned descendants, including separate process groups. There is no asynchronous
collection across calls or automatic retry policy.

## Verified single-child slice

The existing builtin catalog seam verifies parsing, discovery, model activation/wrapping, and
user-then-repository shadowing. Batch integration uses fixture providers and a real bash-launched
child, not an in-process substitute. Tests establish fresh prompts, separate GUID sessions,
inherited model, CLI API-key override, configured limits, repository discovery, authorized edits,
complete JSON answer collection and exclusion of credentials from displayed/recorded output.
A Copilot fixture verifies signing and the CLI-selected chat route through a real nested child.

Nested bubblewrap execution was actually exercised in this development environment: both the
parent and child completed, an authorized workspace write succeeded, and a write attempted from an unauthorized directory failed even when bash changed
the child launch working directory before startup. The canonical skill recipe itself is executed
by these single-child tests, and mode-0600 handoff/parent-temp cleanup are asserted. The explicit unsafe path was tested separately.
The sandbox test prints NOT VERIFIED and returns when prerequisites fail; that is not compatibility
evidence. Nested user namespaces, available bubblewrap and writable signing behavior remain
platform-dependent assumptions; other target environments require execution evidence.

## Verified parallel slice

The batch executable test runs the canonical skill runner with two array briefs through a real
parent bash call. The fixture accepts both child requests before answering either: sequential
execution cannot cross this gate. It verifies two distinct child sessions beneath parent temp,
no unprovided parent history, non-overlapping authorized workspace edits, and file deliverables
in the shared directory that remain readable after the child launches end. JSON and stderr are
separate for each index. The parent receives both stable identifiers, zero exits and complete
300-line final answers; child tool chatter is absent from collected results. The singleton tests
continue to execute the same recipe. Parallel evidence uses explicitly unsafe enforcement;
the separate single-child test remains the evidence for nested sandbox compatibility.

## Verified failure and lifecycle slice

The canonical recipe now accepts exactly one outcome object, validates stop and answer types,
and prints an answer only for a zero-exit final stop with a string answer. Identifiable statuses
separate setup/refusal, missing/invalid JSON, missing answers, non-final stops and exit failures.
Raw JSON and stderr stay separate; no failure logs or partial answers are automatically dumped.
A real provider-refused child was collected alongside a successful sibling. A batch wrapper
matrix exercises setup refusal, missing/invalid/multiple JSON, invalid answer types, inconsistent
exit status and representative max_turns/budget/cancelled/error outcomes. These synthetic outcome
cases validate collection, not the provider loop's production of every stop.

The current GNU/Linux devbox verification passed `cargo test --workspace`,
`cargo clippy --workspace --all-targets -- -D warnings`, and `cargo fmt --all -- --check`.
All 38 batch tests passed, including actual nested sandbox execution.
Batch regressions attempt to overwrite the handoff before both unsafe and sandboxed child
launches and reject writable-file substitutions. Real bash regressions observe delayed survivor
files after root-shell exit, retained pipes under timeout, and cancellation during detached forks.
A per-call subreaper waits for the root, then stops descendants before preserving its exit status.
Cleanup confirms stopped states before descending and rescans adopted children. Pipe collection
continues to check the call deadline and cancellation.

## Remaining limitations

Cleanup requires Linux `/proc` and parent-death signals; nested bubblewrap requires working user/PID
namespaces and its normal proof. Restricted hosts must refuse rather than fall back to unsafe mode.
These checks establish execution in this workspace, not portability to every host. The fork
supervisor uses fixed stack buffers and async-signal-safe Linux calls only; it never allocates,
uses Rust filesystem APIs or sleeps through Rust's thread runtime. Descendant cleanup has a
two-second scanning budget and a depth ceiling of 128. Kernel-blocked tasks cannot be guaranteed
stopped or reaped on a deadline; final `Child::wait` and filesystem operations can still block.
The bounded stop grace is not proof of quiescence: an adversarial fork race can escape the
unsafe-mode descendant scan. The supervisor walks each process leader's `/proc` children;
children forked by other threads can also evade that best-effort walk. Bubblewrap's PID namespace and parent-death lifecycle remain the
sandbox boundary, not that best-effort host scan.

Seals establish immutability, not identity. Read-only launch mounts cannot authorize unsafe
handoffs. A sandbox cannot modify the sealed data or gain host authority by substituting a
handoff: its kernel confinement persists across exec, and unsafe mount handoffs are refused.
An explicitly unsafe actor already has full same-user host authority, including the ability
to create another sealed descriptor or invoke the executable without a handoff. Authenticating
such descriptors would not revoke that authority; it is outside the sandbox threat boundary.
Timeout/cancellation
may interrupt collection before completed sibling answers are printed. Artifacts last only until
parent session cleanup. The recipe cannot guarantee model brevity, coordination or non-recursion.

Out of scope: a scheduling service, recursive delegation, detached work, automatic retries,
answer-length enforcement, child-only read restrictions, worktree/branch management, locks,
merge automation, conflict resolution, implicit history forwarding, or new artifact retention.
