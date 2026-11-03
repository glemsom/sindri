"""The loop: take one piece of work, run it, and sleep for what backoff says.

The loop owns no policy of its own. How long to wait after a failure is
`src/backoff.py`'s, how fast to start work is `src/registry.py`'s, and what happens
to work that has failed too often is a dead-letter row rather than another attempt.
Keeping the three apart is what makes each of them readable on its own: the loop is
the one place they are read together.
"""

import time

from src import backoff
from src.registry import limit_for, spacing


class DeadLetter(Exception):
    """Raised by `drain` when a piece of work has failed as often as the policy
    allows: the work is logged and the loop moves on to the next one."""


class Loop:
    """One queue: the name it was built from, the work it is given, and what it
    did with it.

    A loop is a value a caller holds for a run of the program, so it carries the
    facts a reader wants afterwards — how many pieces of work it started, how many
    waits it made and how long it slept — rather than printing them as it goes.
    """

    def __init__(self, queue, run, sleep=None, rng=None):
        self.queue = queue
        self.limit = limit_for(queue)
        self.run = run
        self.sleep = sleep or time.sleep
        self.rng = rng
        self.started = 0
        self.failed = 0
        self.slept = 0.0

    def pace(self):
        """Sleep for the queue's spacing, which is what keeps the rate honest."""
        wait = spacing(self.queue)
        self.sleep(wait)
        self.slept += wait

    def one(self, work):
        """Run one piece of work, retrying it as the policy says.

        The attempt counter is the number of attempts already spent, so the first
        run asks the backoff module for 0 and the wait after a failure is the wait
        before the next attempt. Work that has failed as often as the policy allows
        raises `DeadLetter` rather than being retried again.
        """
        attempt = 0
        while True:
            self.started += 1
            try:
                return self.run(work)
            except Exception as problem:  # noqa: BLE001 - every failure is retried
                attempt += 1
                self.failed += 1
                if backoff.gave_up(attempt):
                    raise DeadLetter(
                        f"{work!r} failed {attempt} times, last: {problem}"
                    ) from problem
                wait = backoff.attempt_delay(attempt, rng=self.rng)
                self.sleep(wait)
                self.slept += wait

    def drain(self, work):
        """Run everything the queue is given, in order, and answer the dead letters
        rather than raising on the first one: a piece of work the policy gives up
        on is collected, and the loop moves to the next."""
        dead = []
        for piece in work:
            self.pace()
            try:
                self.one(piece)
            except DeadLetter:
                dead.append(piece)
        return dead
