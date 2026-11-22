//! A terminal in raw mode sends `^C` as the byte `0x03` and no signal at all, so a key and a
//! `kill -INT` are two arrivals that land on one flag. The handler sets the cause and writes one
//! byte and nothing else: anything that allocates, takes a lock or reads a table is undefined there.

use std::io;
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::sync::Arc;
use std::sync::atomic::{AtomicPtr, AtomicU8, Ordering};

use rustix::io::write as tell_fd;

const SIGINT: i32 = 2;

/// Its value means nothing: a reader counts readiness, not letters.
const PING: &[u8] = b"!";

/// The wake a handler pings, because a handler may not take a lock: null until one is installed,
/// and leaked there, because a signal reads it for as long as the process runs.
static HANDLER: AtomicPtr<Wake> = AtomicPtr::new(std::ptr::null_mut());

/// Why a wake was raised, as a set: a cancel and a window change can both be waiting behind one
/// readiness. A report that lands carries none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Causes(u8);

impl Causes {
    pub const CANCEL: Causes = Causes(0b01);
    pub const RESIZE: Causes = Causes(0b10);

    /// Whether `cause` is among them.
    pub fn holds(self, cause: Causes) -> bool {
        self.0 & cause.0 != 0
    }
}

/// The byte is why the loop wakes at all: `poll` is never restarted by a signal, so a pipe with
/// a byte in it is a readiness the next `poll` returns.
pub struct Wake {
    reader: OwnedFd,
    /// Shared, because a run thread and a signal handler both raise one and neither owns it.
    writer: Arc<OwnedFd>,
    /// A cancel is never cleared, because a launch ends on it; a resize is taken by the read that
    /// paints it, so a burst of them is one repaint.
    raised: AtomicU8,
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
            raised: AtomicU8::new(0),
        })
    }

    /// The handler is this process's own from here on: the default — the process dying with
    /// the terminal in raw mode and the alternate screen up — is the outcome avoided here.
    pub fn installed() -> io::Result<Arc<Wake>> {
        let wake = Arc::new(Wake::open()?);
        // Leaked, because the pointer is never cleared: a cause freed under a signal is one no
        // handler may set.
        HANDLER.store(
            Arc::into_raw(Arc::clone(&wake)).cast_mut(),
            Ordering::SeqCst,
        );
        let handler: extern "C" fn(i32) = interrupted;
        unsafe { signal(SIGINT, handler as usize) };
        Ok(wake)
    }

    pub fn reader(&self) -> BorrowedFd<'_> {
        self.reader.as_fd()
    }

    /// A report that landed, and no cause with it: the loop takes the report and nothing else.
    pub fn ping(&self) {
        ping(self.writer.as_fd());
    }

    /// The cause first and the byte after, so a loop woken by that byte reads the cause it was
    /// raised with.
    pub fn raise(&self, cause: Causes) {
        self.raised.fetch_or(cause.0, Ordering::SeqCst);
        ping(self.writer.as_fd());
    }

    /// What the loop reads every pass: the cancel is a flag no drain can take away, and the resize
    /// is taken here, so two behind one readiness are one repaint.
    pub fn causes(&self) -> Causes {
        Causes(self.raised.fetch_and(!Causes::RESIZE.0, Ordering::SeqCst))
    }

    pub fn drain(&self) {
        let mut sink = [0u8; 64];
        while reads(self.reader(), &mut sink) > 0 {}
    }
}

/// One `write` of one byte, after the cause: anything else here is undefined behaviour.
extern "C" fn interrupted(_signal: i32) {
    // The wake a handler points at is leaked, so it answers for as long as the process runs.
    if let Some(wake) = unsafe { HANDLER.load(Ordering::SeqCst).as_ref() } {
        wake.raise(Causes::CANCEL);
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

#[cfg(test)]
mod tests;
