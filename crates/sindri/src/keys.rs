use crate::composer::Key;

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

    pub fn name(self) -> &'static str {
        match self {
            Mode::Composer => "composer",
            Mode::Keys => "keys",
            Mode::Files => "files",
            Mode::Models => "model",
            Mode::Settings => "settings",
            Mode::Stats => "stats",
        }
    }

    /// The two labels a screen wears on its card: its name, and the key its own table binds to
    /// leaving it. A draft has neither, because it is no screen.
    pub fn labels(self) -> Option<(&'static str, &'static str)> {
        let leaves = self
            .table()
            .iter()
            .find(|binding| binding.act == Act::Close)?;
        Some((self.title()?, leaves.name))
    }

    /// The name a screen wears in its card's top border, and `None` for the draft, which is no
    /// screen and takes no card.
    pub fn title(self) -> Option<&'static str> {
        match self {
            Mode::Composer => None,
            Mode::Keys => Some("? keys"),
            Mode::Files => Some("@ files"),
            Mode::Models => Some("/model"),
            Mode::Settings => Some("/settings"),
            Mode::Stats => Some("/stats"),
        }
    }

    pub fn table(self) -> &'static [Binding] {
        match self {
            Mode::Composer => &COMPOSER,
            Mode::Keys => &KEYS,
            Mode::Files => &FILES,
            Mode::Models => &MODELS,
            Mode::Settings => &SETTINGS,
            Mode::Stats => &STATS,
        }
    }

    /// A screen has none — nothing is typed into a screen — where a draft, a query and a
    /// settings value fall to that text box's own map.
    fn fallback(self) -> Option<Act> {
        match self {
            Mode::Composer => Some(Act::Draft),
            Mode::Files | Mode::Models => Some(Act::Query),
            Mode::Settings => Some(Act::Value),
            Mode::Keys | Mode::Stats => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    Draft,
    Up,
    Copy,
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Binding {
    pub key: Key,
    pub name: &'static str,
    pub does: &'static str,
    pub act: Act,
}

const COMPOSER: [Binding; 17] = [
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
