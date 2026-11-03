"""The named limits a queue is configured with.

Every queue is built from one name, and the name is the whole configuration: a
caller that spells a name the registry does not know is refused rather than given a
default, because a default rate is a rate nobody chose.
"""

from dataclasses import dataclass


@dataclass(frozen=True)
class Limit:
    """One queue's limit: how many pieces of work it may start per second, and how
    many workers it may hold open."""

    rate: float
    workers: int


# The names a queue may be built from. The rates are in pieces of work per second
# and the worker counts are what the dispatch loop keeps open; both are
# deliberately small, because the queue's job is to be the patient one.
LIMITS = {
    "interactive": Limit(rate=20.0, workers=4),
    "batch": Limit(rate=2.0, workers=8),
    "backfill": Limit(rate=0.25, workers=2),
    "repair": Limit(rate=1.0, workers=1),
}

# The name a queue gets when its caller names nothing: the one gentle enough to be
# safe in front of a service someone is already using.
DEFAULT = "interactive"


def limit_for(name, *, default=None):
    """The limit `name` stands for, or the refusal that names what is on offer.

    A name the registry does not hold raises rather than falling back: a queue
    silently built from the wrong limits is a queue whose rate nobody can predict
    from its own configuration.
    """
    known = LIMITS.get(name)
    if known is not None:
        return known
    if default is not None:
        return LIMITS[default]
    raise KeyError(
        f"no queue named {name!r}: the registry holds " + ", ".join(sorted(LIMITS))
    )


def names():
    """Every name a caller may ask for, in the order a help text lists them."""
    return sorted(LIMITS)


def rate_of(name):
    """One number out of the registry, for the loop that has to pace itself."""
    return limit_for(name).rate


def spacing(name):
    """The seconds between two starts of one queue, which is the reciprocal of its
    rate: the loop sleeps for this and starts nothing while it sleeps."""
    return 1.0 / rate_of(name)
