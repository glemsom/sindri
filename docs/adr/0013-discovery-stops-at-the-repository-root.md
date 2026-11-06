# Discovery stops at the repository root, found by `.git`

§7 says "never above the repository root" and leaves the root undefined. It is the nearest
directory holding a `.git` — a directory in a clone, a **file** in a worktree, which is why
the test is `exists()` and not `is_dir()`. A workspace inside no repository at all is its own
root, because the alternative is walking to `/` and handing a scratch directory whatever
skills happen to sit above it.

Precedence is resolved by visiting sources **lowest first** and letting a later skill take a
name: built-ins, then `~/.agents/skills`, then the `.agents/skills` directories from the root
down to the workspace. Every loss is pushed into `Catalog::shadowing` as data — name, winner
and loser by scope **and path**, because two directories of one scope can hold the same name
and a scope alone cannot tell them apart. `doctor` and the rail read that; nothing prints.

Frontmatter is YAML and must carry `name` and `description`. Any other key is ignored, so
§5's permission flags and Claude-compatible keys arrive without a schema change. A file that
will not parse becomes a `Diagnostic { path, line, message }` and the rest of the catalog
still loads; the parser's own "at line N" is stripped, since it counts from the frontmatter
and would contradict the file line the diagnostic already carries.

`serde_norway` parses it: the maintained fork of `serde_yaml`, same API, no `+deprecated`
in the lock.
