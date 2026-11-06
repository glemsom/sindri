//! The terminal UI's contract with the terminal it draws on: the alternate screen it
//! takes and gives back, raw mode with flow control off, the keys it reads and the
//! sequences it writes back, and a refusal at exit 2 when there is no terminal to do
//! any of that on (§3).
//!
//! The terminal itself is the one thing this crate owns and the core may not name
//! (ADR-0005): the alternate screen, raw mode, the keys read off the tty and the
//! sequences written back to it all live in this module, and what a frame holds is
//! the core's business rather than the terminal's.
//!
//! The loop is a loop that waits. One frame is painted when the screen is taken — the
//! boot frame, empty transcript and all (§12) — and after it a key arrives as bytes
//! from `read`, so a launch with nobody at the keyboard writes nothing further: there
//! is no timer, no tick and no repaint, which is what §12's "an idle frame is never
//! drawn" means as code. A key that arrives is decoded here, because a key is more than
//! a byte — `Shift+Enter` is `CSI 13;2u` under the protocol §17 assumes — and a paste is
//! more than a key: it is a burst of bytes, or a bracketed paste (`ESC[200~ … ESC[201~`)
//! when the terminal offers one, and it reaches the composer as one edit with its line
//! breaks kept.

use std::fs::File;
use std::io::{self, IsTerminal, Read, Write};
use std::os::fd::{AsFd, BorrowedFd};
use std::panic::{self, PanicHookInfo};
use std::process::ExitCode;
use std::sync::{Mutex, MutexGuard, Once, OnceLock};

use ratatui::backend::CrosstermBackend;
use rustix::termios::{InputModes, OptionalActions, Termios, tcgetattr, tcsetattr};
use sindri::composer::{Key, Outcome};
use sindri::view::View;

/// Spec §15's usage error: a launch that cannot be a run, a screen a launch cannot
/// draw on.
const USAGE: u8 = 2;

/// Spec §15's setup failure: a terminal that will not take the screen or will not
/// give a key back.
const SETUP: u8 = 3;

/// Take the alternate screen, hide the cursor so the first frame is not drawn around a
/// caret sitting wherever the last shell left it, and ask for bracketed paste — so a
/// pasted prompt arrives as one sequence and not as a burst of keystrokes (§12).
const TAKE_SCREEN: &[u8] = b"\x1b[?1049h\x1b[?25l\x1b[?2004h";

/// Give the alternate screen back, stop asking for bracketed paste first, and put the
/// cursor back last, so a shell gets a terminal that looks the way it did.
const GIVE_BACK_SCREEN: &[u8] = b"\x1b[?2004l\x1b[?25h\x1b[?1049l";

/// The terminal UI: takes the screen, holds it until `Ctrl-Q`, gives it back.
pub fn run() -> ExitCode {
    if let Some(reason) = missing_terminal() {
        return crate::refuse(&reason, USAGE);
    }
    let terminal = match Terminal::stdio() {
        Ok(terminal) => terminal,
        Err(error) => {
            return crate::refuse(&format!("the terminal could not be taken: {error}"), SETUP);
        }
    };
    let screen = match Screen::take(&terminal) {
        Ok(screen) => screen,
        Err(error) => {
            return crate::refuse(&format!("the terminal would not go raw: {error}"), SETUP);
        }
    };
    let mut view = View::boot();
    let mut canvas = match canvas(&terminal) {
        Ok(canvas) => canvas,
        Err(error) => {
            return crate::refuse(&format!("the frame could not be painted: {error}"), SETUP);
        }
    };
    if let Err(error) = paint(&mut canvas, &view) {
        return crate::refuse(&format!("the frame could not be painted: {error}"), SETUP);
    }
    let keys = keys(&terminal, &mut canvas, &mut view);
    drop(screen);
    match keys {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => crate::refuse(&format!("the terminal could not be read: {error}"), SETUP),
    }
}

/// What a launch with no prompt is short of when it is short of a terminal: the
/// stream that is not one, and the thing a person can do about it.
fn missing_terminal() -> Option<String> {
    if !io::stdout().is_terminal() {
        return Some(String::from(
            "the terminal UI needs a terminal to draw on, and stdout is not one: \
             give a prompt with -b to run as a filter",
        ));
    }
    if !io::stdin().is_terminal() {
        return Some(String::from(
            "the terminal UI needs a terminal to read keys from, and stdin is not one: \
             give a prompt with -b to run as a filter",
        ));
    }
    None
}

/// The terminal this front end draws on, held as the two fds it really is — the one
/// keys arrive on and the one frames leave by — so a test can hand in a
/// pseudo-terminal and read what a person would have seen.
struct Terminal {
    /// Where keys come from: the terminal's input side, whose termios is raw mode.
    keys: File,
    /// Where frames go: the terminal's output side, the alternate screen.
    screen: File,
}

impl Terminal {
    /// This process's own standard input and output, which `run` has already
    /// refused to build a UI on unless both are terminals.
    fn stdio() -> io::Result<Terminal> {
        Terminal::pair(io::stdin().as_fd(), io::stdout().as_fd())
    }

    /// A terminal from the two ends a caller names, taken as copies so this value
    /// owns no descriptor the rest of the process still reads and writes.
    fn pair(keys: BorrowedFd<'_>, screen: BorrowedFd<'_>) -> io::Result<Terminal> {
        Ok(Terminal {
            keys: File::from(keys.try_clone_to_owned()?),
            screen: File::from(screen.try_clone_to_owned()?),
        })
    }
}

/// The screen a launch holds: taking one is what puts the terminal in raw mode, and
/// dropping it is what gives the terminal back. A guard rather than two calls, so
/// there is no path — a `?`, an early return, a panic — that can forget half.
struct Screen;

impl Screen {
    /// Takes the alternate screen and raw mode together, remembering the state that
    /// replacing them is a change to.
    fn take(terminal: &Terminal) -> io::Result<Screen> {
        guard_panics();
        let saved = tcgetattr(&terminal.keys)?;
        let mut raw = saved.clone();
        raw.make_raw();
        // `make_raw` clears flow control already, and this line stays because the
        // day raw mode is spelled differently is the day `Ctrl-Q` stops arriving.
        raw.input_modes.remove(InputModes::IXON);
        tcsetattr(&terminal.keys, OptionalActions::Now, &raw)?;

        // Both halves are handed to the one place a panic hook can reach, so the
        // screen is given back even when the unwinding that follows runs no guard.
        let restore = Restore {
            keys: terminal.keys.try_clone()?,
            screen: terminal.screen.try_clone()?,
            saved,
        };
        *lock(&GIVE_BACK) = Some(restore);
        if let Err(error) = tell(&terminal.screen, TAKE_SCREEN) {
            give_back();
            return Err(error);
        }
        Ok(Screen)
    }
}

impl Drop for Screen {
    fn drop(&mut self) {
        give_back();
    }
}

/// The screen frames are painted through: ratatui's canvas over this terminal's output
/// side. It is taken once, because a canvas that remembered nothing would rewrite every
/// cell of the frame on every key, and a frame is drawn once per change rather than
/// whole.
type Canvas<'a> = ratatui::Terminal<CrosstermBackend<&'a File>>;

/// A canvas over this terminal's output side, remembering what it last drew.
fn canvas(terminal: &Terminal) -> io::Result<Canvas<'_>> {
    ratatui::Terminal::new(CrosstermBackend::new(&terminal.screen))
}

/// Paints the frame the view holds now: at launch the boot frame, and afterwards one
/// frame per key that changed what it holds.
///
/// It goes through `View::paint` — the seam the fixture harness renders through
/// (ADR-0012) — onto this terminal's own output side, so what a reader sees and what a
/// golden frame holds are one picture and not two. The caret is the terminal's own
/// cursor and never a cell of the frame: an ink names a foreground and nothing else, so
/// a caret drawn as a cell would be a tinted letter and nothing at all on a space.
fn paint(canvas: &mut Canvas<'_>, view: &View) -> io::Result<()> {
    canvas.draw(|frame| {
        view.paint(frame.area(), frame.buffer_mut());
        if let Some(at) = view.cursor(frame.area()) {
            frame.set_cursor_position(at);
        }
    })?;
    Ok(())
}

/// Reads the terminal until `Ctrl-Q`, one event at a time, and repaints when an event
/// changed what the frame holds.
///
/// The loop blocks in `read` and paints nothing while it waits: a launch with nobody at
/// the keyboard asks the kernel for bytes and sleeps, so an idle frame is not skipped,
/// it is never drawn (§12).
fn keys(terminal: &Terminal, canvas: &mut Canvas<'_>, view: &mut View) -> io::Result<()> {
    let mut input = &terminal.keys;
    let mut decoder = Decoder::new();
    let mut read = [0u8; 4096];
    loop {
        let count = input.read(&mut read)?;
        // The terminal went away: there is nothing left to quit from, and the screen is
        // still given back by the guard.
        if count == 0 {
            return Ok(());
        }
        decoder.push(&read[..count]);
        while let Some(event) = decoder.next() {
            if answer(canvas, view, event)? {
                return Ok(());
            }
        }
    }
}

/// Answers one event, and says whether the launch ends: the one place a key becomes a
/// repaint or an exit, so a key the composer learns to answer is a new arm here and
/// nothing else.
fn answer(canvas: &mut Canvas<'_>, view: &mut View, event: Event) -> io::Result<bool> {
    let key = match event {
        // A paste is one edit and never a key, so nothing inside one can send: the
        // composer is handed the whole text with its line breaks in it.
        Event::Paste(text) => {
            view.paste(&text);
            paint(canvas, view)?;
            return Ok(false);
        }
        Event::Key(key) => key,
    };
    match view.key(key) {
        Outcome::Unchanged => Ok(false),
        Outcome::Edited | Outcome::Cancel => {
            paint(canvas, view)?;
            Ok(false)
        }
        Outcome::Send(prompt) => {
            // The draft left the composer as a prompt and nothing is listening: the run
            // loop is #43's, so this value is the seam it takes and this arm is where it
            // will be handed over. A prompt nobody took is not an error — it is the
            // frame before the run that answers it.
            drop(prompt);
            paint(canvas, view)?;
            Ok(false)
        }
        // `Ctrl-Q` is the one way out.
        Outcome::Quit => Ok(true),
    }
}

/// What the terminal sent, once the bytes that carry it are whole.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Event {
    /// One key, however many bytes it took to say.
    Key(Key),
    /// A pasted prompt: one edit, its line breaks kept, and no way to send it.
    Paste(String),
}

/// What reading one sequence out of the bytes came to: a whole event, a sequence that
/// only changed what the next one means, or a sequence whose last bytes are still on
/// their way.
enum Step {
    Event(Event),
    Taken,
    Waiting,
}

/// `ESC`, the byte every sequence but a control one starts with.
const ESCAPE: u8 = 0x1b;

/// `ESC [ 2 0 1 ~`: the sequence that ends a bracketed paste, so everything before it is
/// one paste.
const PASTE_END: &[u8] = b"\x1b[201~";

/// The bytes a key arrives as, kept until the sequence they start is complete: a key is
/// more than a byte (§17), and a terminal is free to send one in as many reads as it
/// likes.
struct Decoder {
    /// Bytes read but not yet answered by an event.
    pending: Vec<u8>,
    /// Whether the newest read was one byte, which is the only thing that tells the
    /// `Enter` key from a paste's line break: a burst of bytes is a paste, and no
    /// keyboard puts two keys in one read.
    alone: bool,
    /// Whether the bytes arriving now are a bracketed paste, up to `PASTE_END`.
    pasting: bool,
}

impl Decoder {
    /// A decoder holding nothing: the state a launch's first read starts from.
    fn new() -> Decoder {
        Decoder {
            pending: Vec::new(),
            alone: false,
            pasting: false,
        }
    }

    /// Hands over the bytes of one read, as `read` gave them up.
    fn push(&mut self, bytes: &[u8]) {
        self.pending.extend_from_slice(bytes);
        self.alone = bytes.len() == 1;
    }

    /// The next event the bytes hold, or nothing when the sequence at their front is
    /// still incomplete — whatever it holds stays pending for the next read.
    fn next(&mut self) -> Option<Event> {
        loop {
            let step = if self.pasting {
                self.pasted()
            } else {
                self.one()
            };
            match step {
                Step::Event(event) => return Some(event),
                Step::Taken => {}
                Step::Waiting => return None,
            }
        }
    }

    /// The next key the bytes hold, one byte or one sequence at a time.
    fn one(&mut self) -> Step {
        let Some(&byte) = self.pending.first() else {
            return Step::Waiting;
        };
        if byte == ESCAPE {
            return self.escape();
        }
        if byte >= 0x80 {
            if byte < 0xc0 {
                // A byte that starts no glyph: nothing types one, and a decoder that
                // waited on it would never answer the key behind it.
                self.take(1);
                return Step::Taken;
            }
            return match self.glyph() {
                Some(glyph) => Step::Event(Event::Key(Key::Char(glyph))),
                None => Step::Waiting,
            };
        }
        self.take(1);
        let key = match byte {
            0x03 => Key::Cancel,
            // `Ctrl-Q`, which is why flow control has to be off: a terminal left with
            // `IXON` eats this byte instead of sending it.
            0x11 => Key::Quit,
            0x7f | 0x08 => Key::Backspace,
            // `Ctrl-J` is the plain newline every terminal sends, and it breaks a line
            // rather than sending one: `Enter` is the carriage return below.
            b'\n' => Key::Newline,
            b'\r' if self.alone => Key::Enter,
            // A carriage return that came with more of its burst, before or after it, is
            // a paste's line break: a clipboard's newline reaches a terminal as the byte
            // its Enter key would send, and no line break in a paste may send.
            b'\r' => Key::Newline,
            0x20..=0x7e => Key::Char(char::from(byte)),
            // A control byte no key of this frame's is: not ours to answer.
            _ => return Step::Taken,
        };
        Step::Event(Event::Key(key))
    }

    /// One escape sequence off the front of the bytes: the keys §17's protocol and the
    /// arrows spell, and the sequence that opens a bracketed paste.
    fn escape(&mut self) -> Step {
        match self.pending.get(1) {
            None => Step::Waiting,
            Some(b'[') => self.control(),
            Some(b'O') => {
                // The older single-shift arrows: `ESC O A` is the up arrow under a
                // terminal that is not speaking the protocol §17 assumes.
                let Some(&last) = self.pending.get(2) else {
                    return Step::Waiting;
                };
                self.take(3);
                match arrow(last) {
                    Some(key) => Step::Event(Event::Key(key)),
                    None => Step::Taken,
                }
            }
            Some(_) => {
                // A lone `ESC` is a key this frame has no use for, and holding it would
                // stall every key behind it.
                self.take(1);
                Step::Taken
            }
        }
    }

    /// A control sequence, `ESC [ …`, answered at its final byte — which is how a key of
    /// more than one byte is read whole whatever the terminal split it into.
    fn control(&mut self) -> Step {
        let Some(last_at) = self.pending[2..]
            .iter()
            .position(|byte| (0x40..=0x7e).contains(byte))
        else {
            return Step::Waiting;
        };
        let last_at = last_at + 2;
        let last = self.pending[last_at];
        let params = String::from_utf8_lossy(&self.pending[2..last_at]).into_owned();
        self.take(last_at + 1);
        match (last, params.as_str()) {
            (b'A', _) => Step::Event(Event::Key(Key::Up)),
            (b'B', _) => Step::Event(Event::Key(Key::Down)),
            (b'C', _) => Step::Event(Event::Key(Key::Right)),
            (b'D', _) => Step::Event(Event::Key(Key::Left)),
            (b'H', _) | (b'~', "1") | (b'~', "7") => Step::Event(Event::Key(Key::Home)),
            (b'F', _) | (b'~', "4") | (b'~', "8") => Step::Event(Event::Key(Key::End)),
            // The paste that starts here is answered by `pasted`: one event for
            // everything up to the sequence that ends it.
            (b'~', "200") => {
                self.pasting = true;
                Step::Taken
            }
            (b'u', _) => match reported(&params) {
                Some(key) => Step::Event(Event::Key(key)),
                None => Step::Taken,
            },
            _ => Step::Taken,
        }
    }

    /// The bracketed paste that is arriving: everything up to the sequence that ends it,
    /// as one edit. Until that sequence lands there is no event at all, which is what
    /// makes a line break inside a paste unable to send it.
    fn pasted(&mut self) -> Step {
        let Some(end) = find(&self.pending, PASTE_END) else {
            return Step::Waiting;
        };
        let text = String::from_utf8_lossy(&self.pending[..end]).into_owned();
        self.take(end + PASTE_END.len());
        self.pasting = false;
        Step::Event(Event::Paste(text))
    }

    /// One glyph off the front of the bytes, or nothing while its last bytes are still
    /// on their way: a paste in a language this keyboard is not in arrives a read at a
    /// time, and half a glyph is not a glyph.
    fn glyph(&mut self) -> Option<char> {
        let width = match self.pending.first() {
            Some(0xc0..=0xdf) => 2,
            Some(0xe0..=0xef) => 3,
            Some(0xf0..=0xf7) => 4,
            _ => return None,
        };
        if self.pending.len() < width {
            return None;
        }
        let glyph = std::str::from_utf8(&self.pending[..width])
            .ok()
            .and_then(|text| text.chars().next());
        self.take(width);
        glyph
    }

    /// Drops `count` bytes off the front of what is pending.
    fn take(&mut self, count: usize) {
        self.pending.drain(..count);
    }
}

/// The key an arrow's final byte names, from either spelling of the arrows.
fn arrow(last: u8) -> Option<Key> {
    match last {
        b'A' => Some(Key::Up),
        b'B' => Some(Key::Down),
        b'C' => Some(Key::Right),
        b'D' => Some(Key::Left),
        _ => None,
    }
}

/// The key a kitty-protocol report names: `CSI <code> ; <modifiers> u`, where the
/// modifiers are one more than a mask of shift, alt, ctrl and super. Only the reports
/// this frame's map needs are answered — the `Enter` that sends and the two that break a
/// line — because a key nobody answers is not a key.
fn reported(params: &str) -> Option<Key> {
    /// The bit that says shift was held, in the protocol's modifier mask.
    const SHIFT: u32 = 1;
    /// The bit that says ctrl was held.
    const CTRL: u32 = 4;

    let (code, modifiers) = params.split_once(';').unwrap_or((params, "1"));
    let code: u32 = code.parse().ok()?;
    let modifiers: u32 = modifiers.split(':').next()?.parse().ok()?;
    let modifiers = modifiers.saturating_sub(1);
    match (code, modifiers) {
        (13, 0) => Some(Key::Enter),
        (13, held) if held & SHIFT != 0 => Some(Key::Newline),
        (106, held) if held & CTRL != 0 => Some(Key::Newline),
        _ => None,
    }
}

/// Where `needle` first appears in `bytes`, which is how a bracketed paste finds the
/// sequence that ends it.
fn find(bytes: &[u8], needle: &[u8]) -> Option<usize> {
    bytes
        .windows(needle.len())
        .position(|window| window == needle)
}

/// What the terminal is to be given back as: the state raw mode replaced and the two
/// fds it is put back through, held where a panic hook can reach them, because an
/// unwinding path runs no guard this module owns.
static GIVE_BACK: Mutex<Option<Restore>> = Mutex::new(None);

/// The state to put back, and the terminal it goes to.
struct Restore {
    /// The terminal's input side, whose termios raw mode changed.
    keys: File,
    /// The terminal's output side, where the alternate screen was taken.
    screen: File,
    /// The terminal's state before raw mode.
    saved: Termios,
}

impl Restore {
    /// Puts the terminal back: the screen is left first, so nothing that follows —
    /// a panic message included — is drawn on a screen that is about to be somebody
    /// else's, and then the state raw mode replaced is restored.
    fn put_back(self) {
        let _ = tell(&self.screen, GIVE_BACK_SCREEN);
        let _ = tcsetattr(&self.keys, OptionalActions::Now, &self.saved);
    }
}

/// Gives the terminal back, exactly once, whichever of the guard and the panic hook
/// asks first: the state is taken out of the static, so the second ask finds nothing
/// and a shell is never handed a half-restored terminal twice.
fn give_back() {
    let taken = lock(&GIVE_BACK).take();
    if let Some(restore) = taken {
        restore.put_back();
    }
}

/// A panic hook, in the shape `std::panic` hands out and takes back: what the hook
/// this module installs keeps of the one it replaced, so a panic still gets reported.
type Hook = Box<dyn Fn(&PanicHookInfo<'_>) + Send + Sync>;

/// Installs, once, the hook that gives the terminal back when a panic unwinds past
/// the guard: a terminal left raw and blank is a shell nobody can type in, and an
/// unwinding path runs no guard at all.
fn guard_panics() {
    /// Whether this process has a hook of this module's yet.
    static INSTALLED: Once = Once::new();
    /// The hook this one replaced, which is still what reports a panic.
    static REPLACED: OnceLock<Hook> = OnceLock::new();

    INSTALLED.call_once(|| {
        // The replaced hook is called rather than dropped: a terminal given back is
        // no reason for a panic to go unreported, and a panic nobody can read is the
        // bug this hook is least able to catch.
        let _ = REPLACED.set(panic::take_hook());
        panic::set_hook(Box::new(|raised| {
            give_back();
            if let Some(replaced) = REPLACED.get() {
                replaced(raised);
            }
        }));
    });
}

/// Writes a sequence out and flushes it: a frame that sat in a buffer is a frame
/// nobody saw, and a screen given back only at exit is a terminal left blank when
/// the process dies.
fn tell(out: &File, sequence: &[u8]) -> io::Result<()> {
    let mut out = out;
    out.write_all(sequence)?;
    out.flush()
}

/// A lock held for the length of one statement: a panic hook that blocked on a
/// poisoned lock would nest the panic it was called to clean up after.
fn lock<T>(held: &Mutex<T>) -> MutexGuard<'_, T> {
    held.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    //! The one contract an integration test cannot reach from outside the process: a
    //! panic unwinds past no guard this module owns, so the hook is proved here, on a
    //! pseudo-terminal of this process's own, where a panic can be raised and caught
    //! on purpose and the terminal's state read back afterwards.

    use std::panic;
    use std::path::PathBuf;

    use rustix::fs::{Mode, OFlags, open};
    use rustix::pty::{OpenptFlags, grantpt, openpt, ptsname, unlockpt};
    use rustix::termios::LocalModes;

    use super::*;

    /// A pseudo-terminal the test holds both ends of: the slave stands in for both
    /// standard streams a launch would be handed, and the master is where the
    /// sequences a launch writes come back.
    struct Pty {
        /// The end nobody in the program reads, where what it drew lands.
        master: File,
        /// The end the program takes as its terminal.
        slave: File,
    }

    impl Pty {
        /// A fresh pseudo-terminal, nobody's controlling terminal.
        fn open() -> Pty {
            let master = openpt(OpenptFlags::RDWR | OpenptFlags::NOCTTY).expect("a pseudoterminal");
            grantpt(&master).expect("the slave is granted");
            unlockpt(&master).expect("the slave is unlocked");
            let name = ptsname(&master, Vec::new()).expect("the slave has a name");
            let path = PathBuf::from(name.to_str().expect("a printable name"));
            let slave = open(path.as_path(), OFlags::RDWR | OFlags::NOCTTY, Mode::empty())
                .expect("the slave opens");
            Pty {
                master: File::from(master),
                slave: File::from(slave),
            }
        }

        /// Everything written to the terminal since the last read of it.
        fn drawn(&mut self) -> String {
            let mut drawn = [0u8; 4096];
            let mut master = &self.master;
            let count = master.read(&mut drawn).expect("the terminal's screen");
            String::from_utf8_lossy(&drawn[..count]).into_owned()
        }

        /// The terminal's state, as the program left it.
        fn attrs(&self) -> Termios {
            tcgetattr(&self.slave).expect("the terminal's state")
        }
    }

    #[test]
    fn a_panic_gives_the_terminal_back_before_the_guard_can() {
        let mut pty = Pty::open();
        let before = pty.attrs();
        let terminal =
            Terminal::pair(pty.slave.as_fd(), pty.slave.as_fd()).expect("a terminal to draw on");
        let screen = Screen::take(&terminal).expect("the screen is taken");

        assert!(
            !pty.attrs().local_modes.contains(LocalModes::ICANON),
            "raw mode is in force, so there is something for a panic to undo"
        );

        let caught = panic::catch_unwind(|| panic!("a panic the hook has to clean up after"));

        assert!(caught.is_err(), "the panic really was raised");
        let after = pty.attrs();
        assert_eq!(
            after.local_modes.bits(),
            before.local_modes.bits(),
            "the terminal's local modes are the ones raw mode replaced"
        );
        assert_eq!(
            after.input_modes.bits(),
            before.input_modes.bits(),
            "and its input modes too, flow control included"
        );
        let drawn = pty.drawn();
        assert!(
            drawn.contains(std::str::from_utf8(GIVE_BACK_SCREEN).expect("a printable sequence")),
            "the alternate screen was given back while the guard was still alive: {drawn:?}"
        );

        // The guard is dropped after the panic has already been cleaned up after, and
        // a terminal handed back twice is a shell with its state overwritten.
        drop(screen);
        assert_eq!(
            pty.attrs().input_modes.bits(),
            before.input_modes.bits(),
            "the second give-back changes nothing"
        );
    }

    mod decoding {
        //! The bytes a terminal really sends, read as the keys the composer's map
        //! answers: a sequence that arrives in pieces, a paste that arrives as a burst,
        //! and the bracketed paste §17's protocol wraps one in — none of them guessed at.

        use super::super::*;

        /// Every event the decoder answers for the bytes, pushed the way `read` hands
        /// them over: one chunk at a time.
        fn events(chunks: &[&[u8]]) -> Vec<Event> {
            let mut decoder = Decoder::new();
            let mut events = Vec::new();
            for chunk in chunks {
                decoder.push(chunk);
                while let Some(event) = decoder.next() {
                    events.push(event);
                }
            }
            events
        }

        /// The one event the bytes come to, or a failure naming what came instead.
        fn event(bytes: &[u8]) -> Event {
            let mut all = events(&[bytes]);
            assert_eq!(
                all.len(),
                1,
                "one key is one event, and these bytes made {all:?}"
            );
            all.pop().expect("the event")
        }

        /// A key, as the event that carries it.
        fn key(key: Key) -> Event {
            Event::Key(key)
        }

        #[test]
        fn a_glyph_typed_is_one_key() {
            assert_eq!(event(b"a"), key(Key::Char('a')));
        }

        #[test]
        fn a_glyph_split_across_two_reads_arrives_whole() {
            // The tick's three bytes, as a terminal may hand them over across two reads.
            assert_eq!(events(&[&[0xe2, 0x9c], &[0x93]]), vec![key(Key::Char('✓'))]);
        }

        #[test]
        fn enter_is_the_carriage_return_a_keyboard_sends() {
            assert_eq!(event(b"\r"), key(Key::Enter));
        }

        #[test]
        fn ctrl_j_is_the_newline_every_terminal_sends() {
            assert_eq!(event(b"\n"), key(Key::Newline));
        }

        #[test]
        fn shift_enter_under_the_kitty_protocol_breaks_the_line() {
            assert_eq!(event(b"\x1b[13;2u"), key(Key::Newline));
        }

        #[test]
        fn a_kitty_enter_report_without_a_modifier_still_sends() {
            assert_eq!(event(b"\x1b[13u"), key(Key::Enter));
        }

        #[test]
        fn ctrl_q_and_ctrl_c_are_whole_keys() {
            assert_eq!(event(b"\x11"), key(Key::Quit));
            assert_eq!(event(b"\x03"), key(Key::Cancel));
        }

        #[test]
        fn backspace_arrives_as_either_byte_raw_mode_leaves_it() {
            assert_eq!(event(b"\x7f"), key(Key::Backspace));
            assert_eq!(event(b"\x08"), key(Key::Backspace));
        }

        #[test]
        fn the_arrows_and_the_end_keys_arrive_in_either_spelling() {
            assert_eq!(event(b"\x1b[A"), key(Key::Up));
            assert_eq!(event(b"\x1bOB"), key(Key::Down), "the older spelling");
            assert_eq!(event(b"\x1b[C"), key(Key::Right));
            assert_eq!(event(b"\x1b[D"), key(Key::Left));
            assert_eq!(event(b"\x1b[H"), key(Key::Home));
            assert_eq!(event(b"\x1b[1~"), key(Key::Home));
            assert_eq!(event(b"\x1b[F"), key(Key::End));
            assert_eq!(event(b"\x1b[4~"), key(Key::End));
        }

        #[test]
        fn a_sequence_read_in_pieces_is_still_one_key() {
            assert_eq!(
                events(&[b"\x1b[", b"13;", b"2u"]),
                vec![key(Key::Newline)],
                "a key split across three reads is one key, not four"
            );
        }

        #[test]
        fn a_bracketed_paste_is_one_event_however_many_lines_it_holds() {
            assert_eq!(
                event(b"\x1b[200~one\r\ntwo\rthree\x1b[201~"),
                Event::Paste(String::from("one\r\ntwo\rthree")),
                "the clipboard's text, whole, and not one event per line"
            );
        }

        #[test]
        fn a_paste_that_has_not_ended_yet_is_not_a_key() {
            assert!(
                events(&[b"\x1b[200~one\r"]).is_empty(),
                "no line break inside a paste can escape as the key that sends it"
            );
        }

        #[test]
        fn a_paste_that_arrives_as_a_burst_sends_nothing() {
            let events = events(&[b"one\rtwo\r"]);

            assert!(
                !events.contains(&key(Key::Enter)),
                "a terminal without bracketed paste sends a paste as pressed keys, and \
                 no line break in one may send: {events:?}"
            );
            assert_eq!(events.len(), 8, "one event per glyph and per line break");
        }

        #[test]
        fn a_key_after_a_paste_is_read_again() {
            assert_eq!(
                events(&[b"\x1b[200~a\x1b[201~", b"\r"]),
                vec![Event::Paste(String::from("a")), key(Key::Enter)],
                "the paste ends where its sequence ends, and the next key is a key"
            );
        }
    }
}
