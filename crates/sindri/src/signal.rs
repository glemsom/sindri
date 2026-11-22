//! A terminal in raw mode sends `^C` as the byte `0x03` and no signal at all, so a key and a
//! `kill -INT` are two arrivals that land on one flag. The handler writes one byte and nothing
//! else: anything that allocates, takes a lock or reads a table is undefined behaviour there.

use std::io;
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};

use rustix::io::write as tell_fd;

const SIGINT: i32 = 2;

/// Its value means nothing: a reader counts readiness, not letters.
const PING: &[u8] = b"!";

/// The wake this process's handler pings: null until one is installed, because a handler may not
/// take a lock.
static HANDLER: AtomicPtr<Shared> = AtomicPtr::new(std::ptr::null_mut());

/// Why a wake was raised. A report that lands carries no reason.
#[derive(Clone, Copy, Debug)]
pub enum Reason {
    Cancel,
    Resize,
}

/// What one readiness was raised with: a cancel stays, a resize is taken by the read that paints it.
#[derive(Clone, Copy, Debug)]
pub struct Reasons {
    cancel: bool,
    resize: bool,
}

impl Reasons {
    pub fn holds(self, reason: Reason) -> bool {
        match reason {
            Reason::Cancel => self.cancel,
            Reason::Resize => self.resize,
        }
    }
}

/// The reason state belongs to one wake: a run's thread, a wire's thread and the handler all raise
/// on it, and no two launches share a flag.
struct Shared {
    writer: OwnedFd,
    cancel: AtomicBool,
    resize: AtomicBool,
}

impl Shared {
    /// The reason first and the byte after, so a loop woken by that byte reads the reason it was
    /// raised with.
    fn raise(&self, reason: Reason) {
        match reason {
            Reason::Cancel => self.cancel.store(true, Ordering::SeqCst),
            Reason::Resize => self.resize.store(true, Ordering::SeqCst),
        }
        ping(self.writer.as_fd());
    }
}

/// The byte is why the loop wakes at all: `poll` is never restarted by a signal, so a pipe with
/// a byte in it is a readiness the next `poll` returns.
pub struct Wake {
    reader: OwnedFd,
    raised: Arc<Shared>,
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
            raised: Arc::new(Shared {
                writer,
                cancel: AtomicBool::new(false),
                resize: AtomicBool::new(false),
            }),
        })
    }

    /// The handler is this process's own from here on: the default — the process dying with
    /// the terminal in raw mode and the alternate screen up — is the outcome avoided here.
    pub fn installed() -> io::Result<Arc<Wake>> {
        let wake = Arc::new(Wake::open()?);
        HANDLER.store(Arc::as_ptr(&wake.raised).cast_mut(), Ordering::SeqCst);
        let handler: extern "C" fn(i32) = interrupted;
        unsafe { signal(SIGINT, handler as usize) };
        Ok(wake)
    }

    pub fn reader(&self) -> BorrowedFd<'_> {
        self.reader.as_fd()
    }

    /// A report that landed, and no reason with it: the loop reads the report and nothing else.
    pub fn ping(&self) {
        ping(self.raised.writer.as_fd());
    }

    /// A run's thread and the handler are the launch's own raisers; nothing else raises a reason on
    /// the wake a launch waits on, so only a test reaches this.
    #[cfg(test)]
    pub fn raise(&self, reason: Reason) {
        self.raised.raise(reason);
    }

    /// The cancel is never cleared, because a launch ends once it is raised and so cannot lose it to
    /// a pipe already full or to a read that already happened. The resize is taken here, so two
    /// resizes behind one readiness are one repaint.
    pub fn reasons(&self) -> Reasons {
        Reasons {
            cancel: self.raised.cancel.load(Ordering::SeqCst),
            resize: self.raised.resize.swap(false, Ordering::SeqCst),
        }
    }

    pub fn drain(&self) {
        let mut sink = [0u8; 64];
        while reads(self.reader(), &mut sink) > 0 {}
    }
}

/// One `write` of one byte and the flag: anything else here is undefined behaviour.
extern "C" fn interrupted(_signal: i32) {
    // `HANDLER` is never cleared, so the pointer dangles once the last `Arc<Wake>` drops: this
    // borrow is sound only while a wake is alive.
    let Some(shared) = (unsafe { HANDLER.load(Ordering::SeqCst).as_ref() }) else {
        return;
    };
    shared.raise(Reason::Cancel);
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
