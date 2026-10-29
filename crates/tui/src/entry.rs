//! One row of the transcript, and the state a tool call carries while it runs.

use std::collections::VecDeque;

use crate::driver::CallOver;

/// How many lines of a running command's output the frame keeps.
pub const TAIL_LINES: usize = 5;

/// One entry in the transcript, in the order it happened.
#[derive(Debug)]
pub enum Entry {
    /// What the user asked.
    Prompt(String),
    /// The model's answer, as it streams in.
    Answer {
        turn: u32,
        text: String,
        /// Whether the turn that produced it has ended.
        done: bool,
    },
    /// The model's reasoning, dim and collapsed once the turn ends.
    Reasoning {
        turn: u32,
        text: String,
        /// Whether the turn that produced it has ended.
        done: bool,
    },
    /// One `bash` call, running or over.
    Call(Call),
    /// Something the frame has to say: a failure, a cancellation.
    Note(Note),
}

/// One `bash` call: what was asked, how it ended, and the tail of its output
/// while it runs (ADR-0011 §9).
#[derive(Debug)]
pub struct Call {
    /// The turn it belongs to.
    pub turn: u32,
    /// The script, when the call's arguments could be read.
    pub command: Option<String>,
    /// How it ended, or `None` while it runs.
    pub over: Option<CallOver>,
    /// The last few lines of its output, oldest first.
    pub tail: VecDeque<String>,
}

impl Call {
    /// A call that has not finished and has no output yet.
    pub fn new(turn: u32, command: Option<String>) -> Self {
        Call {
            turn,
            command,
            over: None,
            tail: VecDeque::with_capacity(TAIL_LINES),
        }
    }

    /// One more line of output, keeping only the last few.
    pub fn push(&mut self, line: String) {
        if line.trim().is_empty() {
            return;
        }
        self.tail.push_back(line);
        while self.tail.len() > TAIL_LINES {
            self.tail.pop_front();
        }
    }

    /// Whether the call is still running.
    pub fn running(&self) -> bool {
        self.over.is_none()
    }
}

/// Something the frame says that is not the model's: a failure, a cancellation.
#[derive(Debug)]
pub struct Note {
    /// Which kind of thing it is.
    pub kind: NoteKind,
    /// The words themselves.
    pub text: String,
}

impl Note {
    /// A failure, said where it happened.
    pub fn error(text: impl Into<String>) -> Self {
        Note {
            kind: NoteKind::Error,
            text: text.into(),
        }
    }

    /// Something the user should know but need not act on.
    pub fn info(text: impl Into<String>) -> Self {
        Note {
            kind: NoteKind::Info,
            text: text.into(),
        }
    }
}

/// The two kinds of note the frame writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteKind {
    /// Something went wrong.
    Error,
    /// Something happened.
    Info,
}
