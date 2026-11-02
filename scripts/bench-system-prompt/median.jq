# The median of one sweep file, as one pass of all four tasks:
#
#   jq -s -f median.jq results/<model>-<variant>.jsonl
#
# Each repetition of one tag is one complete pass, so its lines are summed into a
# single bill and the middle of those bills is printed — the two in the middle
# averaged when there is an even number of them. A run that never reached the
# model carries no `in_tokens` and is dropped: it is not a measurement, and a
# null would poison the sum rather than vanish. A file holding two variants is a
# mistake, not a spread, so it is refused instead of averaged.
map(select(.in_tokens != null))
| (map(.variant) | unique) as $variants
| if ($variants | length) > 1 then
    error("one variant per file: \($variants | join(", "))")
  else . end
| group_by([.tag, .repetition])
| map(map(.in_tokens) | add)
| sort
| length as $n
| ($n / 2 | floor) as $half
| if $n == 0 then null
  elif $n % 2 == 1 then .[$half]
  else (.[$half - 1] + .[$half]) / 2
  end
