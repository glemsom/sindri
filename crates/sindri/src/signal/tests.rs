//! The reasons a wake carries, and what one readiness behind it stands for.

use super::*;

/// Far past the capacity a pipe of this process is opened with.
const PINGS: usize = 1 << 18;

#[test]
fn a_cancel_raised_twice_and_drained_once_is_still_read() {
    let wake = Wake::open().expect("a wake");

    wake.raise(Reason::Cancel);
    wake.raise(Reason::Cancel);
    wake.drain();

    assert!(
        wake.reasons().holds(Reason::Cancel),
        "a cancel is a flag and not the byte: a drain that took the pings takes nothing of it"
    );
}

#[test]
fn a_cancel_is_read_though_the_pipe_it_pinged_was_full() {
    let wake = Wake::open().expect("a wake");
    for _ in 0..PINGS {
        wake.ping();
    }

    wake.raise(Reason::Cancel);
    wake.drain();

    assert!(
        wake.reasons().holds(Reason::Cancel),
        "a byte nobody could write is a wake that failed, so the reason raised with it cannot be \
         the byte's to lose"
    );
}

#[test]
fn two_resizes_behind_one_readiness_are_one_read() {
    let wake = Wake::open().expect("a wake");
    wake.raise(Reason::Resize);
    wake.raise(Reason::Resize);

    let first = wake.reasons();
    let second = wake.reasons();

    assert!(first.holds(Reason::Resize), "the readiness is a resize");
    assert!(
        !second.holds(Reason::Resize),
        "and one repaint answers both, so the next read has no resize left to paint"
    );
}

#[test]
fn a_cancel_and_a_resize_behind_one_readiness_are_read_apart() {
    let wake = Wake::open().expect("a wake");
    wake.raise(Reason::Cancel);
    wake.raise(Reason::Resize);

    let both = wake.reasons();
    let after = wake.reasons();

    assert!(
        both.holds(Reason::Cancel) && both.holds(Reason::Resize),
        "one readiness stands for both of the reasons raised behind it"
    );
    assert!(
        after.holds(Reason::Cancel),
        "the cancel is the launch's and outlives the read that took it"
    );
    assert!(
        !after.holds(Reason::Resize),
        "and the resize was taken by that read"
    );
}
