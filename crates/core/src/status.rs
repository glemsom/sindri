//! What a run tells its terminal it is doing, as one `OSC 7501` report: the
//! bytes and nothing else. The encoder is pure — it takes no `Write`, owns no
//! tty and reads no clock — because the core knows nothing about a terminal, and
//! the writing belongs to whoever does hold one: the frame's run and batch
//! mode's stderr.
//!
//! A report is a second output channel, and one `record/redact.rs` cannot guard:
//! it is written to a tty while the run happens, into whatever that terminal
//! logs. So nothing worth redacting is ever put in it — the message is our own
//! coarse words and the turn count, never the command, the prompt or the answer.
//! It is also refused rather than shortened when a part passes the
//! specification's ceiling, because a terminal discards a report it cannot parse
//! whole: half a report is a terminal told nothing and a run that thinks it spoke.
//!
//! The base64 alphabet both of a terminal's escapes are written in lives here
//! (`base64`), because a report's values and an OSC 52 copy's payload are the
//! same spelling of the same fact and a second copy of it is the one that drifts.

use std::time::Duration;

use crate::agent::Stopped;
use crate::provider::Asked;

/// The ceiling on one whole sequence, escape bytes included.
pub const SEQUENCE_CEILING: usize = 4096;
/// A message's ceiling as a terminal reads it, before base64.
pub const MSG_CEILING: usize = 2048;
/// And the same ceiling on the base64 payload it arrives as.
pub const MSG_ENCODED_CEILING: usize = 2732;
/// A title's ceiling, before base64.
pub const TITLE_CEILING: usize = 192;
/// And on the base64 payload.
pub const TITLE_ENCODED_CEILING: usize = 256;
/// The application's ceiling. `app` is the one plain value, so it is the one a
/// `:` could break; it is a constant of ours with no punctuation in it.
pub const APP_CEILING: usize = 32;

/// The name a terminal keys the root record by: the binary's, never the release.
const APP: &str = "sindri";
/// What ends the sequence: ST in its escape form, which is unambiguously not a
/// value's last base64 character.
const END: &str = "\x1b\\";

/// One of the five states a report may hold. `blocked` is in the set and is
/// never sent: nothing in a turn waits on a human, since the sandbox asks no
/// permission and the tools ask no confirmation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// Nothing is running, or an interrupt left the terminal as it found it.
    Idle,
    /// A turn is being spent, and the words say on what.
    Working {
        doing: Doing,
        turn: u32,
        elapsing: Duration,
    },
    /// The answer is written, whichever way the loop was told to stop.
    Done,
    /// The specification's word for a program waiting on a human. Nothing sends
    /// it; it is here so the set is the specification's and no sixth word can
    /// stand in for it.
    Blocked,
    /// The run could not deliver.
    Error,
}

impl State {
    /// The state a run ends in. The mapping is `Stopped`'s and it is closed:
    /// a run that spent its cap or its budget delivered an answer and says
    /// `done`, and an interrupt says `idle` because a terminal holding
    /// `working` for a process that has gone is a terminal lying about a
    /// machine.
    pub const fn settled(stopped: Stopped) -> Self {
        match stopped {
            Stopped::Final | Stopped::MaxTurns | Stopped::Budget => State::Done,
            Stopped::Cancelled => State::Idle,
            Stopped::Error => State::Error,
        }
    }

    /// The state's own word, which is the report's `state=` value.
    pub const fn word(self) -> &'static str {
        match self {
            State::Idle => "idle",
            State::Working { .. } => "working",
            State::Done => "done",
            State::Blocked => "blocked",
            State::Error => "error",
        }
    }

    /// One coarse line of our own words: what is happening, and how long it has
    /// been happening. Never the command, the prompt or the answer.
    fn msg(self) -> String {
        match self {
            // An interrupt leaves the terminal as it found it, holding
            // nothing: a report from a process that has gone cannot say it
            // waits for anything.
            State::Idle => "nothing is running".to_string(),
            State::Working {
                doing,
                turn,
                elapsing,
            } => format!("turn {turn} · {} · {}s", doing.as_str(), elapsing.as_secs()),
            State::Done => "the answer is written".to_string(),
            State::Blocked => "waiting for a human".to_string(),
            State::Error => "the run could not deliver".to_string(),
        }
    }
}

/// What a working run is doing, as a closed set of our own words: the point of
/// the feature is *what*, not only that something is busy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Doing {
    /// There is no tool to name yet, which is most of a turn.
    Thinking,
    /// One script is being run behind the sandbox.
    Command,
    /// A skill is being loaded into this conversation.
    Skill,
    /// A target is being handed to the human's browser.
    Open,
    /// The turn's model call has finished and the answer is arriving.
    Answer,
}

impl Doing {
    /// What a tool call means in our words. A call no tool could answer never
    /// ran, so the run is back to thinking about the reason it was told.
    pub const fn of(asked: &Asked) -> Self {
        match asked {
            Asked::Bash { .. } => Doing::Command,
            Asked::Skill { .. } => Doing::Skill,
            Asked::Open { .. } => Doing::Open,
            Asked::Refused { .. } => Doing::Thinking,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Doing::Thinking => "thinking",
            Doing::Command => "running a command",
            Doing::Skill => "loading a skill",
            Doing::Open => "opening a target",
            Doing::Answer => "writing the answer",
        }
    }
}

/// The bytes of one report for `state`, told by a run standing in `title`, or
/// nothing at all when a part breaks a ceiling the terminal would discard the
/// whole sequence for. A front end writes these and nothing else: the escape is
/// not its business to spell.
pub fn report(state: &State, title: &str) -> Option<Vec<u8>> {
    escape(state.word(), APP, title, &state.msg())
}

/// One sequence from the four values the wire carries, checked against every
/// ceiling of the specification and encoded, and `None` if any of them refuses
/// it. The state word arrives as a `&str` rather than a `State` because it is
/// the wire's value and not a reading of one: a front end has nothing to say
/// here, and the whole-sequence ceiling is reachable only over a word longer
/// than any `State` holds, so the rule is checked where the bytes are composed.
fn escape(state: &str, app: &str, title: &str, msg: &str) -> Option<Vec<u8>> {
    let title = value(title, TITLE_CEILING, TITLE_ENCODED_CEILING)?;
    let msg = value(msg, MSG_CEILING, MSG_ENCODED_CEILING)?;
    if !plain(app, APP_CEILING) {
        return None;
    }
    let bytes =
        format!("\x1b]7501;state={state}:app={app}:title={title}:msg={msg}{END}").into_bytes();
    (bytes.len() <= SEQUENCE_CEILING).then_some(bytes)
}

/// One base64 value a terminal can read, or `None` if it is past either of its
/// ceilings or carries a control character. Base64 is what keeps a `:` or a `;`
/// out of the wire — the whole point of the encoding — so the decoded text is
/// what is held to the character rule, and a value with a newline in it is never
/// a value that reaches a scrollback. The pair of ceilings is the
/// specification's two spellings of one length: base64 of 2048 bytes is exactly
/// 2732 and of 192 exactly 256, so both are checked and neither can differ from
/// the other without the other saying so.
fn value(text: &str, decoded: usize, encoded: usize) -> Option<String> {
    if text.len() > decoded || text.chars().any(char::is_control) {
        return None;
    }
    let payload = base64(text.as_bytes());
    (payload.len() <= encoded).then_some(payload)
}

/// A value a terminal reads as it is: a length ceiling and no punctuation that
/// would end it early.
fn plain(text: &str, ceiling: usize) -> bool {
    text.len() <= ceiling
        && !text.contains([':', ';'])
        && !text.chars().any(char::is_control)
        && !text.is_empty()
}

/// Base64 in the one alphabet a terminal reads, spelled by the crate that writes
/// it: an `OSC 7501` value is base64 and so is an `OSC 52` payload, and the
/// alphabet is a fact about the terminal rather than a rule of ours to keep.
pub fn base64(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

#[cfg(test)]
mod tests {
    //! The ceilings, at the one function that holds them. A report no state can
    //! build is still a report the check has to refuse: the message is ours, so
    //! a message past its ceiling has no public caller, and the sequence's own
    //! ceiling is reachable only over a field the specification does not cap.

    use super::{APP_CEILING, MSG_CEILING, SEQUENCE_CEILING, TITLE_CEILING, base64, escape, plain};

    #[test]
    fn base64_matches_the_alphabet_every_terminal_expects() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
        assert_eq!(base64(b"hello"), "aGVsbG8=");
    }

    #[test]
    fn an_app_past_its_ceiling_is_refused() {
        let long = "a".repeat(APP_CEILING + 1);
        assert_eq!(escape("done", &long, "sindri", "done"), None);
        assert!(escape("done", "sindri", "sindri", "done").is_some());
    }

    #[test]
    fn a_message_past_either_ceiling_is_refused() {
        let long = "m".repeat(MSG_CEILING + 1);
        assert_eq!(escape("working", "sindri", "sindri", &long), None);
        assert!(escape("working", "sindri", "sindri", &"m".repeat(MSG_CEILING)).is_some());
    }

    #[test]
    fn a_title_past_its_ceiling_is_refused() {
        let long = "t".repeat(TITLE_CEILING + 1);
        assert_eq!(escape("done", "sindri", &long, "done"), None);
        assert!(escape("done", "sindri", &"t".repeat(TITLE_CEILING), "done").is_some());
    }

    #[test]
    fn a_report_past_the_whole_sequence_ceiling_is_refused() {
        // Every value inside its own ceiling, and the sequence still past 4096:
        // the word the specification does not cap is what pushes it over.
        let long = "s".repeat(SEQUENCE_CEILING);
        assert_eq!(escape(&long, "sindri", "sindri", "done"), None);
    }

    #[test]
    fn a_message_carrying_a_newline_is_refused() {
        assert_eq!(escape("working", "sindri", "sindri", "two\nlines"), None);
        assert_eq!(escape("working", "sindri", "sindri", "bell\u{7}"), None);
    }

    #[test]
    fn each_decoded_ceiling_is_the_encoded_one_it_is_sent_as() {
        let title = base64(&[b't'; TITLE_CEILING]);
        assert_eq!(title.len(), super::TITLE_ENCODED_CEILING);
        let msg = base64(&[b'm'; MSG_CEILING]);
        assert_eq!(msg.len(), super::MSG_ENCODED_CEILING);
    }

    #[test]
    fn a_plain_value_ends_early_on_a_colon_or_a_semicolon() {
        for refused in ["sin:dri", "sin;dri", "", "sin\ndri"] {
            assert!(!plain(refused, APP_CEILING), "{refused:?} is not a value");
        }
        assert!(plain("sindri", APP_CEILING));
    }
}
