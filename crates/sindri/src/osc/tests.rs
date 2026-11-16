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

    let read: serde_json::Value = serde_json::from_str(document).expect("the document reads whole");
    assert_eq!(read["workspace"], "weird;dir", "{sequence:?}");
}

#[test]
fn the_words_are_padded_out_to_whole_groups_and_survive_their_bytes() {
    assert_eq!(base64(b"hi"), "aGk=");
    assert_eq!(base64(b"why"), "d2h5");
    assert_eq!(base64(b"one\ntwo"), "b25lCnR3bw==");
    assert_eq!(base64(b""), "");
}
