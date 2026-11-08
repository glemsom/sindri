//! The one door an outside `SIGINT` comes through: a handler that writes one byte to a
//! pipe, and the read end the key loop waits on beside the terminal, so the user's own
//! word arrives where a keystroke does and the terminal it interrupts is given back rather
//! than left raw and blank (§4, §12).
//!
//! Raw mode is what makes this two doors instead of one: a terminal in raw mode sends `^C`
//! as the byte `0x03` and no signal at all, so a key and a `kill -INT` are different
//! arrivals that must land in the same place. They do — the key loop sets one flag when it
//! decodes the byte and the same flag when this pipe says a signal came — and that flag is
//! the core's own cancellation, which the run's loop reads before every turn and a command
//! in flight hears while it runs (§4, ADR-0006).
//!
//! The handler does one thing, because it may: it writes a byte. Anything that allocates,
//! takes a lock or reads a table is undefined behaviour inside a signal handler, so a
//! handler that turned a signal into a `Cancel` would be the bug this module exists not to
//! have. The byte is why the loop wakes at all: `poll` is never restarted by a signal, and
//! a pipe with a byte in it is a readiness the next `poll` returns.

use std::io;
use std::os::fd::AsRawFd;
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

use rustix::io::write as tell_fd;

/// The signal whose whole meaning is the user's word (§4).
const SIGINT: i32 = 2;

/// The byte a wake carries. Its value means nothing: a reader counts readiness, not letters.
const PING: &[u8] = b"!";

/// The write end the handler pings, kept as a number because a handler may not take a lock
/// to reach anything else. `-1` until a [`Wake`] has been installed, which is a handler
/// that runs before there is anything to wake.
static HANDLER: AtomicI32 = AtomicI32::new(-1);

/// Whether a `SIGINT` has arrived in this process, which is a fact about the launch rather
/// than about one wake: the launch ends when it is set, so nothing ever clears it.
static ARRIVED: AtomicBool = AtomicBool::new(false);

/// A pipe a writer wakes a reader through: one byte per event, and a reader that takes them
/// all at once. `poll` waits on the read end beside the terminal, so a wake is one of the
/// things that can end a launch's night — a signal, a run with something to say, and a key
/// all arrive the same way.
pub struct Wake {
    /// The end whoever waits reads: nothing is ever written here but pings.
    reader: OwnedFd,
    /// The end whoever raises writes to, shared because a run thread and a signal handler
    /// both raise one and neither owns the other.
    writer: Arc<OwnedFd>,
}

impl Wake {
    /// A fresh pipe, both ends non-blocking: a handler must never block on a full pipe, and
    /// a reader that has drained everything must not block waiting for a byte that is not
    /// coming.
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

    /// The end to wait on.
    pub fn reader(&self) -> BorrowedFd<'_> {
        self.reader.as_fd()
    }

    /// One ping: a wake for whoever is waiting, and nothing at all when the pipe is already
    /// full, which is a wake nobody has taken yet.
    pub fn raise(&self) {
        ping(self.writer.as_fd());
    }

    /// Takes every ping this pipe holds, so the next wait is a wait and not a spin.
    pub fn drain(&self) {
        let mut sink = [0u8; 64];
        while reads(self.reader(), &mut sink) > 0 {}
    }
}

/// The signal handling one launch owns: the wake a `SIGINT` pings, and the flag that says it
/// came.
pub struct Signals {
    /// The pipe the handler writes a byte to.
    wake: Arc<Wake>,
}

impl Signals {
    /// Installs the handler this launch hears `SIGINT` through, and hands back the wake it
    /// pings.
    ///
    /// The handler is this process's own from here on: a launch that wants a signal to end
    /// it has to be the one that says what ending means, and the default — the process dying
    /// with the terminal in raw mode and the alternate screen up — is the one outcome this
    /// module exists to prevent.
    pub fn install() -> io::Result<Signals> {
        let wake = Arc::new(Wake::open()?);
        HANDLER.store(wake.writer.as_fd().as_raw_fd(), Ordering::SeqCst);
        let handler: extern "C" fn(i32) = interrupted;
        unsafe { signal(SIGINT, handler as usize) };
        Ok(Signals { wake })
    }

    /// The wake this launch waits on: a `SIGINT` pings it, and so does a run with something
    /// to say.
    pub fn wake(&self) -> Arc<Wake> {
        Arc::clone(&self.wake)
    }

    /// Whether a `SIGINT` has arrived: the flag `Ctrl-C` sets is the same one, so a launch
    /// is cancelled by its own word however that word came (§4).
    pub fn arrived(&self) -> bool {
        ARRIVED.load(Ordering::SeqCst)
    }
}

/// The handler `SIGINT` runs: one `write` of one byte to the wake, and the flag that says
/// why. Anything else here would be undefined behaviour, and anything slower would be a
/// keystroke the terminal has to wait behind.
extern "C" fn interrupted(_signal: i32) {
    ARRIVED.store(true, Ordering::SeqCst);
    let fd = HANDLER.load(Ordering::SeqCst);
    if fd >= 0 {
        // `HANDLER` holds the number and is never cleared, so the fd dangles once the last
        // `Arc<Wake>` drops: this borrow is sound only while a wake is alive.
        ping(unsafe { BorrowedFd::borrow_raw(fd) });
    }
}

unsafe extern "C" {
    /// `signal(2)`, declared here rather than pulled in as a dependency: a cancellation is
    /// one flag and one byte, and the core names no crate to hear it either.
    fn signal(signum: i32, handler: usize) -> usize;
}

/// One byte, best effort: a pipe already full is a wake already waiting, and a handler has
/// nothing to do about it.
fn ping(fd: BorrowedFd<'_>) {
    let _ = tell_fd(fd, PING);
}

/// How many bytes one read took, and nothing at all when there is nothing there.
fn reads(fd: BorrowedFd<'_>, sink: &mut [u8]) -> usize {
    rustix::io::read(fd, sink).unwrap_or(0)
}

/// Makes one end of the pipe non-blocking: the flag `poll` needs so that a drain runs out
/// instead of waiting.
fn nonblocking(fd: &OwnedFd) -> io::Result<()> {
    rustix::fs::fcntl_setfl(fd, rustix::fs::OFlags::NONBLOCK)?;
    Ok(())
}
