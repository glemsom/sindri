//! Escape sequences that carry words out of the terminal are written here — the copy
//! that puts words on a clipboard, and the status that says what a run is doing — so
//! the interface never spells one out at a call site.
//!
//! A sequence is built as bytes and handed back: the writing belongs to whoever owns the
//! tty (§14), which is this crate's terminal and not the core, and a [`Teller`] is that
//! writer — the one place a status meets a stream. That is also what makes a copy work
//! over SSH — the bytes go down the same wire the frame does, and the terminal at the near
//! end owns the clipboard, so no `xclip`, `wl-copy` or `pbcopy` has to be installed, found
//! or trusted (§12, §17).

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use sindri_core::records::{Status, Told};
use sindri_core::status::Throttle;

/// The bytes a copy leaves by: `OSC 52 ; c ; <base64> ST`, which every terminal that
/// speaks OSC 52 reads as "put these words in the reader's clipboard" (§12). `c` is the
/// system clipboard; the payload is base64 because the sequence carries no raw bytes of
/// its own, and it ends with ST rather than BEL so no terminal rings on a copy.
pub fn copy(text: &str) -> Vec<u8> {
    let mut sequence = Vec::from(&b"\x1b]52;c;"[..]);
    sequence.extend_from_slice(base64(text.as_bytes()).as_bytes());
    sequence.extend_from_slice(b"\x1b\\");
    sequence
}

/// The bytes a status leaves by: `OSC 7501 ; <document> ST`, which a terminal that
/// speaks OSC 7501 reads as what the run is doing and every other terminal ignores at
/// the cost of a pipe (§14).
///
/// The payload is the status document itself — the same four facts `status.json` keeps,
/// in the same shape — so the two windows onto a run cannot disagree about it, and no
/// field can be read as another: a workspace named `weird;dir` would otherwise split a
/// semicolon-separated sequence into fields it never meant.
///
/// It is written unqueried and always: a detection query needs a stdin read a filter
/// does not do and false-negatives on the terminals we run in, so the sequence goes out
/// whether the terminal wants it or not, and an unsupported terminal drops it.
pub fn status(report: &Status) -> Vec<u8> {
    let mut sequence = Vec::from(&b"\x1b]7501;"[..]);
    sequence.extend_from_slice(&report.json());
    sequence.extend_from_slice(b"\x1b\\");
    sequence
}

/// `text` as base64, the standard alphabet, padded: the one encoding an OSC 52 reader
/// decodes, written here rather than taken as a dependency because it is twelve lines and
/// a crate for them would be a crate for everything.
fn base64(text: &[u8]) -> String {
    /// The alphabet RFC 4648 names, in the order it names it.
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut encoded = String::with_capacity(text.len().div_ceil(3) * 4);
    for chunk in text.chunks(3) {
        let byte = |at: usize| u32::from(chunk.get(at).copied().unwrap_or(0));
        let triple = (byte(0) << 16) | (byte(1) << 8) | byte(2);
        for seat in 0..4u32 {
            if usize::try_from(seat).unwrap_or(4) <= chunk.len() {
                let index = (triple >> (18 - seat * 6)) & 0x3f;
                encoded.push(char::from(ALPHABET[index as usize]));
            } else {
                // A group of one or two bytes is padded out to four seats: the decoder a
                // terminal uses counts seats and drops the padding.
                encoded.push('=');
            }
        }
    }
    encoded
}

/// A terminal that is told what the run is doing: the sequence, §14's pace and the one
/// stream, so a front end hands over a fact and spells nothing out.
///
/// The clock is `Instant::now`, because a pace is worth nothing but in real time; what
/// is *decided* about the pace is the core's [`Throttle`], which takes its clock as an
/// argument so a test can walk a second in microseconds.
pub struct Teller<W: Write> {
    /// Where the sequences go, when they go anywhere at all.
    stream: W,
    /// The one switch: the `status` key, on by default (§11, §14).
    on: bool,
    /// The workspace the reports are about, whose name is its last component.
    workspace: PathBuf,
    /// What has been told, and what the pace is holding.
    said: Throttle,
}

impl<W: Write> Teller<W> {
    /// A teller on `stream`, on when `on`, about a run standing in `workspace`.
    pub fn new(stream: W, on: bool, workspace: &Path) -> Teller<W> {
        Teller {
            stream,
            on,
            workspace: workspace.to_path_buf(),
            said: Throttle::new(),
        }
    }

    /// Throws §14's one switch, read again when a settings visit rewrote it: a `status`
    /// key turned off mid-launch stops the telling at the next report, which is what a
    /// reader who just turned it off expects (ADR-0009).
    pub fn switch(&mut self, on: bool) {
        self.on = on;
    }

    /// Says what the run is doing now, if the switch is on and the pace allows it.
    pub fn say(&mut self, told: Told) -> io::Result<()> {
        if !self.on {
            return Ok(());
        }
        let report = Status::new(told, &self.workspace);
        match self.said.offer(report, Instant::now()) {
            Some(report) => self.write(&report),
            None => Ok(()),
        }
    }

    /// Says the report a rate limit withheld, once its second is up: a front end with a
    /// clock in its hand calls this whenever it wakes, so a withheld report is said late
    /// rather than lost (§14).
    pub fn due(&mut self) -> io::Result<()> {
        if !self.on {
            return Ok(());
        }
        match self.said.due(Instant::now()) {
            Some(report) => self.write(&report),
            None => Ok(()),
        }
    }

    /// Writes one report and pushes it out: a status nobody reads is worthless, and a
    /// terminal waiting for the rest of a sequence reads the frame as a smear.
    fn write(&mut self, report: &Status) -> io::Result<()> {
        self.stream.write_all(&status(report))?;
        self.stream.flush()
    }
}

#[cfg(test)]
mod tests {
    //! What a copy really writes, in bytes: the sequence is a wire format, and a wire
    //! format checked against a re-run of its own encoder would pass whatever it did.

    use super::*;

    use sindri_core::redact::SecretSet;
    use sindri_core::run::Stop;

    #[test]
    fn a_copy_leaves_as_the_sequence_that_carries_words_out() {
        assert_eq!(
            copy("sindri"),
            b"\x1b]52;c;c2luZHJp\x1b\\".to_vec(),
            "OSC 52, the system clipboard, the words as base64, ST"
        );
    }

    #[test]
    fn a_status_leaves_as_the_sequence_that_carries_what_the_run_is_doing() {
        let report = Status::new(Told::Asking { turn: 3 }, Path::new("/work/sindri"));
        assert_eq!(
            status(&report),
            b"\x1b]7501;{\"phase\":\"working\",\"message\":\"asking the model\",\"turn\":3,\"workspace\":\"sindri\"}\x1b\\"
                .to_vec(),
            "OSC 7501, the status document, ST"
        );
    }

    #[test]
    fn a_status_is_neither_a_progress_report_nor_a_query() {
        let told = [
            Told::Idle,
            Told::Asking { turn: 12 },
            Told::Waiting { turn: 12 },
            Told::Stopped {
                turn: 12,
                stop: Stop::MaxTurns,
            },
            Told::Stopped {
                turn: 12,
                stop: Stop::Error(SecretSet::default().redact("the wire said no")),
            },
        ];

        for told in told {
            let sequence = String::from_utf8(status(&Status::new(
                told.clone(),
                Path::new("/work/sindri"),
            )))
            .expect("a sequence is UTF-8");
            assert!(
                sequence.starts_with("\x1b]7501;") && sequence.ends_with("\x1b\\"),
                "{told:?} is one OSC sequence and nothing else: {sequence:?}"
            );
            assert!(
                !sequence.contains("progress") && !sequence.contains("9;4"),
                "never a progress report: {sequence:?}"
            );
            assert!(
                !sequence.contains('?'),
                "and nothing a terminal would have to answer: {sequence:?}"
            );
        }
    }

    #[test]
    fn a_workspace_named_with_a_semicolon_cannot_split_the_sequence() {
        // A reader splits the sequence at the single `;` after the number and takes the
        // rest whole, so a directory called `weird;dir` is a character inside the document
        // rather than a field boundary the sequence never meant.
        let sequence = String::from_utf8(status(&Status::new(
            Told::Idle,
            Path::new("/work/weird;dir"),
        )))
        .expect("a sequence is UTF-8");
        let document = sequence
            .strip_prefix("\x1b]7501;")
            .and_then(|rest| rest.strip_suffix("\x1b\\"))
            .expect("OSC 7501 before the document and ST after it");

        let read: serde_json::Value =
            serde_json::from_str(document).expect("the document reads whole");
        assert_eq!(read["workspace"], "weird;dir", "{sequence:?}");
    }

    #[test]
    fn a_teller_that_is_off_says_nothing_at_all() {
        let mut written = Vec::new();
        let mut teller = Teller::new(&mut written, false, Path::new("/work/sindri"));

        teller.say(Told::Idle).expect("a teller that is off");
        assert!(written.is_empty(), "nothing was written: {written:?}");
    }

    #[test]
    fn a_teller_says_one_state_once() {
        let mut written = Vec::new();
        {
            let mut teller = Teller::new(&mut written, true, Path::new("/work/sindri"));
            teller
                .say(Told::Asking { turn: 1 })
                .expect("the first report");
            teller
                .say(Told::Asking { turn: 2 })
                .expect("a turn inside the second");
            teller
                .say(Told::Asking { turn: 1 })
                .expect("the state already told");
        }
        let text = String::from_utf8(written).expect("a sequence is UTF-8");

        assert_eq!(
            text.matches("\x1b]7501;").count(),
            1,
            "one state, said once: {text:?}"
        );
        assert!(text.contains("\"turn\":1"), "{text:?}");
    }

    #[test]
    fn the_words_are_padded_out_to_whole_groups_and_survive_their_bytes() {
        assert_eq!(base64(b"hi"), "aGk=");
        assert_eq!(base64(b"why"), "d2h5");
        assert_eq!(base64(b"one\ntwo"), "b25lCnR3bw==");
        assert_eq!(base64(b""), "");
    }
}
