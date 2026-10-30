//! A picture in the frame: the block of rows it takes in the transcript, the
//! caption under it, and the row count the frame keeps rather than measuring
//! again (ADR-0016 §2, §5, §9).

use super::*;

use std::path::Path;

use sindri_tui::graphics::Graphics;
use sindri_tui::theme;

/// The pictures a test draws from: committed beside the crate.
const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/pictures");

/// A frame that can draw a picture, in a workspace the test names.
fn pictured(workspace: &Path) -> App {
    App::new(launch(), None)
        .with_graphics(Some(Graphics::kitty()))
        .with_workspace(workspace)
}

/// The frame with one answer in it, and where that answer is.
fn answered(app: &mut App, text: &str) -> usize {
    app.event(Event::TextDelta {
        turn: 1,
        text: text.to_string(),
    });
    app.entries().len() - 1
}

/// The transcript column's own characters, above the composer: the rail keeps
/// its 32 columns and a column of canvas separates them.
const COLUMN: usize = 100 - RAIL;

fn column(rows: &[String]) -> Vec<String> {
    rows.iter()
        .take_while(|row| !row.starts_with('╭'))
        .map(|row| row[..COLUMN.min(row.len())].trim_end().to_string())
        .collect()
}

#[test]
fn a_picture_that_opens_an_answer_is_blank_rows_and_then_its_caption() {
    let mut app = pictured(Path::new(FIXTURES));
    answered(&mut app, "![a chart](narrow.png)\n\nthe next block\n");
    let shown = column(&screen(&mut app, 100, 30));

    let caption = shown
        .iter()
        .position(|row| row.contains("a chart ‹narrow.png›"))
        .unwrap_or_else(|| panic!("the caption is in the transcript: {shown:#?}"));
    assert!(
        shown[caption - 3..caption].iter().all(|row| row.is_empty()),
        "the three rows the store measured are blank above it: {shown:#?}"
    );
    assert!(
        !shown[caption - 4].is_empty(),
        "and the block starts where they do, under the answer's marker: {shown:#?}"
    );
    assert_eq!(
        shown[caption + 1],
        "",
        "the next block starts one row after the caption: {shown:#?}"
    );
    assert_eq!(shown[caption + 2], "  the next block", "{shown:#?}");
}

#[test]
fn a_picture_the_terminal_cannot_draw_is_its_caption_and_the_reason() {
    // No graphics answer, so no store: the frame says what it could not do and
    // draws no placeholder box (ADR-0016 §1, §8).
    let mut app = app();
    answered(&mut app, "![a chart](narrow.png)\n\nthe next block\n");
    let shown = column(&screen(&mut app, 100, 30));
    let caption = shown
        .iter()
        .position(|row| row.contains("a chart ‹narrow.png"))
        .unwrap_or_else(|| panic!("the caption is said: {shown:#?}"));

    assert!(
        shown[caption].contains("this terminal draws no picture"),
        "the reason is in the ‹› where the drawing would be: {shown:#?}"
    );
    assert!(
        !shown[caption - 1].is_empty(),
        "it took no rows at all, so the caption sits under the answer's marker: {shown:#?}"
    );
    assert_eq!(
        shown[caption + 1],
        "",
        "and the next block starts one row after it: {shown:#?}"
    );
    assert_eq!(shown[caption + 2], "  the next block", "{shown:#?}");
    let buf = row_styles(&mut app, 100, 30, caption, "a chart");
    assert_eq!(buf, theme::FAINT, "the caption is dimmed");
}

/// The colour of the first cell of `needle` on the drawn row that says it.
fn row_styles(
    app: &mut App,
    width: u16,
    height: u16,
    row: usize,
    needle: &str,
) -> ratatui::style::Color {
    let shown = screen(app, width, height);
    let column = shown[row]
        .find(needle)
        .unwrap_or_else(|| panic!("row {row} says {needle:?}: {shown:#?}"));
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("a test terminal");
    terminal
        .draw(|frame| render::draw(app, frame))
        .expect("the frame draws");
    terminal.backend().buffer()[(column as u16, row as u16)].fg
}

#[test]
fn the_same_width_is_measured_once_and_a_resize_asks_the_store_again() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = scratch.path().join("plot.png");
    std::fs::copy(format!("{FIXTURES}/wide.png"), &path).expect("a picture in the workspace");

    let mut app = pictured(scratch.path());
    let answer = answered(&mut app, "![a chart](plot.png)\n");

    assert_eq!(
        app.measure(answer, 30),
        7,
        "a width never measured is the store's answer: five rows, the marker and the caption"
    );
    assert_eq!(
        app.measure(answer, 30),
        7,
        "and the same width is that answer, already held"
    );
    std::fs::remove_file(&path).expect("the picture is gone");
    assert_eq!(
        app.measure(answer, 30),
        7,
        "the row count held costs no file read, even with the file gone"
    );
    assert_eq!(
        app.measure(answer, 60),
        2,
        "a resize is measured again from the store, which now says the file is gone"
    );
}
