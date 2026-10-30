//! One row of the transcript, and the state a tool call carries while it
//! runs.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::driver::{CallOver, Side};

/// The lines a running call shows beneath its row.
pub const TAIL_LINES: usize = 5;
/// The lines a call keeps of its whole output: an expansion reads this, never
/// the stream, so the entry is bounded the way the spill file is and a thread
/// of five hundred calls is still a transcript and not a session log
/// (issue #44).
pub const FULL_LINES: usize = 400;

#[derive(Debug)]
pub enum Entry {
    Prompt(Prompt),
    /// The answer, and whether it is still arriving: `done` is what the
    /// marker settles on, `failed` is which glyph it settles with, and `took`
    /// is the run's clock frozen at the moment it stopped streaming.
    Answer {
        turn: u32,
        text: String,
        done: bool,
        failed: bool,
        took: Option<Duration>,
    },
    Reasoning {
        turn: u32,
        text: String,
        done: bool,
        /// Whether the reader has it open: a streamed thought shows its tail,
        /// a finished one collapses to one row, and the key opens either.
        open: bool,
    },
    Call(Call),
    Note(Note),
}

/// A turn's opening: what the human typed, and the header that names them —
/// when they asked, and from where.
#[derive(Debug)]
pub struct Prompt {
    pub text: String,
    pub at: String,
    pub cwd: String,
}

#[derive(Debug)]
pub struct Call {
    pub turn: u32,
    pub command: Option<String>,
    pub over: Option<CallOver>,
    pub tail: VecDeque<(Side, String)>,
    /// The whole output, and how much of it was ever given: a bounded tail of
    /// the stream that `Enter` opens in place, whatever the spill did
    /// (issue #44).
    out: VecDeque<(Side, String)>,
    lines: usize,
    open: bool,
    /// Where the untruncated streams were written, when they did not fit.
    /// The frame offers them rather than holding them.
    pub spill: Vec<PathBuf>,
    pub truncated: bool,
    /// The clock when the call was announced, and the duration it ended on:
    /// a running call counts up from the first, a finished one is frozen at
    /// the second.
    pub started: Duration,
    pub took: Option<Duration>,
}

impl Call {
    pub fn new(turn: u32, command: Option<String>, started: Duration) -> Self {
        Call {
            turn,
            command,
            over: None,
            tail: VecDeque::with_capacity(TAIL_LINES),
            out: VecDeque::new(),
            lines: 0,
            open: false,
            spill: Vec::new(),
            truncated: false,
            started,
            took: None,
        }
    }

    /// How long the call has been running, or how long it took when it is
    /// over and that is the answer that does not move.
    pub fn elapsed(&self, now: Duration) -> Duration {
        self.took
            .unwrap_or_else(|| now.saturating_sub(self.started))
    }

    pub fn spill_file(&self) -> Option<&Path> {
        if !self.truncated {
            return None;
        }
        self.spill.first().map(PathBuf::as_path)
    }

    pub fn push(&mut self, side: Side, line: String) {
        if line.trim().is_empty() {
            return;
        }
        self.lines += 1;
        self.out.push_back((side, line.clone()));
        while self.out.len() > FULL_LINES {
            self.out.pop_front();
        }
        self.tail.push_back((side, line));
        while self.tail.len() > TAIL_LINES {
            self.tail.pop_front();
        }
    }

    pub fn running(&self) -> bool {
        self.over.is_none()
    }

    /// The output the entry kept, oldest first — what an expansion shows and
    /// what the live tail is the end of.
    pub fn kept(&self) -> &VecDeque<(Side, String)> {
        &self.out
    }

    /// Every line the call was ever given, including the ones the bound has
    /// since dropped: a collapsed row says how much is behind it, not how
    /// much the frame can still show.
    pub fn lines(&self) -> usize {
        self.lines
    }

    /// How many of the oldest lines the bound has dropped.
    pub fn dropped(&self) -> usize {
        self.lines.saturating_sub(self.out.len())
    }

    pub fn expanded(&self) -> bool {
        self.open
    }

    pub fn expandable(&self) -> bool {
        self.lines > 0
    }

    pub fn toggle(&mut self) {
        self.open = !self.open;
    }
}

impl Entry {
    /// Whether `Enter` and a click have anything to open here.
    pub fn expandable(&self) -> bool {
        match self {
            Entry::Call(call) => call.expandable(),
            Entry::Reasoning { text, .. } => !text.trim().is_empty(),
            _ => false,
        }
    }

    /// Opens what is folded, folds what is open, and says whether it found
    /// anything to do it to.
    pub fn toggle(&mut self) -> bool {
        match self {
            Entry::Call(call) if call.expandable() => call.toggle(),
            Entry::Reasoning { open, text, .. } if !text.trim().is_empty() => *open = !*open,
            _ => return false,
        }
        true
    }
}

#[derive(Debug)]
pub struct Note {
    pub kind: NoteKind,
    pub text: String,
}

impl Note {
    pub fn error(text: impl Into<String>) -> Self {
        Note {
            kind: NoteKind::Error,
            text: text.into(),
        }
    }

    pub fn info(text: impl Into<String>) -> Self {
        Note {
            kind: NoteKind::Info,
            text: text.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteKind {
    Error,
    Info,
}
