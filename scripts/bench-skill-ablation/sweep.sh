#!/bin/bash
# sweep.sh RESULTS.jsonl ARM... — every arm, REPS times over, one JSON line per
# run appended to RESULTS.jsonl. All the arms land in one file because the file is
# one comparison: the mean of `with` against the mean of `without`, and the rate
# at which `with` and `no-trigger` load a skill before the first command, over the
# same task and the same repetitions.
#
# The file is a record, not a workbench: a repetition already in it is refused
# rather than measured twice under one number. A run that never reached the model
# carries no token count and is tried once more before it is recorded.
set -uo pipefail
BENCH=$(dirname "$(readlink -f "$0")")
out=${1:?usage: sweep.sh RESULTS.jsonl ARM...}; shift
REPS=${REPS:-6}
tag=${TAG:-run}
case $REPS in ''|*[!0-9]*|0) echo "sweep: REPS must be a positive number, not '$REPS'" >&2; exit 2;; esac
[ "$#" -gt 0 ] || { echo "sweep: name at least one arm" >&2; exit 2; }
for arm in "$@"; do
  for rep in $(seq 1 "$REPS"); do
    if [ -f "$out" ] && jq -e --arg arm "$arm" --arg tag "$tag" --argjson rep "$rep" \
      'select(.arm == $arm and .tag == $tag and .repetition == $rep)' "$out" >/dev/null 2>&1; then
      echo "sweep: $out already holds $tag $arm repetition $rep — a new TAG or a new file" >&2
      exit 2
    fi
    # each attempt is its own run directory: a retry must not overwrite the
    # transcript of the attempt that never reached the model
    for try in 1 2; do
      line=$(TAG=$tag REPETITION=$rep ATTEMPT=$try "$BENCH/run_one.sh" "$arm" "$BENCH/runs/$tag-$arm-r$rep-a$try")
      if printf '%s' "$line" | jq -e '.in_tokens' >/dev/null 2>&1; then break; fi
      sleep 3
    done
    if ! printf '%s' "$line" | jq -e . >/dev/null 2>&1; then
      echo "sweep: $arm repetition $rep wrote no line, so nothing is recorded: see runs/$tag-$arm-r$rep-a$try/err.txt" >&2
      exit 2
    fi
    # the line is kept either way — a run that never reached the model is a fact
    # about the sweep — but it carries no token count, so the report says the
    # arm's mean is over fewer runs than it made
    if ! printf '%s' "$line" | jq -e '.in_tokens' >/dev/null 2>&1; then
      echo "sweep: $arm repetition $rep never reached the model, so it is recorded with no tokens: runs/$tag-$arm-r$rep-a$try" >&2
    fi
    printf '%s\n' "$line" >> "$out"
    echo "$arm rep $rep done" >&2
  done
done
