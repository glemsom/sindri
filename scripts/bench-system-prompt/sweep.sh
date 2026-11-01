#!/bin/bash
# sweep.sh RESULTS.jsonl VARIANT... — every task for each variant, one JSON line
# per run, appended to RESULTS.jsonl. A run that never reached the model is not a
# measurement, so it is tried once more before it is recorded.
set -u
BENCH=$(dirname "$(readlink -f "$0")")
out=$1; shift
: > "$out"
for v in "$@"; do
  for t in T1 T2 T3 T4; do
    for try in 1 2; do
      line=$("$BENCH/run_one.sh" "$BENCH/bin-$v" "$t" "$BENCH/runs/${TAG:-run}-$v-$t")
      if printf '%s' "$line" | jq -e '.in_tokens' >/dev/null 2>&1; then break; fi
      sleep 3
    done
    printf '%s' "$line" | jq -c --arg v "$v" --arg n "$try" '. + {variant: $v, attempt: ($n|tonumber)}' >> "$out"
    echo "$v $t done" >&2
  done
done
