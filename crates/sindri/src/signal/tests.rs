//! The causes a wake carries: a cancel that outlives the drain behind it and a pipe already full,
//! and a window change that one repaint answers however many arrived.

use super::*;

/// Far past the capacity a pipe of this process is opened with.
const PINGS: usize = 1 << 18;

#[test]
fn a_cancel_raised_twice_and_drained_once_is_still_read() {
    let wake = Wake::open().expect("a wake");

    wake.raise(Causes::CANCEL);
    wake.raise(Causes::CANCEL);
    wake.drain();

    assert!(
        wake.causes().holds(Causes::CANCEL),
        "a cancel is a flag and not the byte: a drain that took both pings takes nothing of it"
    );
}

#[test]
fn a_cancel_is_read_though_the_pipe_it_pinged_was_full() {
    let wake = Wake::open().expect("a wake");
    for _ in 0..PINGS {
        wake.ping();
    }

    wake.raise(Causes::CANCEL);
    wake.drain();

    assert!(
        wake.causes().holds(Causes::CANCEL),
        "a byte nobody could write is a wake that failed, so the cause raised with it cannot be \
         the byte's to lose"
    );
}

#[test]
fn two_resizes_behind_one_readiness_are_one_read() {
    let wake = Wake::open().expect("a wake");

    wake.raise(Causes::RESIZE);
    wake.raise(Causes::RESIZE);

    let first = wake.causes();
    let second = wake.causes();

    assert!(first.holds(Causes::RESIZE), "the readiness is a resize");
    assert!(
        !second.holds(Causes::RESIZE),
        "and one repaint answers both, so the next read has no resize left to paint"
    );
}

#[test]
fn a_cancel_and_a_window_change_behind_one_readiness_are_read_apart() {
    let wake = Wake::open().expect("a wake");

    wake.raise(Causes::RESIZE);
    wake.raise(Causes::CANCEL);

    let both = wake.causes();
    let after = wake.causes();

    assert!(
        both.holds(Causes::CANCEL) && both.holds(Causes::RESIZE),
        "one readiness stands for both of the causes raised behind it"
    );
    assert!(
        after.holds(Causes::CANCEL),
        "the cancel is the launch's and outlives the read that took it"
    );
    assert!(
        !after.holds(Causes::RESIZE),
        "and the window change was taken by that read"
    );
}
