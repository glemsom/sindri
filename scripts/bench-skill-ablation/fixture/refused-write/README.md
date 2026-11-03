# greet

A one-command greeting, in POSIX shell.

    $ bin/greet Ada
    Hello, Ada!

`src/greet.sh` holds the greeting, `src/lib.sh` says where the greeting log
lives, `bin/greet` is the front end, and the tests are `sh tests/run.sh`.

The log's directory is `/var/lib/greet`, hard-coded — which is why the suite
does not run: the sandbox mounts the host root read-only, so the directory the
writer makes is refused before the greeting is ever printed.
