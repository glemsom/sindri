//! One row of the transcript, and the state a tool call carries while it
//! runs: the command, the output it kept, and the word its row says it ended
//! on. That word is the row's own — two of them no result carries — so the set
//! and the one function that makes one live beside the row that says it.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::Duration;

use sindri_core::exec::{Outcome, ToolResult};

use crate::driver::Side;

/// What a call's row says happened to it. It is a projection of the result and
/// not a second spelling of it — bar the two words no result carries, a call the
/// run was stopped under and a call with no exit to watch. A word made from an
/// outcome is made by [`CallOver::of`] and nowhere else; the other two are the
/// row's own, set where the row is announced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallOver {
    Exited(i32),
    TimedOut,
    Cancelled,
    Refused(String),
    /// A call with no exit to watch: `open` hands one target to the host's
    /// opener and the loop is already past it, so the row is over the moment it
    /// was announced. What the opener answered is the model's to read, never
    /// something this column can say.
    Announced,
}

impl CallOver {
    /// The row's word for one command's outcome. A result that is not there is
    /// a call the run was stopped under, which is the one thing an `Outcome`
    /// cannot say, and a sandbox that refused the command carries its own
    /// reason as the payload.
    pub fn of(outcome: Option<&Outcome>) -> CallOver {
        match outcome {
            None => CallOver::Cancelled,
            Some(Outcome::Exited { exit_code }) => CallOver::Exited(*exit_code),
            Some(Outcome::TimedOut) => CallOver::TimedOut,
            Some(Outcome::SandboxError { detail }) => CallOver::Refused(detail.clone()),
        }
    }
}

/// The lines a running call shows beneath its row.
pub const TAIL_LINES: usize = 5;
/// The lines a call keeps of its whole output: an expansion reads this, never
/// the stream, so the entry is bounded the way the spill file is and a thread
/// of five hundred calls is still a transcript and not a session log.
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
    /// A thought the run is having, as a row of the thread: a fragment is
    /// appended to `text` as it lands, so no row above it moves while it
    /// streams, and its rows stay where they are when it ends. `started` is the
    /// run's clock when the first fragment landed, which is what its marker
    /// counts from while it is still arriving.
    Thought {
        turn: u32,
        text: String,
        /// Whether the thought has ended: `done` is the marker's word — the run
        /// is thinking, or the thought is over — and `close_thought` is what
        /// sets it.
        done: bool,
        started: Duration,
        /// How long the run spent on it, from the run's own clock, read through
        /// `pulse::said` on the marker row as an answer's is. `None` for a
        /// thought nobody watched arrive: there is no clock to count from, so
        /// the marker carries no duration.
        took: Option<Duration>,
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
    /// Set when the call is the `skill` tool's: which skill, and what the load
    /// answered. A load is the one call whose row says what it made true rather
    /// than what it ran, because it ran nothing.
    pub skill: Option<Skill>,
    pub over: Option<CallOver>,
    pub tail: VecDeque<(Side, String)>,
    /// The whole output, and how much of it was ever given: a bounded tail of
    /// the stream that `Enter` opens in place, whatever the spill did.
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
            skill: None,
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

    /// The one result the call ended on, frozen into the row: what ran, where
    /// the bytes the model was not given went, and when. One value, one place,
    /// so nothing can set three of the four facts a row has about its result.
    pub fn ended(&mut self, result: &ToolResult, now: Duration) {
        self.freeze(
            CallOver::of(Some(&result.outcome)),
            result.truncated,
            result.spill.clone(),
            now,
        );
    }

    /// A call with no exit to watch: a load and an `open` are over the moment
    /// they are announced, so the row says so and spends no clock at all.
    pub fn announced(&mut self) {
        self.took = Some(Duration::ZERO);
        self.over = Some(CallOver::Announced);
    }

    /// A call the run was stopped under: there is no result to read, so the row
    /// takes the one word no result carries and nothing behind it.
    pub fn cancelled(&mut self, now: Duration) {
        self.freeze(CallOver::of(None), false, Vec::new(), now);
    }

    fn freeze(&mut self, over: CallOver, truncated: bool, spill: Vec<PathBuf>, now: Duration) {
        // The duration freezes where the call ended: a finished call is not
        // still counting.
        self.took = Some(now.saturating_sub(self.started));
        self.over = Some(over);
        self.truncated = truncated;
        self.spill = spill;
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
    /// Whether `Enter` and a click have anything to open here. A thought has
    /// not: the whole of it is a row of the thread already, so there is nothing
    /// folded away to open.
    pub fn expandable(&self) -> bool {
        match self {
            Entry::Call(call) => call.expandable(),
            _ => false,
        }
    }

    /// Opens what is folded, folds what is open, and says whether it found
    /// anything to do it to.
    pub fn toggle(&mut self) -> bool {
        match self {
            Entry::Call(call) if call.expandable() => call.toggle(),
            _ => return false,
        }
        true
    }
}

/// A call to the `skill` tool: the name, and the word the result opened with
/// once it has one. The body is kept as the call's output, so the row is the
/// chip and `Enter` is the whole body — never the other way round.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skill {
    pub name: String,
    /// `loaded`, `already_loaded` or `unreadable`: absent while the call is
    /// still being answered.
    pub outcome: Option<String>,
}

impl Skill {
    pub fn new(name: impl Into<String>) -> Skill {
        Skill {
            name: name.into(),
            outcome: None,
        }
    }

    /// What the row says the load did, in the fewest cells: the status column is
    /// eight wide, and `already_loaded` is not a word a row has room for.
    pub fn said(&self) -> &str {
        match self.outcome.as_deref() {
            Some("already_loaded") => "already",
            Some("unreadable") => "unread",
            // A call no tool answered never reached a skill at all.
            None => "not run",
            Some(word) => word,
        }
    }

    /// Whether the load put the skill in force: the one outcome the rail and
    /// the transcript's count of loads are both made from.
    pub fn in_force(&self) -> bool {
        self.outcome.as_deref() == Some("loaded")
    }

    /// Whether the call failed: a skill that could not be read, or a call no
    /// tool answered. A repeat is neither — nothing moved, and nothing went
    /// wrong.
    pub fn failed(&self) -> bool {
        matches!(self.outcome.as_deref(), Some("unreadable") | None)
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

#[cfg(test)]
mod tests {
    #![allow(
        clippy::expect_used,
        reason = "a panicking test is how a broken contract reports itself"
    )]

    use std::path::PathBuf;
    use std::time::Duration;

    use sindri_core::exec::{Outcome, ToolResult};

    use super::{Call, CallOver};

    #[test]
    fn a_command_that_exited_says_its_own_exit_code() {
        assert_eq!(
            CallOver::of(Some(&Outcome::Exited { exit_code: 3 })),
            CallOver::Exited(3),
            "the row repeats the code the command itself reported"
        );
    }

    #[test]
    fn a_command_the_clock_cut_short_says_it_timed_out() {
        assert_eq!(
            CallOver::of(Some(&Outcome::TimedOut)),
            CallOver::TimedOut,
            "a budget spent is a row with its own word, not an exit code"
        );
    }

    #[test]
    fn a_sandbox_that_refused_the_command_carries_its_own_reason() {
        assert_eq!(
            CallOver::of(Some(&Outcome::SandboxError {
                detail: "bwrap could not start".to_string(),
            })),
            CallOver::Refused("bwrap could not start".to_string()),
            "the reason travels as the payload, never as a word the row made up"
        );
    }

    #[test]
    fn a_call_the_run_was_stopped_under_says_cancelled_where_no_outcome_can() {
        assert_eq!(
            CallOver::of(None),
            CallOver::Cancelled,
            "a command killed with the run leaves no result, and the row still says what happened"
        );
    }

    #[test]
    fn one_result_freezes_the_four_facts_a_row_has_about_it() {
        let spill = PathBuf::from("/state/sindri/sessions/4f1c/spill-1.out");
        let result = ToolResult {
            outcome: Outcome::Exited { exit_code: 2 },
            stdout: "…".to_string(),
            stderr: String::new(),
            truncated: true,
            spill: vec![spill.clone()],
            duration_ms: 900,
        };
        let mut call = Call::new(1, Some("seq 500".to_string()), Duration::from_millis(10));

        call.ended(&result, Duration::from_millis(310));

        assert_eq!(call.over, Some(CallOver::Exited(2)), "what ran");
        assert_eq!(
            call.took,
            Some(Duration::from_millis(300)),
            "and when, from the call's own clock rather than the command's"
        );
        assert!(call.truncated, "whether the model's copy was cut down");
        assert_eq!(
            call.spill_file(),
            Some(spill.as_path()),
            "and where the untruncated bytes went"
        );
    }

    #[test]
    fn a_call_cancelled_with_its_run_leaves_nothing_behind_it() {
        let mut call = Call::new(1, Some("sleep 600".to_string()), Duration::ZERO);

        call.cancelled(Duration::from_millis(5));

        assert_eq!(call.over, Some(CallOver::Cancelled));
        assert_eq!(call.took, Some(Duration::from_millis(5)));
        assert!(!call.truncated, "a cancelled command truncated nothing");
        assert!(call.spill.is_empty(), "and left no file to offer");
    }
}
