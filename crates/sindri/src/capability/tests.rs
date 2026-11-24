use std::cell::RefCell;
use std::collections::BTreeMap;

use super::*;

fn terms(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(key, value)| (String::from(*key), String::from(*value)))
        .collect()
}

fn probed(environment: &BTreeMap<String, String>, asked: &impl Asked) -> Capabilities {
    Capabilities::probed(environment, asked, true).0
}

#[derive(Default)]
struct Scripted {
    size: Option<CellSize>,
    cell: Option<&'static [u8]>,
    sizing: Option<&'static [u8]>,
    animation: Option<&'static [u8]>,
    asked: RefCell<Vec<Question>>,
}

impl Scripted {
    fn asked(&self) -> Vec<Question> {
        self.asked.borrow().clone()
    }
}

impl Asked for Scripted {
    fn winsize(&self) -> Option<CellSize> {
        self.size
    }

    fn ask(&self, question: Question) -> Option<Vec<u8>> {
        self.asked.borrow_mut().push(question);
        match question {
            Question::Cell => self.cell,
            Question::Sizing => self.sizing,
            Question::Animation => self.animation,
        }
        .map(<[u8]>::to_vec)
    }
}

#[test]
fn the_cell_is_the_one_the_terminal_reported() {
    let (capabilities, typed) =
        Capabilities::probed(&terms(&[]), &Answers::at(CellSize::new(9, 18)), true);

    assert_eq!(
        capabilities.cell(),
        CellSize::new(9, 18),
        "the cell a terminal reports is the one a picture is measured against"
    );
    assert!(typed.is_empty(), "and nothing was typed into the launch");
}

#[test]
fn a_terminal_that_says_nothing_is_drawn_at_the_nominal_cell() {
    let capabilities = probed(&terms(&[]), &Answers::silent());

    assert_eq!(
        capabilities.cell(),
        CellSize::NOMINAL,
        "the nominal cell is what a terminal that will not say gets"
    );
}

#[test]
fn a_terminal_that_names_kitty_is_handed_pictures_at_the_cell_it_reported() {
    let capabilities = probed(
        &terms(&[("TERM", "xterm-kitty")]),
        &Answers::at(CellSize::new(9, 18)),
    );

    assert_eq!(
        capabilities.pictures().cell(),
        Some(CellSize::new(9, 18)),
        "a terminal that names Kitty is handed the pictures its frames draw from, measured \
             at the cell it reported"
    );
}

#[test]
fn a_terminal_that_names_no_protocol_is_handed_no_pictures() {
    let capabilities = probed(
        &terms(&[("TERM", "xterm-256color")]),
        &Answers::at(CellSize::NOMINAL),
    );

    assert!(
        !capabilities.pictures().draws(),
        "and one that names no protocol is handed none, which is the words of a picture and \
             no placement"
    );
}

#[test]
fn the_pointer_is_the_one_the_names_vouch_for() {
    assert!(
        probed(&terms(&[("VTE_VERSION", "5000")]), &Answers::silent()).reports_pointer(),
        "a terminal that names a version reporting input reports a pointer"
    );
    assert!(
        !probed(&terms(&[("TERM", "xterm-256color")]), &Answers::silent()).reports_pointer(),
        "and one nobody can vouch for is handed a frame that takes none"
    );
}

#[test]
fn the_palette_is_the_one_the_names_give() {
    let palette = |pairs: &[(&str, &str)]| probed(&terms(pairs), &Answers::silent()).palette();

    assert_eq!(
        palette(&[("COLORTERM", "truecolor")]),
        Palette::TRUECOLOR,
        "the design as it was drawn, for a terminal that says it holds the colours"
    );
    assert_eq!(
        palette(&[("TERM", "xterm-256color")]),
        Palette::ANSI,
        "and the design in the colours a terminal with no twenty-four-bit colour has"
    );
    assert_eq!(
        palette(&[("NO_COLOR", "1"), ("COLORTERM", "truecolor")]),
        Palette::SLOTS,
        "a reader who asked for no colour is given the terminal's own slots"
    );
}

#[test]
fn the_keys_typed_beside_the_answers_come_back_in_the_order_they_were_typed() {
    let scripted = Scripted {
        cell: Some(b"a\x1b[6;18;9tb"),
        sizing: Some(b"c\x1b[1;1R\x1b[1;3Rd"),
        ..Scripted::default()
    };

    let (capabilities, typed) =
        Capabilities::probed(&terms(&[("TERM", "xterm-kitty")]), &scripted, true);

    assert_eq!(capabilities.cell(), CellSize::new(9, 18));
    assert_eq!(
        typed,
        b"abcd".to_vec(),
        "the keys the cell answer left out come first and the sizing answer's after them"
    );
}

#[test]
fn the_face_is_the_one_the_config_asked_for() {
    let (drawn, _) = Capabilities::probed(&terms(&[]), &Answers::silent(), true);
    assert!(
        drawn.draws_face(),
        "a config that asks for the face gets it"
    );

    let (hidden, _) = Capabilities::probed(&terms(&[]), &Answers::silent(), false);
    assert!(
        !hidden.draws_face(),
        "and one that does not is drawn without it"
    );
}

#[test]
fn the_face_a_settings_visit_moves_leaves_every_other_fact_where_it_was() {
    let capabilities = probed(&terms(&[("TERM", "xterm-kitty")]), &Answers::silent());
    let hidden = capabilities.clone().faced(false);

    assert!(!hidden.draws_face(), "the visit turned the face off");
    assert_eq!(hidden.cell(), capabilities.cell(), "and moved no cell");
    assert_eq!(hidden.palette(), capabilities.palette(), "and no colour");
}

#[test]
fn the_pointer_a_reporting_moves_leaves_every_other_fact_where_it_was() {
    let capabilities = probed(&terms(&[("TERM", "xterm-kitty")]), &Answers::silent());
    let reporting = capabilities.clone().reporting(true);

    assert!(reporting.reports_pointer(), "the pointer was taken up");
    assert_eq!(reporting.cell(), capabilities.cell(), "and no cell moved");
    assert_eq!(
        reporting.palette(),
        capabilities.palette(),
        "and no colour moved"
    );
}

#[test]
fn the_cell_a_window_crossed_to_is_the_cell_the_pictures_are_measured_at() {
    let capabilities = probed(
        &terms(&[("TERM", "xterm-kitty")]),
        &Answers::at(CellSize::new(9, 18)),
    );
    let moved = capabilities.clone().celled(CellSize::new(20, 40));

    assert_eq!(
        moved.cell(),
        CellSize::new(20, 40),
        "the cell a frame draws on is the one the window crossed to"
    );
    assert_eq!(
        moved.pictures().cell(),
        Some(CellSize::new(20, 40)),
        "and the pictures are measured at it"
    );
}

#[test]
fn the_cell_a_window_kept_leaves_the_pictures_and_every_other_fact_where_they_were() {
    let capabilities = probed(
        &terms(&[("TERM", "xterm-kitty")]),
        &Answers::at(CellSize::new(9, 18)),
    );
    let kept = capabilities.clone().celled(CellSize::new(9, 18));

    assert_eq!(
        kept.pictures().cell(),
        capabilities.pictures().cell(),
        "a cell that did not move left the pictures where they were"
    );
    assert_eq!(kept.cell(), capabilities.cell(), "and no cell moved");
    assert_eq!(
        kept.palette(),
        capabilities.palette(),
        "and no colour moved"
    );
    assert_eq!(
        kept.sizes_text(),
        capabilities.sizes_text(),
        "and no sizing"
    );
    assert_eq!(
        kept.reports_pointer(),
        capabilities.reports_pointer(),
        "and no pointer"
    );
    assert_eq!(kept.draws_face(), capabilities.draws_face(), "and no face");
}

#[test]
fn the_sizing_question_is_never_put_to_a_terminal_that_names_no_large_text() {
    let scripted = Scripted::default();
    let capabilities = probed(&terms(&[("TERM", "xterm-256color")]), &scripted);

    assert!(
        !capabilities.sizes_text(),
        "a terminal that names no large text is handed the spelling its glyphs are"
    );
    assert!(
        !scripted.asked().contains(&Question::Sizing),
        "and it is never asked whether it does: {:?}",
        scripted.asked()
    );
}

#[test]
fn a_terminal_that_names_large_text_is_asked_and_the_answer_read() {
    let scripted = Scripted {
        sizing: Some(b"\x1b[1;1R\x1b[1;3R"),
        ..Scripted::default()
    };
    let capabilities = probed(&terms(&[("TERM", "xterm-kitty")]), &scripted);

    assert!(
        capabilities.sizes_text(),
        "a terminal that names large text moved its cursor two cells over the block"
    );
    assert_eq!(
        scripted.asked(),
        [Question::Cell, Question::Sizing, Question::Animation],
        "the cell first, then the sizing, and the face's animation last of all: a terminal that \
         speaks Kitty is asked whether it keeps frames and turns them"
    );
}

#[test]
fn a_block_drawn_at_size_moved_the_cursor_two_cells() {
    assert_eq!(
        sizing_reply(b"\x1b[1;1R\x1b[1;3R"),
        (true, Vec::new()),
        "two reports two cells apart are a terminal that drew the space at twice the cell"
    );
}

#[test]
fn a_terminal_that_reads_no_sizing_moved_the_cursor_none() {
    assert_eq!(
        sizing_reply(b"\x1b[1;1R\x1b[1;1R"),
        (false, Vec::new()),
        "a terminal that read the block as one cell drew no large text"
    );
    assert_eq!(
        sizing_reply(b""),
        (false, Vec::new()),
        "and one that answered nothing is handed the spelling its glyphs are"
    );
    assert_eq!(
        sizing_reply(b"\x1b[1;1R"),
        (false, Vec::new()),
        "an answer cut in half is no answer"
    );
}

#[test]
fn the_keys_typed_beside_the_sizing_answer_are_kept_in_the_order_they_were_typed() {
    assert_eq!(
        sizing_reply(b"a\x1b[1;1R\x1b[1;3Rb"),
        (true, b"ab".to_vec()),
        "the two keys that were not the answer are kept, and the answer is still read"
    );
}

#[test]
fn the_cell_is_read_off_the_bytes_the_terminal_sent_and_the_keys_beside_it_kept() {
    let (cell, typed) = cell_reply(b"a\x1b[6;18;9tb");

    assert_eq!(cell, Some(CellSize::new(9, 18)), "9 wide by 18 tall");
    assert_eq!(
        typed,
        b"ab".to_vec(),
        "the two keys that were not the answer are kept, in the order they were typed"
    );

    let (cell, typed) = cell_reply(b"\x1b[6;18;9");
    assert_eq!(cell, None, "an answer cut in half is no answer");
    assert_eq!(
        typed,
        b"\x1b[6;18;9".to_vec(),
        "and every byte of it is a key"
    );
}

/// One feature answer a terminal gives, in the keys it names and the word it says of them.
fn reply(keys: &str, said: &str) -> String {
    format!("\x1b_G{keys};{said}\x1b\\")
}

#[test]
fn a_terminal_that_answers_both_features_animates_and_one_that_answers_neither_does_not() {
    let both = format!("a{}{}b", reply("e=f,c=1,X=1", "OK"), reply("e=a", "OK"));
    let (animates, typed) = animation_reply(both.as_bytes());
    assert!(
        animates,
        "a terminal that keeps frames and animates them is handed the loop"
    );
    assert_eq!(
        typed,
        b"ab".to_vec(),
        "and the keys that arrived beside the answer are the launch's"
    );

    assert!(
        !animation_reply(reply("e=f,c=1,X=1", "ENOTSUPPORTED").as_bytes()).0,
        "a terminal that answers no such feature is handed the still face"
    );
    assert!(
        !animation_reply(b"").0,
        "a terminal that says nothing is handed the still face too"
    );

    let only_frames = format!(
        "{}{}",
        reply("e=f,c=1,X=1", "OK"),
        reply("e=a", "ENOTSUPPORTED")
    );
    assert!(
        !animation_reply(only_frames.as_bytes()).0,
        "frames without animation are no loop: both features are needed"
    );
    let no_overwrite = format!("{}{}", reply("e=f,c=1", "OK"), reply("e=a", "OK"));
    assert!(
        !animation_reply(no_overwrite.as_bytes()).0,
        "and frames the terminal will not overwrite a rectangle of are no frames the face can \
         be sent, so they are no animation either"
    );

    let animating = Answers::silent()
        .animating()
        .ask(Question::Animation)
        .expect("a terminal that animates answers the question");
    assert!(
        animation_reply(&animating).0,
        "and a scripted terminal that animates answers both features"
    );
    assert!(
        !animation_reply(
            &Answers::silent()
                .ask(Question::Animation)
                .expect("a terminal that does not animate still says so")
        )
        .0,
        "while one that does not says it keeps none of them"
    );
}
