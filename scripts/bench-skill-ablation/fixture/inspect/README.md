# throttle

Rate limiting and retry for a dispatch queue, in Python and nothing else: the queue
hands work to a worker, the worker asks `src/backoff.py` how long to wait after a
failure, and `src/registry.py` keeps the named limits a queue is configured with.

    $ python3 -m unittest discover -s tests -q

## Where the retry delay comes from

`retry_delay(attempt)` doubles the base delay once per attempt already spent and is
not allowed to grow past `_ceiling`: a queue that backs off for ever stops draining
after the first afternoon of failures, and one that gives up at once never waits out
a restart. The cap is the one line of the file that decides what a long outage
costs, so it is the line to read before changing anything here.

## The rest

`src/dispatch.py` is the loop: it takes work, runs it, and sleeps for what the
backoff module says. `src/registry.py` holds the limits a queue is configured with
and refuses a name it does not know, which is the one rule the loop relies on.
