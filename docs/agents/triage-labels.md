# Triage Labels

The five roles triage speaks in, and the one label string each has in this repository's
tracker. They are stated here and nowhere else: `AGENTS.md` points at this file rather
than listing them a second time.

| Role              | Label              | Meaning                                  |
| ----------------- | ------------------ | ---------------------------------------- |
| `needs-triage`    | `needs-triage`     | Maintainer needs to evaluate this issue  |
| `needs-info`      | `needs-info`       | Waiting on reporter for more information |
| `ready-for-agent` | `ready-for-agent`  | Fully specified, ready for an AFK agent  |
| `ready-for-human` | `ready-for-human`  | Requires human implementation            |
| `wontfix`         | `wontfix`          | Will not be actioned                     |

Every role's label is the role's own name, so a skill asking for "the AFK-ready triage
label" means `ready-for-agent`. All five are in the tracker, and `gh label list` says so;
a role with no label behind it is a broken instruction, never a label to invent.
