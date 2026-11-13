use sindri_core::records::Status;

/// `OSC 52 ; c ; <base64> ST`: the terminal at the far end owns the clipboard, so a copy
/// needs no `xclip` and works over SSH. ST rather than BEL, so no terminal rings on a copy.
pub fn copy(text: &str) -> Vec<u8> {
    let mut sequence = Vec::from(&b"\x1b]52;c;"[..]);
    sequence.extend_from_slice(base64(text.as_bytes()).as_bytes());
    sequence.extend_from_slice(b"\x1b\\");
    sequence
}

/// `OSC 66 ; s=<scale> ; <words> ST`: the words drawn over `scale` cells each way, so a heading of
/// them stands twice the cell high and wide. The metadata names the scale alone, because a terminal
/// splits the words into cells as it would without the sequence, and the row was measured so.
pub fn sized(scale: u16, words: &str) -> Vec<u8> {
    sequence(format!("\x1b]66;s={scale};").as_bytes(), words)
}

/// `OSC 8 ; ; <destination> ST`: the cells written after it are a way to somewhere until the
/// same sequence with no destination closes them. A destination is words a wire wrote, so no
/// glyph of it is written as part of the sequence it is carried in.
pub fn link(destination: &str) -> Vec<u8> {
    sequence(b"\x1b]8;;", destination)
}

/// `OSC 2 ; <workspace> — <state> ST`: the title the terminal wears while a launch holds it, so a
/// reader with six windows open can tell which one a run is working in.
pub fn title(workspace: &str, state: &str) -> Vec<u8> {
    sequence(b"\x1b]2;", &format!("{workspace} \u{2014} {state}"))
}

/// `OSC 9 ; 4 ; 3 ST` for a run that is streaming and `; 0` for one that is not: the terminal's own
/// progress mark, indeterminate because a run's turns are a count and never a share of anything.
pub fn progress(marked: bool) -> Vec<u8> {
    match marked {
        true => MARK.to_vec(),
        false => CLEAR.to_vec(),
    }
}

/// `OSC 9 ; <words> ST`: the notification a terminal hands the desktop, which draws it beside a
/// reader who is looking at something else.
pub fn notification(words: &str) -> Vec<u8> {
    // The mark shares this sequence and is told apart by the state digit after it, so a body that
    // opened with one would be read as a mark: the space is what keeps the two apart.
    let lead = match words.starts_with(|glyph: char| glyph.is_ascii_digit()) {
        true => " ",
        false => "",
    };
    sequence(b"\x1b]9;", &format!("{lead}{words}"))
}

const MARK: &[u8] = b"\x1b]9;4;3\x1b\\";

const CLEAR: &[u8] = b"\x1b]9;4;0\x1b\\";

/// A sequence's opening, the words it carries, and `ST` after them. The words are filtered of every
/// control glyph, because a directory named with an `ESC` would otherwise end the sequence it is
/// carried in and begin one of its own.
fn sequence(opening: &[u8], words: &str) -> Vec<u8> {
    let mut sequence = Vec::from(opening);
    sequence.extend(
        words
            .chars()
            .filter(|glyph| !glyph.is_control())
            .collect::<String>()
            .bytes(),
    );
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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    use sindri_core::records::Told;
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
    fn a_sized_run_leaves_as_the_sequence_that_names_its_scale_and_the_words_it_draws() {
        assert_eq!(
            sized(2, "The fence rule"),
            b"\x1b]66;s=2;The fence rule\x1b\\".to_vec(),
            "OSC 66, the scale as metadata, the words, ST"
        );
    }

    #[test]
    fn a_link_leaves_as_the_sequence_that_opens_it_and_the_empty_one_that_closes_it() {
        assert_eq!(
            link("https://example.test/spec"),
            b"\x1b]8;;https://example.test/spec\x1b\\".to_vec(),
            "OSC 8, no params, the destination, ST"
        );
        assert_eq!(
            link(""),
            b"\x1b]8;;\x1b\\".to_vec(),
            "and no destination at all, which closes the one before it"
        );
    }

    #[test]
    fn a_destination_that_carries_a_sequence_of_its_own_cannot_end_the_one_it_is_in() {
        assert_eq!(
            link("https://example.test/\x1b]8;;\x07evil"),
            b"\x1b]8;;https://example.test/]8;;evil\x1b\\".to_vec(),
            "a destination is words the wire wrote: no glyph of it ends the sequence that carries it"
        );
    }

    #[test]
    fn the_title_names_the_workspace_and_the_state() {
        assert_eq!(
            title("sindri", "working"),
            b"\x1b]2;sindri \xe2\x80\x94 working\x1b\\".to_vec(),
            "OSC 2, the workspace, the state, ST"
        );
    }

    #[test]
    fn a_progress_mark_is_learned_and_cleared_as_the_terminals_own() {
        assert_eq!(
            progress(true),
            b"\x1b]9;4;3\x1b\\".to_vec(),
            "OSC 9;4, the indeterminate state: a run's turns are a count, never a share of anything"
        );
        assert_eq!(
            progress(false),
            b"\x1b]9;4;0\x1b\\".to_vec(),
            "and nought, which is the state that takes the mark away"
        );
    }

    #[test]
    fn a_notification_carries_the_words_and_never_reads_as_the_mark_sharing_its_sequence() {
        assert_eq!(
            notification("sindri: answered"),
            b"\x1b]9;sindri: answered\x1b\\".to_vec(),
            "OSC 9, the words, ST"
        );
        assert_eq!(
            notification("4;2 done"),
            b"\x1b]9; 4;2 done\x1b\\".to_vec(),
            "a body that opened with a state digit would be read as the progress mark, so the \
             space before it is what keeps the two apart"
        );
    }

    #[test]
    fn a_window_named_with_a_sequence_of_its_own_cannot_end_the_one_it_is_in() {
        assert_eq!(
            title("s\x1b]0;pwned\x07", "idle"),
            b"\x1b]2;s]0;pwned \xe2\x80\x94 idle\x1b\\".to_vec(),
            "a workspace is a directory name: no glyph of one ends the sequence that carries it"
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
    fn the_words_are_padded_out_to_whole_groups_and_survive_their_bytes() {
        assert_eq!(base64(b"hi"), "aGk=");
        assert_eq!(base64(b"why"), "d2h5");
        assert_eq!(base64(b"one\ntwo"), "b25lCnR3bw==");
        assert_eq!(base64(b""), "");
    }
}
