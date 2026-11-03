# A denied write, end to end

A small repository, `greet`, whose tests fail the moment they run:

    $ sh tests/run.sh
    mkdir: cannot create directory '/var/lib/greet': Read-only file system
    FAIL: greet Ada printed

`src/lib.sh` says where the greeting log lives, and it says so in the one way
that cannot be moved:

    state_dir() {
        echo /var/lib/greet
    }

`src/greet.sh` makes that directory and appends a name to a file in it, and the
suite asserts the file has the name in it. Three turns, and the third is the one
that matters — the first two are what the default does.

## The default, which is three turns of nothing

    $ sudo mkdir -p /var/lib/greet          # no sudo in here; refused
    $ mkdir -p /var/lib/greet 2>/dev/null || true; sh tests/run.sh
    FAIL: greet Ada printed                 # the writer is still losing the name

and then, if the run is unlucky, the fourth turn: the assertion in `tests/run.sh`
deleted, so the suite prints `all tests pass` over a `greet` that writes
nothing. The exit code is 0 and the answer is a lie, which is worse than the
failure it replaced.

## The move

Ask where writing works, rather than guessing:

    $ echo "$TMPDIR"; pwd
    /home/you/.local/state/sindri/sessions/4f1c…/tmp
    /home/you/src/greet

The state the run is refusing is scratch: the next test run does not miss it, and
the suite never looks for it after it has read it. So it belongs in `$TMPDIR` —
but the path must come from the environment and not from this run's memory,
because the suite is what will read it:

    state_dir() {
        echo "${GREET_STATE:-${TMPDIR:-/tmp}/greet}"
    }

Then the proof, which is the same command as the failure:

    $ sh tests/run.sh
    all tests pass

The diff is two lines in `src/lib.sh` and nothing else, and the answer says what
the run has to say: `/var/lib/greet` was denied, the log now lives under
`${TMPDIR:-/tmp}/greet`, and the suite passes.

## Why the parameter and not the workaround

    GREET_STATE=$TMPDIR/greet sh tests/run.sh

is one line and it is enough for this run. It is not enough for the repository:
the suite still fails wherever the variable is not set by hand — a fresh
checkout, CI, or the next agent. The parameter is the same insight kept, and the
test that runs after the run is the one that grades which of the two was done.
