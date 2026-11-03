# Issue tracker: GitHub

Issues and specs live as GitHub issues. Use the `gh` CLI for everything.

## Conventions

- **Create**: `gh issue create --title "..." --body "..."`; a heredoc for multi-line
  bodies.
- **Read**: `gh issue view <n> --json number,title,body,labels,comments`.
- **List**: `gh issue list --state open --json number,title,body,labels,comments --jq
  '[.[] | {number, title, body, labels: [.labels[].name], comments: [.comments[].body]}]'`,
  with `--label` and `--state` as needed.
- **Sub-issue**: `gh issue create --parent <parent> …`, or `gh issue edit <parent>
  --add-sub-issue <child>`. Without sub-issues, `Part of #<parent>` at the top of the body.
- **Blocking**: native dependencies — `gh issue create --blocked-by <n>,<n>` or `gh issue
  edit <child> --add-blocked-by <blocker>`. Older `gh`: `gh api --method POST
  repos/<owner>/<repo>/issues/<child>/dependencies/blocked_by -F issue_id=<db-id>`, where
  the id is `gh api repos/<owner>/<repo>/issues/<n> --jq .id`. A ticket is unblocked when
  every blocker is closed.
- **Open blockers**: `gh api repos/<owner>/<repo>/issues/<n> --jq
  .issue_dependencies_summary.blocked_by` (0 means startable). `gh issue view <n> --json
  blockedBy` lists closed ones too, so filter:
  `--jq '[.blockedBy.nodes[] | select(.state == "OPEN")] | length'`.
- **Comment**: `gh issue comment <n> --body "..."`.
- **Labels**: `gh issue edit <n> --add-label "..."` / `--remove-label "..."`.
- **Close**: comment the explanation, then `gh issue close <n>`. On a closed issue,
  `--comment` drops the comment and still exits 0.

`gh` infers the repo from `git remote -v`. File only into `glemsom/sindri`: in a worktree
of a fork `gh` can default to the upstream parent, so precede a session's first create with
`gh repo view --json nameWithOwner` and say so if it names anything else.

## Pull requests

A PR is a change to review and merge, not a ticket: nothing triages it, and a PR asking for
work is answered by an issue. GitHub shares one number space across issues and PRs, so a
bare `#42` may be either: resolve with `gh pr view 42`, falling back to `gh issue view 42`.

## When a skill says "publish to the issue tracker"

Create a GitHub issue. "Fetch the relevant ticket" is **Read** above.
