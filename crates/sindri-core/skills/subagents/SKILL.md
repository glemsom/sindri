---
name: subagents
description: Delegate substantial implementation, testing, review or research to fresh batch contexts through bash.
---
# Subagents

Fresh context is the primary purpose; concurrency is optional. Keep trivial command-sized work inline. Write a temporary Bash runner under
`TMPDIR` and execute it with the existing bash tool in one call. The launch supplies
`SINDRI_EXECUTABLE` and `SINDRI_LAUNCH`; never read, print, copy or put the handoff's contents
in a prompt or argv. It contains secrets and expires with the parent session temp.

Give each brief workspace context, goal, relevant facts, exclusive edit ownership, verification
expectations and a concise answer shape/length suggestion. Children see neither this conversation
nor loaded skills. They can discover the same skills and edit with the same authority, not more.
Tell children not to delegate. Avoid overlapping child edits and your own edits to child-owned files;
use sequential groups for dependencies. Request bulky deliverables in workspace files or the group's
artifact directory, not a child's disposable temp. Collect paths and short summaries. Children cannot rely on asking the user for clarification.

## Prepare briefs

Select independent work and assign exclusive file/area ownership before launching. Keep parent
edits outside those areas until collection finishes; sequence dependent or overlapping work.
Each array element should supply:

- Workspace path and relevant repository instructions.
- Goal, known facts, relevant files and any decisions the child needs from parent history.
- Scope, authorized edits and exclusive ownership (or explicitly no edits).
- Verification commands and expected evidence; report limitations rather than inventing results.
- Deliverable shape: findings/changes, verification, paths and blockers; suggest roughly 150–300
  words when useful, not a hard cap. Save bulky detail in authorized workspace files or the shared
  artifact directory appended by the runner. Child temp disappears when its launch ends.
- Complete this brief directly; do not delegate recursively or depend on user clarification.

Children retain equal permissions. This recipe prescribes neither branch nor workdir management.
Choose all briefs in the group yourself; there is no scheduler or automatic retry.

## Bash runner

Use a Bash array, one self-contained brief per element. One element is valid. Choose group size
and a bash tool timeout sufficient for the entire group. Write this recipe to `$TMPDIR/runner.sh`,
replace the example brief, and run `bash "$TMPDIR/runner.sh"`. Do not detach collection.

```bash
set -uo pipefail
umask 077
: "${SINDRI_EXECUTABLE:?launch executable missing}"
: "${SINDRI_LAUNCH:?launch handoff missing}"
group=$(mktemp -d "$TMPDIR/subagents.XXXXXXXX") || exit 1
briefs=('Work in the inherited workspace. Do not delegate. Inspect the assigned area; return a short verified summary. Make no edits.')
pids=()
stop_group() {
  for pid in "${pids[@]}"; do kill "$pid" 2>/dev/null || :; done
  for pid in "${pids[@]}"; do wait "$pid" 2>/dev/null || :; done
}
trap stop_group EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
for i in "${!briefs[@]}"; do
  prompt=$(printf '%s\nShared artifact directory: %s\n' "${briefs[$i]}" "$group")
  env -i PATH="$PATH" SINDRI_LAUNCH="$SINDRI_LAUNCH" "$SINDRI_EXECUTABLE" -b "$prompt" --format json \
    </dev/null >"$group/$i.json" 2>"$group/$i.stderr" &
  pids+=("$!")
done
for i in "${!pids[@]}"; do
  code=0
  wait "${pids[$i]}" || code=$?
  printf 'brief=%s exit=%s artifacts=%s\n' "$i" "$code" "$group/$i"
  if [[ ! -s "$group/$i.json" ]]; then
    case "$code" in
      2|3) status=setup_or_refusal ;;
      *) status=missing_json ;;
    esac
  elif ! jq -se 'length == 1 and (.[0] | type == "object" and
      (.stopped | IN("final", "max_turns", "budget", "cancelled", "error")) and
      ((has("answer") | not) or (.answer | type == "string")))' "$group/$i.json" >/dev/null 2>&1; then
    status=invalid_json
  else
    stopped=$(jq -r '.stopped' "$group/$i.json")
    if [[ "$stopped" != final ]]; then
      status="non_final:$stopped"
    elif [[ "$code" != 0 ]]; then
      status=exit_failure
    elif ! jq -e '.answer | type == "string"' "$group/$i.json" >/dev/null; then
      status=missing_answer
    else
      status=success
    fi
  fi
  printf 'status=%s json=%s stderr=%s\n' "$status" "$group/$i.json" "$group/$i.stderr"
  if [[ "$status" == success ]]; then jq -r '.answer' "$group/$i.json"; fi
done
trap - EXIT
```

Keep complete answers; do not truncate them. An exit other than zero, a non-final stop or missing/
invalid JSON is not success. Inspect stderr only when needed, never dump it by default. Decide
whether to retry or finish inline. The existing bash timeout/cancellation kills the process group;
children must not escape it or run detached. The child preserves enforcement (including explicitly
unsafe launches); a nested sandbox refusal must be reported, never bypassed with an unsafe flag.


## Failure and lifecycle decisions

Only `status=success` prints an answer: exit zero, one valid outcome, `stopped=final`, and a
string answer. Other statuses retain explicit JSON/stderr paths; non-final partial answers stay
in JSON. Inspect only the relevant artifact (for example a bounded stderr range). A setup/refusal
never justifies weakening enforcement. Choose a new explicit brief for a manual retry, or finish
inline; the runner retries nothing.

Set the tool timeout for the slowest brief plus launch/collection overhead, within the inherited
command budget. Launch, wait and collect synchronously in that call. Timeout/cancellation can
interrupt collection; completed artifacts are session-scoped, not a detached collection service.
Linux subreaper cleanup kills and reaps all descendants, including thread-owned children and
separate groups, before returning. Kernel-uninterruptible tasks can delay cleanup beyond the
timeout; SIGKILL has no instant-exit guarantee. Forced supervisor/launch death is outside this
synchronous contract; parent-death signals and sandbox teardown are additional guards.

Execution evidence: batch fixtures exercised mixed failures, full sibling answers, parallel nested
bubblewrap launches, timeout and SIGINT cancellation. Actual child processes stopped and delayed
writes did not occur. Writable-boundary and protected credential/signing inheritance tests passed.
Compatibility requires Linux `/proc`, parent-death signals and working nested bubblewrap user/PID
namespaces; prerequisite skips are not evidence. Other hosts need their own execution checks.
