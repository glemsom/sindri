use std::os::fd::AsFd;
use std::time::Duration;

use rustix::event::{PollFd, PollFlags, Timespec, poll};

use super::*;

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

/// `SIG_IGN` as `signal()` names it: the report is the reader's to ignore, and a launch that found it
/// ignored has to leave it that way.
const SIG_IGN: usize = 1;

fn sent_window_change() {
    rustix::process::kill_process(rustix::process::getpid(), rustix::process::Signal::WINCH)
        .expect("the window change is delivered");
}

fn awakened(wake: &Wake) {
    let wait = Timespec::try_from(Duration::from_secs(5)).ok();
    let reader = wake.reader();
    let mut fds = [PollFd::new(&reader, PollFlags::IN)];
    poll(&mut fds, wait.as_ref()).expect("the wait on the wake");
    assert!(
        fds[0].revents().contains(PollFlags::IN),
        "the signal reached the pipe a launch waits on, so a resize is caught at all"
    );
}

#[test]
fn a_window_change_is_caught_beside_the_cancel_and_a_burst_of_them_is_one_read() {
    let _installing = one_at_a_time();
    let wake = Wake::installed().expect("a wake");

    sent_window_change();
    awakened(&wake);
    wake.drain();
    sent_window_change();
    awakened(&wake);

    let first = wake.causes();
    let second = wake.causes();

    assert!(
        first.holds(Causes::RESIZE),
        "a window manager's report is a cause a launch reads, so an idle frame is painted at the \
         size the terminal now holds"
    );
    assert!(
        !second.holds(Causes::RESIZE),
        "however many reports arrived behind one readiness: a report is a repaint, and one repaint \
         answers all of them"
    );
    give_signals_back();
}

#[test]
fn the_screen_gives_the_signals_back_where_it_found_them() {
    let _installing = one_at_a_time();
    let _one_screen = crate::term::lock(&crate::term::tests::ONE_SCREEN);
    let found = unsafe { signal(SIGWINCH, SIG_IGN) };
    let wake = Wake::installed().expect("a wake");
    sent_window_change();
    awakened(&wake);
    assert!(
        wake.causes().holds(Causes::RESIZE),
        "a launch that holds the terminal catches the report while it holds it"
    );

    let pty = crate::term::pty::Pty::open();
    let terminal = crate::term::Terminal::pair(pty.slave.as_fd(), pty.slave.as_fd())
        .expect("a terminal to draw on");
    drop(crate::term::Screen::take(&terminal, false, false).expect("the screen is taken"));

    let left = unsafe { signal(SIGWINCH, found) };
    assert_eq!(
        left, SIG_IGN,
        "the screen gives the signals back where it found them, so a launch that began on a \
         terminal that ignored the report leaves it ignored, and never a handler pointing at a \
         pipe that is gone"
    );
}
