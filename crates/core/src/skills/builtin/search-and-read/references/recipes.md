# The recipes, one intent and one call

Read down the left column to the intent, and take the call on the right as it
stands. Every one of them is a single command: the filter is in the command, so
the output is the answer and not the material an answer is cut out of.

## Finding

| the answer is | the call |
| --- | --- |
| the line a string is on | `rg -n 'pattern' path` |
| which files hold it | `rg -l 'pattern' path` |
| how many times, per file | `rg -c 'pattern' path` |
| a whole-word, literal match | `rg -nwF 'a.b' path` |
| the definitions of one name | `rg -n '^(pub )?(fn|struct|enum|impl|trait) Name\b' .` |
| which files exist | `rg --files -g '*.rs' path` |
| a path a search cannot express | `find path -name '*.log' -newer marker -size +1M` |
| the first hit per file only | `rg -n -m 1 'pattern' path` |

## Reading

| the answer is | the call |
| --- | --- |
| the lines around a hit | `sed -n '38,44p' file` |
| the head of a file | `sed -n '1,40p' file` |
| how long a file is | `wc -l file` |
| the outline of a file | `rg -n '^(pub |fn |struct |impl |#)' file` |
| the shape of a directory | `find path -maxdepth 1 -type f -printf '%s\t%p\n' | sort -rn | head` |
| whether a name is used at all | `rg -lw 'name' .` |

## Narrowing what you already have

A stream that is already open is the cheapest thing there is to narrow, and
`head`/`tail`/`sort -rn | head`/`uniq -c` on it are one pass:

```sh
cargo test 2>&1 | rg -n 'FAILED|panicked' | head -20
```

## The two this table does not hold

`cat file` is the one call with no answer in it: it prints the whole file to ask a
question the table has a cheaper row for. And two greps over one tree are one grep
with a better pattern, or one `rg` with `-g` to cut the walk — a second pass is the
cost the first pass already paid.
