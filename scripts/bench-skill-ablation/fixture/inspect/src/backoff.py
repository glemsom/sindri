"""How long a failed queue attempt waits before the next one.

The delay is the base delay doubled once per attempt already spent, and it stops
growing at the ceiling: an outage that lasts an afternoon costs the same wait per
attempt as one that lasts a minute, and a queue that backs off without a limit
never drains again.

The cap is a module constant rather than a setting, because a queue whose worst
case depends on its configuration is a queue whose worst case nobody can state. A
caller that needs another cap drives the instance it builds (`Policy`), and the
module's own value is what a caller that sets nothing gets.
"""

import random

# The delay of the first retry, in seconds. The second attempt waits twice this,
# the third four times, and so on, until the cap stops the growth.
BASE_DELAY = 0.25

# The most any one attempt waits: 0.25 * 2 ** 5 is 8.0, so the fifth retry is the
# first one the cap holds and every attempt after it waits the same. A test
# asserts that arithmetic, because the number is the one decision in this file.
_ceiling = 8.0

# The most attempts a single piece of work is retried before the loop gives up on
# it and hands it to the dead-letter log. Not the same thing as the cap above: the
# cap bounds one wait, this bounds the whole sequence of them.
MAX_ATTEMPTS = 12

# The share of the delay that may be taken back at random, so that a restarted
# queue's workers do not all retry in the same instant. A tenth is enough for the
# spread and small enough that the cap stays the worst case.
JITTER_SHARE = 0.1


def grown(base, attempt):
    """`base` doubled once per attempt already spent: the growth, with no cap in it.

    One function, because the module's delay and a policy's differ in their cap and
    in nothing else — two copies of a doubling are two places for the arithmetic to
    disagree with itself.
    """
    return base * (2 ** attempt)


def retry_delay(attempt):
    """The delay to wait before `attempt` is tried again, in seconds.

    `attempt` is the number of attempts already spent, so 0 is the first retry and
    a worker that has failed seven times asks for 7. The answer never exceeds the
    module's cap.
    """
    delay = grown(BASE_DELAY, attempt)
    if delay > _ceiling:
        return _ceiling
    return delay


def jittered(delay, spread=JITTER_SHARE, rng=None):
    """`delay` with up to `spread` of its length taken back at random.

    The wait is shortened rather than lengthened, so that the cap is still the
    longest a caller ever waits: a jitter that added to a delay would make the cap
    a lie every time it was applied.
    """
    chooser = rng or random.random
    return delay * (1.0 - spread * chooser())


def attempt_delay(attempt, rng=None):
    """The delay of one retry with the jitter applied, which is what the loop
    sleeps for: the two functions above, in the order the loop holds them."""
    return jittered(retry_delay(attempt), rng=rng)


def schedule(attempts, rng=None):
    """The whole sequence of waits one piece of work makes, one per attempt spent.

    The result is what a caller prints when it asks whether a retry policy is
    sensible: the first few delays grow, and the tail is flat at the cap.
    """
    return [attempt_delay(attempt, rng=rng) for attempt in range(attempts)]


def gave_up(attempt):
    """Whether the loop stops retrying this piece of work after `attempt`."""
    return attempt >= MAX_ATTEMPTS


def describe(attempt, rng=None):
    """One line saying what happens after `attempt` failures: the wait, or the fact
    that the work is handed on instead of retried."""
    if gave_up(attempt):
        return f"attempt {attempt}: dead-lettered after {MAX_ATTEMPTS} attempts"
    return f"attempt {attempt}: wait {attempt_delay(attempt, rng=rng):.3f}s"


class Policy:
    """The delays one queue's retries use, held as a value rather than read from
    the module.

    The module's constants are the policy a caller that builds nothing gets; this is
    the one a caller passes to the loop when it wants another base delay, a cap of
    its own, or a chooser it can predict in a test. The growth and the jitter are the
    module's, so a policy cannot disagree with it about either; the base and the cap
    are the instance's, and they are the whole of the difference.
    """

    def __init__(self, base=BASE_DELAY, cap=None, share=JITTER_SHARE, rng=None):
        self.base = base
        self.cap = _ceiling if cap is None else cap
        self.share = share
        self.rng = rng

    def delay(self, attempt):
        """This policy's wait before the attempt after `attempt` failures.

        A policy with a lower cap is a queue that gives up on waiting sooner, and one
        with a higher base is a queue that waits longer from the first retry.
        """
        delay = grown(self.base, attempt)
        if delay > self.cap:
            return self.cap
        return delay

    def wait(self, attempt):
        """This policy's wait with its own jitter applied, which is what the loop
        sleeps for."""
        return jittered(self.delay(attempt), spread=self.share, rng=self.rng)
