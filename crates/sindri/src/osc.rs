use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use sindri_core::records::{Status, Told};
use sindri_core::status::Throttle;

/// `OSC 52 ; c ; <base64> ST`: the terminal at the far end owns the clipboard, so a copy
/// needs no `xclip` and works over SSH. ST rather than BEL, so no terminal rings on a copy.
pub fn copy(text: &str) -> Vec<u8> {
    let mut sequence = Vec::from(&b"\x1b]52;c;"[..]);
    sequence.extend_from_slice(base64(text.as_bytes()).as_bytes());
    sequence.extend_from_slice(b"\x1b\\");
    sequence
}

/// `OSC 7501 ; <document> ST`, the document being the JSON `status.json` holds: a reader takes
/// it whole after the single `;`, so a workspace named `weird;dir` cannot split it. Written
/// unqueried and always, because a detection query needs a stdin read a filter does not do.
pub fn status(report: &Status) -> Vec<u8> {
    let mut sequence = Vec::from(&b"\x1b]7501;"[..]);
    sequence.extend_from_slice(&report.json());
    sequence.extend_from_slice(b"\x1b\\");
    sequence
}

fn base64(text: &[u8]) -> String {
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
                // A group of one or two bytes is padded to four seats: the decoder counts seats.
                encoded.push('=');
            }
        }
    }
    encoded
}

/// The clock is `Instant::now`, because a pace is worth nothing but in real time; what is
/// decided about the pace is the core's [`Throttle`], which takes its clock as an argument.
pub struct Teller<W: Write> {
    stream: W,
    on: bool,
    workspace: PathBuf,
    said: Throttle,
}

impl<W: Write> Teller<W> {
    pub fn new(stream: W, on: bool, workspace: &Path) -> Teller<W> {
        Teller {
            stream,
            on,
            workspace: workspace.to_path_buf(),
            said: Throttle::new(),
        }
    }

    /// A `status` key turned off mid-launch stops the telling at the next report.
    pub fn switch(&mut self, on: bool) {
        self.on = on;
    }

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

    /// Says the report the pace withheld: a front end with a clock calls this whenever it
    /// wakes, so a withheld report is said late rather than lost.
    pub fn due(&mut self) -> io::Result<()> {
        if !self.on {
            return Ok(());
        }
        match self.said.due(Instant::now()) {
            Some(report) => self.write(&report),
            None => Ok(()),
        }
    }

    /// Flushed, because a terminal waiting for the rest of a sequence reads the frame as a
    /// smear.
    fn write(&mut self, report: &Status) -> io::Result<()> {
        self.stream.write_all(&status(report))?;
        self.stream.flush()
    }
}

#[cfg(test)]
mod tests {
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
        // A reader splits at the single `;` after the number, so `weird;dir` stays a character.
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
