# Cancellation beats every dial; otherwise the first trigger wins

A run can hit its turn cap, exhaust a budget and receive a cancel in any order, sometimes within
the same turn. The rule: **cancellation always wins**, because it is the user's word; otherwise
**whichever trigger fired first in time wins**. Two guards make it unambiguous: the runner never
starts a turn once a budget is exhausted, so `budget` cannot hide behind `max_turns`; and a turn
that produced a complete answer stops as `final`, so the cap never steals a success.

Exit codes follow the stop reason from one mapping table in core. Neither front end gets a second
opinion, because a second opinion is how a script sees exit 0 on a failed run.
