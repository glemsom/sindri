---
name: search-and-read
description: "Finding and reading files: `cat`, a grep per guess and a whole file printed for one line are the defaults to correct — one call, filtered in the command, and only the bytes that answer."
---

# The fewest bytes that answer

A file is not the question. The question is one string, one symbol, one count or one
shape, and every one of those is a filter the command can apply before a byte of
output is written. The defaults cost turns and answer less: `cat` a file to see what
it holds, run another grep because the last was too broad, and print the whole file
because one line of it matched. `references/recipes.md` is the same move as a table,
one intent and one call for each.

## One intent, one call

Name the shape of the answer before reaching for a command, because the shape of the
answer is the shape of the question. Of the shapes below, the line number is the
cheapest, because it is what makes the next read a range instead of a file.

```sh
rg -n 'fn load' crates/core/src
```

The rest of the family is the same command with one flag changed. `rg -l` when the
answer is which files hold it, `rg -c` when it is how many times per file, `rg -w`
when a substring is too eager, `rg -F` when the pattern is literal, `rg --files -g
'*.rs'` when the answer is a listing, and `find` for what a search cannot express —
`-newer`, `-size`, `-perm`, and a path that may not exist yet.

Where the question is both the line and its surroundings, `-C 3` makes the search the
read: one call, and the second one is left for what could not be searched for.

## Push the filter into the command

A second pass over a tree is the default this skill is here to replace. `-g '*.rs'`
and `-t rust` cut the walk before it starts; `-m 1` stops at the first hit per file;
`-A 3`/`-B 3`/`-C 3` carry the context that would otherwise be a second call; and
`head`, `tail`, `sort -rn | head`, `uniq -c` narrow a stream that is already open,
which costs nothing. Two greps piped together are one regex away from a single pass —
write the regex.

## Open a range, and never a whole file

The line number a search returned is the range to read: open from a little before it
to a little after it, and nothing more.

```sh
sed -n '40,90p' crates/core/src/agent/skill.rs
```

A file's shape is cheaper than its body: `wc -l` says how long it is, `sed -n '1,40p'`
says what it is, and `rg -n '^(pub |fn |struct |impl |#)'` says how it is organised —
which is also the index a later range is chosen from. `cat` answers none of those and
spends the whole file to say nothing.

## Many inputs, one call

The same question about ten files is one loop in one call, and the loop prints its
conclusions rather than its working: the file and the verdict, never every line it
looked at. A loop that echoes what it read is a whole-file dump wearing the loop's
clothes. `references/many-files.md` is one such script end to end, with the shape to
copy.

Two calls in a row are the shape this skill replaces, and a run that finds itself
writing the second one has usually learned what it should have asked for first: fold
it back into the query, or into the one script, and run that.

## Say what you found

The answer names the file and the line and quotes the few lines that decided it. A
summary of a tree the reader cannot check is worth less than the one path that
answers, and a path with no line in it sends the reader back to the search the run
just paid for.
