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
  if ! jq -e 'type == "object" and (.stopped | type == "string")' "$group/$i.json" >/dev/null 2>&1; then
    printf 'missing or invalid JSON; inspect stderr selectively\n'
  else
    jq -r '"stop=" + .stopped, (.answer // "no final answer")' "$group/$i.json"
  fi
done
trap - EXIT
```

Keep complete answers; do not truncate them. An exit other than zero, a non-final stop or missing/
invalid JSON is not success. Inspect stderr only when needed, never dump it by default. Decide
whether to retry or finish inline. The existing bash timeout/cancellation kills the process group;
children must not escape it or run detached. The child preserves enforcement (including explicitly
unsafe launches); a nested sandbox refusal must be reported, never bypassed with an unsafe flag.
