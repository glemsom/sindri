---
name: search-and-read-files
description: "Finding and reading files: `cat`, a grep per guess and a whole file printed for one line are the defaults to correct — one call, filtered in the command, and only the bytes that answer."
---

## File Inspection

- **Find:** `rg -l <pattern>` locates files; `rg -n --heading --color=never`
  views matching lines.
- **Read:** the shape returning the least that still tells the whole truth. A
  menu, not a rulebook: a range (`sed -n 'A,Bp'`); a symbol with context
  (`rg -n 'Foo' -A 25 -B 5`); a unit to its closing brace
  (`sed -n '/^func Foo/,/^}/p'`); numbered coordinates
  (`nl -ba <file> | sed -n 'X,Yp'`) when exact lines matter. Size first
  (`wc -l`) before a whole-file read; `--stat` / `--name-only` before a full
  diff.
- **Filter as you read:** push the predicate into the command rather than
  reading then discarding — drop blanks, banners, imports, and generated or
  vendored blocks you weren't asked about; keep comments that are the content
  (doc strings, rationale).
