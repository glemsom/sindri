#!/bin/bash
# sweep.sh RESULTS.jsonl VARIANT... — every task for each variant, REPS times
# over, one JSON line per run appended to RESULTS.jsonl. One repetition is one
# complete pass of the four tasks, so a median is `jq -s -f median.jq` over the
# file and never a hand-gathered set of files with different tags. A run that
# never reached the model is not a measurement, so it is tried once more before
# it is recorded.
#
# The file is a record, not a workbench: a repetition already in it is refused
# rather than measured twice under one number. `REPS` defaults to 6 and `TAG`
# names the run directories; neither is guessed.
set -uo pipefail
BENCH=$(dirname "$(readlink -f "$0")")
out=${1:?usage: sweep.sh RESULTS.jsonl VARIANT...}; shift
REPS=${REPS:-6}
tag=${TAG:-run}
case $REPS in ''|*[!0-9]*|0) echo "sweep: REPS must be a positive number, not '$REPS'" >&2; exit 2;; esac
[ "$#" -gt 0 ] || { echo "sweep: name at least one variant" >&2; exit 2; }
for v in "$@"; do
  for rep in $(seq 1 "$REPS"); do
    if [ -f "$out" ] && jq -e --arg v "$v" --arg tag "$tag" --argjson rep "$rep" \
      'select(.variant == $v and .tag == $tag and .repetition == $rep)' "$out" >/dev/null 2>&1; then
      echo "sweep: $out already holds $tag $v repetition $rep — a new TAG or a new file" >&2
      exit 2
    fi
    for t in T1 T2 T3 T4; do
      # a repetition is its own run directory: a repeated tag must not overwrite
      # the attempt whose transcript explains it
      for try in 1 2; do
        line=$("$BENCH/run_one.sh" "$BENCH/bin-$v" "$t" "$BENCH/runs/$tag-$v-$t-r$rep")
        if printf '%s' "$line" | jq -e '.in_tokens' >/dev/null 2>&1; then break; fi
        sleep 3
      done
      printf '%s' "$line" | jq -c --arg v "$v" --arg tag "$tag" --arg rep "$rep" --arg attempt "$try" \
        '. + {variant: $v, tag: $tag, repetition: ($rep|tonumber), attempt: ($attempt|tonumber)}' >> "$out"
      echo "$v $t rep $rep done" >&2
    done
  done
done
