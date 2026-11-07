//! The mode stack and one key table per mode: a key resolves in exactly one mode, the
//! mode on top of the stack answers every key, and the row that answers it is one row
//! of one table — so a new mode is a new table and nothing else (§12, A7, A9).
//!
//! A table is data rather than a `match` for one reason: the `?` screen lists the keys
//! a reader can press right now, and a screen built from the same rows that answer the
//! keys cannot drift from them. `Mode::table` is therefore both the keymap and the
//! content of the screen that explains the keymap, and the appendix's "one screen
//! listing the keys of the current mode stack" is that list, mode by mode.
//!
//! The stack is explicit and shallow: `Composer` is the base every frame has, because a
//! prompt is what this front end is for, and a modal — today only `Keys` — sits over it.
//! `Stack::act` asks the mode on top and never the modes under it, which is what "a key
//! resolves in exactly one mode" means as code: a glyph pressed at a screen that binds
//! no glyph does nothing at all, rather than editing a draft nobody can see. The one
//! key that needs a rule instead of a row is `↑` (A9), and it is a row like any other
//! bound to `Act::Up`, so the rule that reads the queue lives in one arm of the frame
//! and not in a second keymap.

use crate::composer::Key;

/// One mode of the frame: a screen that answers keys, and the base a prompt is typed
/// into. A mode owns exactly one table, and a mode added later is a variant, a table
/// and a row in `Mode::ALL` — never a second `match` on keys somewhere else.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// The draft: the base of every stack, and the mode a launch opens in.
    Composer,
    /// The `?` screen: every key in force, the mode that answers first at the top.
    Keys,
}

impl Mode {
    /// Every mode there is, so a test can walk the tables rather than trust that the
    /// one it remembered is the one that landed.
    pub const ALL: [Mode; 2] = [Mode::Composer, Mode::Keys];

    /// The mode's name, as the `?` screen heads its rows with it.
    pub fn name(self) -> &'static str {
        match self {
            Mode::Composer => "composer",
            Mode::Keys => "keys",
        }
    }

    /// The keys this mode answers and what each of them does.
    pub fn table(self) -> &'static [Binding] {
        match self {
            Mode::Composer => &COMPOSER,
            Mode::Keys => &KEYS,
        }
    }

    /// What a key this mode's table binds nothing to comes to. The draft's table names
    /// the keys worth explaining and this is the rest of them: every other glyph, and a
    /// key no line of the frame has a use for, go to the composer's own map, which is
    /// where a text box's rules live. A screen has no such default — nothing is typed
    /// into a screen.
    fn fallback(self) -> Option<Act> {
        match self {
            Mode::Composer => Some(Act::Draft),
            Mode::Keys => None,
        }
    }
}

/// What pressing a key comes to, in the small vocabulary a table row spends: the frame
/// acts on it, and how it acts is one `match` in `View`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    /// The draft's own map answers it: a glyph typed, a line break, a backspace, a
    /// caret moved, `Ctrl-Q` quitting or `Ctrl-C` cancelling — the composer's rules,
    /// which this frame reads and never restates.
    Draft,
    /// `↑`, one key with two homes: the queue's newest prompt on the composer's first
    /// line, the row above the caret anywhere else (A9).
    Up,
    /// `Ctrl-Y`: the words the mark covers leave the frame, or the newest answer does
    /// when nothing is marked (§12).
    Copy,
    /// `Shift+↓`: the mark grows a whole row down the transcript (§12).
    ExtendDown,
    /// `Shift+↑`: the mark gives a whole row of itself back (§12).
    ExtendUp,
    /// `?`: the screen that lists the keys in force.
    Keys,
    /// `Esc`: the screen on top closes and the mode under it has the keys again.
    Close,
}

/// One row of a key table: the key, the way the `?` screen spells it, and what it does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Binding {
    /// The key this row answers, as the decoder hands it over.
    pub key: Key,
    /// How the key is spelled on the `?` screen — `Enter`, `Shift+Enter, Ctrl-J`, `↑`.
    pub name: &'static str,
    /// What pressing it does, in the words the `?` screen prints beside the name.
    pub does: &'static str,
    /// What the frame does about it.
    pub act: Act,
}

/// The draft's keys, in the order the `?` screen lists them: the ones a prompt is typed
/// and sent with first, then the caret's, then the two keys that end things.
///
/// Every row here but the ones carrying a rule of the frame's — `?`, `↑`, the two shift
/// arrows and `Ctrl-Y` — is `Act::Draft`: the composer's map is where a text box's rules
/// are, and this table is what that means as a list a reader can read.
const COMPOSER: [Binding; 15] = [
    Binding {
        key: Key::Char('?'),
        name: "?",
        does: "the keys, while the draft is empty",
        act: Act::Keys,
    },
    Binding {
        key: Key::Enter,
        name: "Enter",
        does: "send the prompt",
        act: Act::Draft,
    },
    Binding {
        key: Key::Newline,
        name: "Shift+Enter, Ctrl-J",
        does: "a line break in the draft",
        act: Act::Draft,
    },
    Binding {
        key: Key::Up,
        name: "↑",
        does: "the newest queued prompt, or the line above",
        act: Act::Up,
    },
    Binding {
        key: Key::Down,
        name: "↓",
        does: "the line below",
        act: Act::Draft,
    },
    Binding {
        key: Key::ShiftDown,
        name: "Shift+↓",
        does: "grow the mark a row down",
        act: Act::ExtendDown,
    },
    Binding {
        key: Key::ShiftUp,
        name: "Shift+↑",
        does: "grow the mark a row up",
        act: Act::ExtendUp,
    },
    Binding {
        key: Key::Left,
        name: "←",
        does: "a glyph left",
        act: Act::Draft,
    },
    Binding {
        key: Key::Right,
        name: "→",
        does: "a glyph right",
        act: Act::Draft,
    },
    Binding {
        key: Key::Home,
        name: "Home",
        does: "the start of the line",
        act: Act::Draft,
    },
    Binding {
        key: Key::End,
        name: "End",
        does: "the end of the line",
        act: Act::Draft,
    },
    Binding {
        key: Key::Copy,
        name: "Ctrl-Y",
        does: "copy the mark, or the last answer",
        act: Act::Copy,
    },
    Binding {
        key: Key::Backspace,
        name: "Backspace",
        does: "erase the glyph before the caret",
        act: Act::Draft,
    },
    Binding {
        key: Key::Cancel,
        name: "Ctrl-C",
        does: "cancel the run, or clear the draft",
        act: Act::Draft,
    },
    Binding {
        key: Key::Quit,
        name: "Ctrl-Q",
        does: "quit",
        act: Act::Draft,
    },
];

/// The `?` screen's own keys. Two of them close it and nothing else is bound: a screen
/// answers what it names and swallows what it does not, so the list a reader is looking
/// at is the whole of what the screen will do.
const KEYS: [Binding; 2] = [
    Binding {
        key: Key::Escape,
        name: "Esc",
        does: "close this screen",
        act: Act::Close,
    },
    Binding {
        key: Key::Char('?'),
        name: "?",
        does: "close this screen",
        act: Act::Close,
    },
];

/// The stack of modes: the base the frame is always in and the screens opened over it,
/// top last. A stack rather than a field, because a screen is entered from whatever was
/// live and gives it back when it closes, whichever that was.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stack {
    /// Base first — the composer, which never closes — and the screens above it.
    modes: Vec<Mode>,
}

impl Stack {
    /// A stack holding the composer and nothing over it: the stack a launch opens on.
    pub fn new() -> Stack {
        Stack {
            modes: vec![Mode::Composer],
        }
    }

    /// The mode that answers: the one on top.
    pub fn top(&self) -> Mode {
        self.modes.last().copied().unwrap_or(Mode::Composer)
    }

    /// Every mode of the stack, the one that answers first: the order the `?` screen
    /// lists them in, because the reader is in the top one and the rest are why they
    /// cannot use them.
    pub fn modes(&self) -> impl DoubleEndedIterator<Item = Mode> + '_ {
        self.modes.iter().rev().copied()
    }

    /// The screen on top, when there is one: what the frame paints instead of its bands
    /// and what has no caret in it.
    pub fn modal(&self) -> Option<Mode> {
        (self.top() != Mode::Composer).then(|| self.top())
    }

    /// Puts a screen on top. A screen already on top is not opened twice: `?` at the
    /// keys screen is one of the keys that closes it.
    pub fn open(&mut self, mode: Mode) {
        if self.top() != mode {
            self.modes.push(mode);
        }
    }

    /// Closes the screen on top. The composer is the base and stays: a frame with no
    /// mode would answer no key at all.
    pub fn close(&mut self) {
        if self.modes.len() > 1 {
            self.modes.pop();
        }
    }

    /// What `key` comes to here: one row of the top mode's table, or the top mode's
    /// fallback when its table names the key not at all. The modes under it are not
    /// asked, which is what keeps a key to one mode.
    pub fn act(&self, key: Key) -> Option<Act> {
        let mode = self.top();
        mode.table()
            .iter()
            .find(|binding| binding.key == key)
            .map(|binding| binding.act)
            .or_else(|| mode.fallback())
    }
}

impl Default for Stack {
    /// The stack a frame with no screens on it is: the composer alone.
    fn default() -> Stack {
        Stack::new()
    }
}
