//! A terminal in raw mode sends `^C` as the byte `0x03` and no signal at all, so a key and a
//! `kill -INT` are two arrivals that must land on one flag.
//!
//! The handler writes one byte and does nothing else: anything that allocates, takes a lock
//! or reads a table is undefined behaviour inside a signal handler. The byte is why the loop
//! wakes at all — `poll` is never restarted by a signal.

use std::io;
use std::os::fd::AsRawFd;
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

use rustix::io::write as tell_fd;

const SIGINT: i32 = 2;

/// Its value means nothing: a reader counts readiness, not letters.
const PING: &[u8] = b"!";

/// A number because a handler may not take a lock: `-1` until a [`Wake`] is installed.
static HANDLER: AtomicI32 = AtomicI32::new(-1);

/// Never cleared: the launch ends once it is set.
static ARRIVED: AtomicBool = AtomicBool::new(false);

pub struct Wake {
    reader: OwnedFd,
    /// Shared, because a run thread and a signal handler both raise one and neither owns it.
    writer: Arc<OwnedFd>,
}

impl Wake {
    /// Both ends non-blocking: a handler must never block on a full pipe, and a reader that
    /// has drained everything must not block on a byte that is not coming.
    pub fn open() -> io::Result<Wake> {
        let (reader, writer) = std::io::pipe()?;
        let reader = OwnedFd::from(reader);
        let writer = OwnedFd::from(writer);
        nonblocking(&reader)?;
        nonblocking(&writer)?;
        Ok(Wake {
            reader,
            writer: Arc::new(writer),
        })
    }

    pub fn reader(&self) -> BorrowedFd<'_> {
        self.reader.as_fd()
    }

    pub fn raise(&self) {
        ping(self.writer.as_fd());
    }

    pub fn drain(&self) {
        let mut sink = [0u8; 64];
        while reads(self.reader(), &mut sink) > 0 {}
    }
}

pub struct Signals {
    wake: Arc<Wake>,
}

impl Signals {
    /// The handler is this process's own from here on: the default — the process dying with
    /// the terminal in raw mode and the alternate screen up — is the outcome avoided here.
    pub fn install() -> io::Result<Signals> {
        let wake = Arc::new(Wake::open()?);
        HANDLER.store(wake.writer.as_fd().as_raw_fd(), Ordering::SeqCst);
        let handler: extern "C" fn(i32) = interrupted;
        unsafe { signal(SIGINT, handler as usize) };
        Ok(Signals { wake })
    }

    pub fn wake(&self) -> Arc<Wake> {
        Arc::clone(&self.wake)
    }

    pub fn arrived(&self) -> bool {
        ARRIVED.load(Ordering::SeqCst)
    }
}

/// One `write` of one byte and the flag: anything else here is undefined behaviour.
extern "C" fn interrupted(_signal: i32) {
    ARRIVED.store(true, Ordering::SeqCst);
    let fd = HANDLER.load(Ordering::SeqCst);
    if fd >= 0 {
        // `HANDLER` is never cleared, so the fd dangles once the last `Arc<Wake>` drops: this
        // borrow is sound only while a wake is alive.
        ping(unsafe { BorrowedFd::borrow_raw(fd) });
    }
}

unsafe extern "C" {
    fn signal(signum: i32, handler: usize) -> usize;
}

/// Best effort: a pipe already full is a wake already waiting.
fn ping(fd: BorrowedFd<'_>) {
    let _ = tell_fd(fd, PING);
}

fn reads(fd: BorrowedFd<'_>, sink: &mut [u8]) -> usize {
    rustix::io::read(fd, sink).unwrap_or(0)
}

fn nonblocking(fd: &OwnedFd) -> io::Result<()> {
    rustix::fs::fcntl_setfl(fd, rustix::fs::OFlags::NONBLOCK)?;
    Ok(())
}
