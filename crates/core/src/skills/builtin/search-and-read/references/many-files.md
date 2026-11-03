# Many files, one call

The same question about many inputs is one script in one call. The script loops,
decides, and prints only its conclusions; the run pays for the answer and not for
the material the answer was cut from.

## The shape

```sh
for f in $(rg -l --no-messages 'TODO' src); do
  n=$(rg -c 'TODO' "$f")
  printf '%s\t%s\n' "$n" "$f"
done | sort -rn | head
```

Three things make that one call rather than ten.

The list comes from the search, not from a glob the shell expands and not from a
`find` piped into `xargs` in a second stage: `rg -l` already knows which files hold
the thing, and a file that does not hold it is a file the loop never opens.

The body of the loop prints a line, not a transcript. `echo "checking $f"`, or the
match itself for every match, is a whole-file dump with a loop around it, and the
reader then has to do the counting the script was for.

The exit status is the loop's own, so `--no-messages` matters: a missing directory
should not turn the last stage's status into a failure the caller reads as the
answer being wrong.

## The same question over a listing

When the inputs are not found by a search, they are still one loop:

```sh
for f in $(git ls-files '*.rs'); do
  printf '%s\t%s\n' "$(rg -c 'unwrap' "$f" || echo 0)" "$f"
done | sort -rn | head -5
```

`rg -c` exits 1 where a file holds no match, so the `|| echo 0` keeps that file's
row in the table instead of silently dropping the files that are clean — the ones a
reader most needs to be able to count.

## Where it pays

A loop pays when the answer is a table, a count or a set of verdicts, one row per
input. It does not pay when the answer is one hit in one file: that is a single
`rg -n`, and wrapping it in a loop adds a layer to read. The rule is the shape of the
answer, not the number of files.
