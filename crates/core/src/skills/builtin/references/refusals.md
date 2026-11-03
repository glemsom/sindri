# The refusals, and the move each one wants

The first line of the error is the diagnosis. Read it before the next call, and
match it here rather than reaching for the move that worked last time — the
messages below are two different problems that read alike, and a run that treats
`Permission denied` as a boundary spends its turns on the wrong side of the one
it was given.

## Writes

| the message | what it names | the move |
| --- | --- | --- |
| `Read-only file system` (EROFS) | a root this run may read and not write | retarget the artefact into a writable root, or make the path a parameter; never retry, never escalate |
| `Permission denied` (EACCES) on a path inside a writable root | that file's or directory's own mode | `ls -l` it, then write what you are allowed to write; `chmod` only on a path you own, and only what you need |
| `Permission denied` on a bind, a device or `/proc/sys` | the kernel, or a namespace this run is not in | not yours to change: solve it another way or say it cannot be done |
| `No such file or directory` (ENOENT) | a missing parent, or a name that was never there | `mkdir -p` the parent inside a writable root; an absolute path copied out of documentation usually has one |
| `Text file busy` (ETXTBSY) | a binary that is still running | build to a new name and move it into place |
| `Disk quota exceeded` / `No space left on device` | the writable root is small | keep scratch in `$TMPDIR` and clean up as you go |
| `Too many open files` | this process's own limit | close what you opened; it is not a permission |

## The network and its credentials

| the message | what it names | the move |
| --- | --- | --- |
| `Could not resolve host` | name resolution | once is enough: the name is wrong, or nothing here answers it |
| `Connection refused` / `Network is unreachable` | nothing listening, or no route | once; then work from what is on disk |
| `401 Unauthorized` | a request with no credential, or a wrong one | no retry helps: the answer is the human's to give |
| `403 Forbidden` | a request understood and refused | no retry helps either |
| `407 Proxy Authentication Required` | a proxy that wants a credential | same |
| `SSL certificate problem` | a certificate the run cannot verify in time | do not disable verification to get past it; that is a boundary traded for a build |
| `502` / `503` / `504` | the far side is unwell | one retry after a pause is defensible, three is a loop |

## What a refusal is not

`exit_code: 1` is a command that decided to fail, and the message says which of
the tables above it belongs to. An outcome of `sandbox_error` belongs to neither
table: no path and no host was refused, so nothing in the command should be
retargeted — say what happened and stop, rather than rewriting a path that was
never the problem.
