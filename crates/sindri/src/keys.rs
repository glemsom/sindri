use crate::composer::Key;
use crate::ink::{Faces, Ink, Look};
use crate::list::{self, Column, ScreenRow};

pub const KEY: Look = Look::plain(Ink::Plain).wearing(Faces::BOLD);

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

    pub fn labels(self) -> Option<(&'static str, &'static str)> {
        Some((self.title()?, self.leaves()?))
    }

    pub fn leaves(self) -> Option<&'static str> {
        Some(
            self.description()
                .table
                .iter()
                .find(|binding| binding.act == Act::Close)?
                .name,
        )
    }

    pub fn title(self) -> Option<&'static str> {
        self.description().title
    }

    pub fn table(self) -> &'static [Binding] {
        self.description().table
    }

    pub fn body(self) -> Option<Body> {
        self.description().body
    }

    fn fallback(self) -> Option<Act> {
        self.description().fallback
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    Draft,
    Complete,
    Up,
    Fold,
    Drop,
    Keys,
    Files,
    Close,
    Query,
    Previous,
    Next,
    Pick,
    Choose,
    Flip,
    Clear,
    Write,
    Back,
    Forward,
    Tail,
    Before,
    After,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Binding {
    pub key: Key,
    pub name: &'static str,
    pub does: &'static str,
    pub act: Act,
}

const COMPOSER: [Binding; 24] = [
    Binding {
        key: Key::BackTab,
        name: "Shift+Tab",
        does: "the marked commands row back",
        act: Act::Previous,
    },
    Binding {
        key: Key::Tab,
        name: "Tab",
        does: "complete the marked commands row into the draft",
        act: Act::Complete,
    },
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
        key: Key::Char('['),
        name: "[",
        does: "the prompt before, while the draft is empty",
        act: Act::Before,
    },
    Binding {
        key: Key::Char(']'),
        name: "]",
        does: "the prompt after, while the draft is empty",
        act: Act::After,
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
        key: Key::Fold,
        name: "Ctrl-O",
        does: "open or fold the newest work, call or prompt",
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

const SETTINGS: [Binding; 10] = [
    Binding {
        key: Key::Forward,
        name: "Ctrl-F",
        does: "refresh served names in the model editor",
        act: Act::Forward,
    },
    Binding {
        key: Key::Escape,
        name: "Esc",
        does: "close this screen and write nothing",
        act: Act::Close,
    },
    Binding {
        key: Key::Enter,
        name: "Enter",
        does: "edit or select; Apply saves; Cancel discards",
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
        does: "toggle a switch, or type in an editor",
        act: Act::Flip,
    },
    Binding {
        key: Key::Backspace,
        name: "Backspace",
        does: "erase the last glyph of the value",
        act: Act::Query,
    },
];

const STATS: [Binding; 1] = [Binding {
    key: Key::Escape,
    name: "Esc",
    does: "close this screen, and the draft as it was",
    act: Act::Close,
}];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Body {
    Keys,
    Files,
    Models,
    Settings,
    Ledger,
}

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
        fallback: Some(Act::Query),
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
