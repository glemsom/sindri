//! The bytes a key arrives as, answered as the key or the paste they are: a decoder holds the
//! half of a sequence one read brought, and asks a terminal for nothing.

use crate::composer::Key;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Key(Key),
    Paste(String),
    /// Whether the terminal says this window is the one being read: a report and never a key.
    Focus(bool),
}

/// What one look at the front of the bytes came to: an event, bytes consumed that answer
/// nothing, or a wait for the rest of a sequence.
enum Step {
    Event(Event),
    Taken,
    Waiting,
}

const ESCAPE: u8 = 0x1b;

/// The bits a modifier mask holds, counted from one: shift, then alt, ctrl and super.
const SHIFT: u32 = 1;

const CTRL: u32 = 4;

/// Alt, super, hyper and meta: no act of this frame's is held with one of them.
const UNANSWERED: u32 = 2 | 8 | 16 | 32;

/// Caps lock and num lock, which a report carries beside the keys held and which name no key.
const LOCKS: u32 = 64 | 128;

/// `ESC [ 2 0 1 ~`: the sequence that ends a bracketed paste, so all before it is one paste.
const PASTE_END: &[u8] = b"\x1b[201~";

/// The bytes a key arrives as, kept until the sequence they start is complete: a terminal is
/// free to send one key in as many reads as it likes.
pub struct Decoder {
    pending: Vec<u8>,
    /// Whether the newest read was one byte, which is the only thing that tells the `Enter` key
    /// from a paste's line break, and a bare `ESC` from the first byte of a sequence: no keyboard
    /// puts two keys in one read.
    alone: bool,
    pasting: bool,
}

impl Decoder {
    pub fn new() -> Decoder {
        Decoder {
            pending: Vec::new(),
            alone: false,
            pasting: false,
        }
    }

    pub fn push(&mut self, bytes: &[u8]) {
        self.pending.extend_from_slice(bytes);
        self.alone = bytes.len() == 1;
    }

    /// The next answer the bytes hold, or nothing while the sequence they start is still
    /// arriving: `None` is a decoder waiting for more and never one that is finished.
    pub fn next_event(&mut self) -> Option<Event> {
        loop {
            let step = if self.pasting {
                self.pasted()
            } else {
                self.next_key()
            };
            match step {
                Step::Event(event) => return Some(event),
                Step::Taken => {}
                Step::Waiting => return None,
            }
        }
    }

    fn next_key(&mut self) -> Step {
        let Some(&byte) = self.pending.first() else {
            return Step::Waiting;
        };
        // An `ESC` that arrived alone is the Esc key: a sequence that opens with it brings the
        // bytes saying which, and waiting for one a keyboard may never send is a screen nobody can
        // close.
        if byte == ESCAPE && self.pending.len() == 1 && self.alone {
            self.take(1);
            return Step::Event(Event::Key(Key::Escape));
        }
        if byte == ESCAPE {
            return self.escape();
        }
        if byte >= 0x80 {
            if byte < 0xc0 {
                // A byte that starts no glyph: nothing types one, and a decoder that waited on it
                // would never answer the key behind it.
                self.take(1);
                return Step::Taken;
            }
            return match self.glyph() {
                Some(glyph) => Step::Event(Event::Key(Key::Char(glyph))),
                None => Step::Waiting,
            };
        }
        self.take(1);
        // A carriage return that came with more of its burst, before or after it, is a paste's line
        // break: a clipboard's newline reaches a terminal as the byte its Enter key would send, and
        // no line break in a paste may send.
        let key = match byte {
            b'\r' if self.alone => Key::Enter,
            b'\r' => Key::Newline,
            _ => match control(byte) {
                Some(key) => key,
                None => return Step::Taken,
            },
        };
        Step::Event(Event::Key(key))
    }

    fn escape(&mut self) -> Step {
        match self.pending.get(1) {
            None => Step::Waiting,
            Some(b'[') => self.control(),
            Some(b'O') => {
                // The older single-shift arrows: `ESC O A` is the up arrow under a terminal that
                // does not speak the protocol.
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
                // Taken rather than held: holding it would stall every key behind it.
                self.take(1);
                Step::Taken
            }
        }
    }

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
            // A modified arrow says so between the bracket and its final byte: the one spelling
            // terminals use for `Shift+↑` and `Shift+↓`.
            (b'A', _) if shifted(&params) => Step::Event(Event::Key(Key::ShiftUp)),
            (b'B', _) if shifted(&params) => Step::Event(Event::Key(Key::ShiftDown)),
            // `Shift+Tab` is `CSI Z` on every terminal, and `CSI 1;2Z` under one that says which
            // modifier it held.
            (b'Z', _) => Step::Event(Event::Key(Key::BackTab)),
            // `CSI I` and `CSI O`, the reports a terminal sends when its window becomes the one
            // being read and when it stops: neither names a glyph, so neither is a key.
            (b'I', _) => Step::Event(Event::Focus(true)),
            (b'O', _) => Step::Event(Event::Focus(false)),
            (b'H', _) | (b'~', "1") | (b'~', "7") => Step::Event(Event::Key(Key::Home)),
            (b'F', _) | (b'~', "4") | (b'~', "8") => Step::Event(Event::Key(Key::End)),
            (b'~', "200") => {
                self.pasting = true;
                Step::Taken
            }
            (b'u', _) => match reported(&params) {
                Some(key) => Step::Event(Event::Key(key)),
                None => Step::Taken,
            },
            _ => match arrow(last) {
                Some(key) => Step::Event(Event::Key(key)),
                None => Step::Taken,
            },
        }
    }

    /// The bracketed paste that is arriving: everything up to the sequence that ends it, as one
    /// edit. Until that sequence lands there is no event at all, which is what makes a line break
    /// inside a paste unable to send it.
    fn pasted(&mut self) -> Step {
        let Some(end) = self
            .pending
            .windows(PASTE_END.len())
            .position(|window| window == PASTE_END)
        else {
            return Step::Waiting;
        };
        let text = String::from_utf8_lossy(&self.pending[..end]).into_owned();
        self.take(end + PASTE_END.len());
        self.pasting = false;
        Step::Event(Event::Paste(text))
    }

    /// One glyph off the front of the bytes, or nothing while its last bytes are still on their
    /// way: a paste in a language this keyboard is not in arrives a read at a time, and half a
    /// glyph is not a glyph.
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

    fn take(&mut self, count: usize) {
        self.pending.drain(..count);
    }
}

impl Default for Decoder {
    fn default() -> Decoder {
        Decoder::new()
    }
}

/// The mask a modifier field holds, one more than shift, alt, ctrl and super: nothing when the
/// field is not a number, which is a terminal saying no modifier was held.
fn held(field: &str) -> Option<u32> {
    let field = field.split(':').next().unwrap_or_default();
    field.parse::<u32>().ok().map(|held| held.saturating_sub(1))
}

/// Whether a control sequence's parameters say the shift key was held: the modifiers are the
/// last field.
fn shifted(params: &str) -> bool {
    let modifiers = params.rsplit(';').next().unwrap_or_default();
    held(modifiers).is_some_and(|held| held & SHIFT != 0)
}

/// The key an arrow's final byte names, in either spelling: `ESC O A` and `CSI A` are the up
/// arrow.
fn arrow(last: u8) -> Option<Key> {
    match last {
        b'A' => Some(Key::Up),
        b'B' => Some(Key::Down),
        b'C' => Some(Key::Right),
        b'D' => Some(Key::Left),
        _ => None,
    }
}

/// The key one byte names, when it names one: what a control byte is read as, whether it reached
/// the loop as a byte or as a report that spells it out.
fn control(byte: u8) -> Option<Key> {
    match byte {
        0x03 => Some(Key::Cancel),
        0x15 => Some(Key::Clear),
        0x18 => Some(Key::Drop),
        b'\t' => Some(Key::Tab),
        // `Ctrl-Q`, which is why flow control has to be off: a terminal left with `IXON` eats this
        // byte instead of sending it.
        0x11 => Some(Key::Quit),
        0x19 => Some(Key::Copy),
        0x7f | 0x08 => Some(Key::Backspace),
        // `Ctrl-J` is the plain newline every terminal sends: it breaks a line rather than sending
        // one, and `Enter` is the carriage return `Ctrl-M` is.
        b'\n' => Some(Key::Newline),
        b'\r' => Some(Key::Enter),
        ESCAPE => Some(Key::Escape),
        0x20..=0x7e => Some(Key::Char(char::from(byte))),
        _ => None,
    }
}

/// The key a protocol report names: `CSI <code> ; <modifiers> u`. Only the reports this frame's
/// map has an answer for are answered, because a key nobody answers is not a key.
fn reported(params: &str) -> Option<Key> {
    // A report says which glyph a layout's shift makes of the key after a colon, and the key
    // before it is the one the frame answers.
    let (code, modifiers) = params.split_once(';').unwrap_or((params, "1"));
    let code: u32 = code.split(':').next()?.parse().ok()?;
    let modifiers = held(modifiers)? & !LOCKS;
    if modifiers & UNANSWERED != 0 {
        return None;
    }
    let shift = modifiers & SHIFT != 0;
    let ctrl = modifiers & CTRL != 0;
    match code {
        13 if modifiers == 0 => Some(Key::Enter),
        13 if shift || ctrl => Some(Key::Newline),
        9 if modifiers == 0 => Some(Key::Tab),
        9 if shift => Some(Key::BackTab),
        27 if modifiers == 0 => Some(Key::Escape),
        127 if modifiers == 0 || ctrl => Some(Key::Backspace),
        _ if ctrl => control(control_byte(code)?),
        _ => None,
    }
}

/// The byte a `Ctrl` and one key name, for the keys a control byte is the key's own code with its
/// three high bits taken off: the letters, and the six punctuation keys that share their range.
fn control_byte(code: u32) -> Option<u8> {
    let key = u8::try_from(code).ok()?;
    matches!(key, b'a'..=b'z' | b'@' | b'[' | b'\\' | b']' | b'^' | b'_').then(|| key & 0x1f)
}
