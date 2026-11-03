---
name: edit-files
description: "Changing part of a file: the whole-file rewrite loses what surrounds the change, so read the anchor, assert it once, and let the tool's own answer stand."
---

## File Editing

- For focused edits, consult the `search-and-read-files` skill first: an edit is
  only as good as the anchor it is written against.
- **Edit:** inline Python with `Path.read_text()` / `Path.write_text()`; assert
  the anchor appears exactly once (`assert count == 1`). On `AssertionError`,
  re-read the fresh file first — a partial write may have landed.
- **New files / rewrites:** `cat <<'EOF' > file` heredocs.
- **Many edits:** one script per edit, run sequentially.
