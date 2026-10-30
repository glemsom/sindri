//! One row of the transcript, and the state a tool call carries while it
//! runs.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use crate::driver::{CallOver, Side};

pub const TAIL_LINES: usize = 5;

#[derive(Debug)]
pub enum Entry {
    Prompt(Prompt),
    Answer { turn: u32, text: String, done: bool },
    Reasoning { turn: u32, text: String, done: bool },
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
    /// Where the untruncated streams were written, when they did not fit
    /// (ADR-0007). The frame offers them rather than holding them.
    pub spill: Vec<PathBuf>,
    pub truncated: bool,
}

impl Call {
    pub fn new(turn: u32, command: Option<String>) -> Self {
        Call {
            turn,
            command,
            over: None,
            tail: VecDeque::with_capacity(TAIL_LINES),
            spill: Vec::new(),
            truncated: false,
        }
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
        self.tail.push_back((side, line));
        while self.tail.len() > TAIL_LINES {
            self.tail.pop_front();
        }
    }

    pub fn running(&self) -> bool {
        self.over.is_none()
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
