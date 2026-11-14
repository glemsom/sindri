use crate::composer::Key;
use crate::ink::{Faces, Ink, Look};
use crate::list::{self, Column, ScreenRow};

/// The look a key is drawn in wherever a key is named.
pub const KEY: Look = Look::plain(Ink::Plain).wearing(Faces::BOLD);

/// The look the words that say what a key does hang in, quieter than the key they explain.
pub const MEANING: Look = Look::plain(Ink::Muted);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Composer,
    Keys,
    Files,
    Models,
    Settings,
    Stats,
}

impl Mode {
    pub const ALL: [Mode; 6] = [
        Mode::Composer,
        Mode::Keys,
        Mode::Files,
        Mode::Models,
        Mode::Settings,
        Mode::Stats,
    ];

    pub fn description(self) -> &'static Description {
        DESCRIPTIONS
            .iter()
            .find(|described| described.mode == self)
            .expect("a mode with no description has no name, no table and no card to wear")
    }

    pub fn name(self) -> &'static str {
        self.description().name
    }

    /// The two labels a screen wears on its card: its name, and the key its own table binds to
    /// leaving it. A draft has neither, because it is no screen.
    pub fn labels(self) -> Option<(&'static str, &'static str)> {
        Some((self.title()?, self.leaves()?))
    }

    /// The key this mode's table binds to leaving it, and `None` when it binds none.
    pub fn leaves(self) -> Option<&'static str> {
        Some(
            self.description()
                .table
                .iter()
                .find(|binding| binding.act == Act::Close)?
                .name,
        )
    }

    /// The name a screen wears in its card's top border, and `None` for the draft, which is no
    /// screen and takes no card.
    pub fn title(self) -> Option<&'static str> {
        self.description().title
    }

    pub fn table(self) -> &'static [Binding] {
        self.description().table
    }

    /// The body this screen's description names, and `None` for the draft, which is no screen.
    pub fn body(self) -> Option<Body> {
        self.description().body
    }

    /// A screen has none — nothing is typed into a screen — where a draft, a query and a
    /// settings value fall to that text box's own map.
    fn fallback(self) -> Option<Act> {
        self.description().fallback
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    Draft,
    Up,
    Copy,
    Fold,
    ExtendDown,
    ExtendUp,
    Drop,
    Keys,
    Files,
    Close,
    Query,
    Previous,
    Next,
    Pick,
    Choose,
    Value,
    Flip,
    Clear,
    Write,
    Back,
    Forward,
    Tail,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Binding {
    pub key: Key,
    pub name: &'static str,
    pub does: &'static str,
    pub act: Act,
}

const COMPOSER: [Binding; 23] = [
    Binding {
        key: Key::Char('?'),
        name: "?",
        does: "the keys, while the draft is empty",
        act: Act::Keys,
    },
    Binding {
        key: Key::Char('@'),
        name: "@",
        does: "the files of the workspace, while it starts a word",
        act: Act::Files,
    },
    Binding {
        key: Key::Enter,
        name: "Enter",
        does: "send the prompt",
        act: Act::Draft,
    },
    Binding {
        key: Key::Escape,
        name: "Esc",
        does: "close the commands card, and the draft as it was",
        act: Act::Close,
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
        key: Key::Drop,
        name: "Ctrl-X",
        does: "drop the newest queued prompt",
        act: Act::Drop,
    },
    Binding {
        key: Key::Down,
        name: "↓",
        does: "the line below",
        act: Act::Draft,
    },
    Binding {
        key: Key::PageUp,
        name: "PgUp",
        does: "the transcript a band back",
        act: Act::Back,
    },
    Binding {
        key: Key::Clear,
        name: "Ctrl-U",
        does: "the transcript a band back",
        act: Act::Back,
    },
    Binding {
        key: Key::PageDown,
        name: "PgDn",
        does: "the transcript a band on",
        act: Act::Forward,
    },
    Binding {
        key: Key::Forward,
        name: "Ctrl-D",
        does: "the transcript a band on",
        act: Act::Forward,
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
        does: "the tail of the run, or the end of the line",
        act: Act::Tail,
    },
    Binding {
        key: Key::Copy,
        name: "Ctrl-Y",
        does: "copy the mark, or the last answer",
        act: Act::Copy,
    },
    Binding {
        key: Key::Fold,
        name: "Ctrl-O",
        does: "open or fold the newest result",
        act: Act::Fold,
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

const FILES: [Binding; 5] = [
    Binding {
        key: Key::Escape,
        name: "Esc",
        does: "close this screen, and the draft as it was",
        act: Act::Close,
    },
    Binding {
        key: Key::Up,
        name: "↑",
        does: "the file above",
        act: Act::Previous,
    },
    Binding {
        key: Key::Down,
        name: "↓",
        does: "the file below",
        act: Act::Next,
    },
    Binding {
        key: Key::Enter,
        name: "Enter",
        does: "put the chosen path in the draft",
        act: Act::Pick,
    },
    Binding {
        key: Key::Backspace,
        name: "Backspace",
        does: "erase the last glyph of the query",
        act: Act::Query,
    },
];

const MODELS: [Binding; 5] = [
    Binding {
        key: Key::Escape,
        name: "Esc",
        does: "close this screen, and name no model",
        act: Act::Close,
    },
    Binding {
        key: Key::Up,
        name: "↑",
        does: "the served name above",
        act: Act::Previous,
    },
    Binding {
        key: Key::Down,
        name: "↓",
        does: "the served name below",
        act: Act::Next,
    },
    Binding {
        key: Key::Enter,
        name: "Enter",
        does: "write the chosen name as the model",
        act: Act::Choose,
    },
    Binding {
        key: Key::Backspace,
        name: "Backspace",
        does: "erase the last glyph of the query",
        act: Act::Query,
    },
];

const SETTINGS: [Binding; 9] = [
    Binding {
        key: Key::Escape,
        name: "Esc",
        does: "close this screen and write nothing",
        act: Act::Close,
    },
    Binding {
        key: Key::Enter,
        name: "Enter",
        does: "write the keys you touched, and close",
        act: Act::Write,
    },
    Binding {
        key: Key::Down,
        name: "↓",
        does: "the key below",
        act: Act::Next,
    },
    Binding {
        key: Key::Up,
        name: "↑",
        does: "the key above",
        act: Act::Previous,
    },
    Binding {
        key: Key::Tab,
        name: "Tab",
        does: "the key below",
        act: Act::Next,
    },
    Binding {
        key: Key::BackTab,
        name: "Shift+Tab",
        does: "the key above",
        act: Act::Previous,
    },
    Binding {
        key: Key::Clear,
        name: "Ctrl-U",
        does: "empty the value, which unsets the key",
        act: Act::Clear,
    },
    Binding {
        key: Key::Char(' '),
        name: "Space",
        does: "flip a toggle, or type a space",
        act: Act::Flip,
    },
    Binding {
        key: Key::Backspace,
        name: "Backspace",
        does: "erase the last glyph of the value",
        act: Act::Value,
    },
];

const STATS: [Binding; 1] = [Binding {
    key: Key::Escape,
    name: "Esc",
    does: "close this screen, and the draft as it was",
    act: Act::Close,
}];

/// What a screen's body holds: the rows the screen keeping them builds, and — the one body that is
/// not rows — the facts a ledger lays out whole.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Body {
    Keys,
    Files,
    Models,
    Settings,
    /// The facts a run holds, laid out whole as the record wrote them rather than set in rows.
    Ledger,
}

/// What a mode is called, what its card wears, the keys it answers, what a printable key falls back
/// to and the body its screen builds. The draft states no title and no body, which is how a mode is
/// handed no card.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Description {
    pub mode: Mode,
    pub name: &'static str,
    pub title: Option<&'static str>,
    pub table: &'static [Binding],
    pub fallback: Option<Act>,
    pub body: Option<Body>,
}

pub const DESCRIPTIONS: [Description; 6] = [
    Description {
        mode: Mode::Composer,
        name: "composer",
        title: None,
        table: &COMPOSER,
        fallback: Some(Act::Draft),
        body: None,
    },
    Description {
        mode: Mode::Keys,
        name: "keys",
        title: Some("? keys"),
        table: &KEYS,
        fallback: None,
        body: Some(Body::Keys),
    },
    Description {
        mode: Mode::Files,
        name: "files",
        title: Some("@ files"),
        table: &FILES,
        fallback: Some(Act::Query),
        body: Some(Body::Files),
    },
    Description {
        mode: Mode::Models,
        name: "model",
        title: Some("/model"),
        table: &MODELS,
        fallback: Some(Act::Query),
        body: Some(Body::Models),
    },
    Description {
        mode: Mode::Settings,
        name: "settings",
        title: Some("/settings"),
        table: &SETTINGS,
        fallback: Some(Act::Value),
        body: Some(Body::Settings),
    },
    Description {
        mode: Mode::Stats,
        name: "stats",
        title: Some("/stats"),
        table: &STATS,
        fallback: None,
        body: Some(Body::Ledger),
    },
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stack {
    modes: Vec<Mode>,
}

impl Stack {
    pub fn new() -> Stack {
        Stack {
            modes: vec![Mode::Composer],
        }
    }

    pub fn top(&self) -> Mode {
        self.modes.last().copied().unwrap_or(Mode::Composer)
    }

    pub fn modes(&self) -> impl DoubleEndedIterator<Item = Mode> + '_ {
        self.modes.iter().rev().copied()
    }

    pub fn modal(&self) -> Option<Mode> {
        (self.top() != Mode::Composer).then(|| self.top())
    }

    pub fn open(&mut self, mode: Mode) {
        if self.top() != mode {
            self.modes.push(mode);
        }
    }

    pub fn close(&mut self) {
        if self.modes.len() > 1 {
            self.modes.pop();
        }
    }

    /// Every mode's name as a heading over the keys it binds, each key in one column and what it does
    /// beside it: a description wider than the card wraps under its own column rather than losing its
    /// tail, because a key without the line that explains it is a key a reader has to try.
    pub fn body(&self, width: u16) -> Vec<ScreenRow> {
        let modes: Vec<Mode> = self.modes().collect();
        let column = Column::of(
            modes
                .iter()
                .flat_map(|mode| mode.table())
                .map(|binding| binding.name),
            width,
            KEY,
            MEANING,
        );
        let mut rows = Vec::new();
        for mode in modes {
            rows.push(list::whole(mode.name(), Look::plain(Ink::Dim)));
            for binding in mode.table() {
                rows.extend(column.hung(binding.name, binding.does));
            }
        }
        rows
    }

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
    fn default() -> Stack {
        Stack::new()
    }
}
