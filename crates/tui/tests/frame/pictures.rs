//! A picture in the frame: the block of rows it takes in the transcript, the
//! caption under it, and the row count the frame keeps rather than measuring
//! again.

use super::*;

use std::path::Path;

use crossterm::event::{KeyCode, KeyModifiers, MouseEvent, MouseEventKind};
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
/// its 32 columns and a column of canvas separates them. That column is the
/// margin, and a terminal that speaks graphics draws the seam's light strip
/// there, so the words are the columns before it.
const COLUMN: usize = MARGIN;

fn column(rows: &[String]) -> Vec<String> {
    rows.iter()
        .take_while(|row| !row.starts_with('╭') && !row.starts_with(DRAWN))
        .map(|row| {
            row.chars()
                .take(COLUMN)
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect()
}

/// The mark the test backend reads a drawn graphic as (tests/frame/main.rs).
const DRAWN: &str = "\u{25A9}";

fn marker_row() -> String {
    format!("  {DRAWN}")
}

#[test]
fn a_picture_is_drawn_where_its_rows_are_and_the_caption_under_its_last() {
    let mut app = pictured(Path::new(FIXTURES));
    answered(&mut app, "![a chart](narrow.png)\n\nthe next block\n");
    let shown = column(&screen(&mut app, 100, 30));
    let caption = shown
        .iter()
        .position(|row| row.contains("a chart ‹narrow.png›"))
        .unwrap_or_else(|| panic!("the caption is in the transcript: {shown:#?}"));

    let placed = app.viewport().pictures.clone();
    assert_eq!(placed.len(), 1, "one picture on screen, one placement");
    let area = placed[0].area;
    // 100 pixels across is thirteen columns of eight and three rows of
    // eighteen: the block the store measured, at the answer's own edge.
    assert_eq!(
        (area.x, area.y, area.width, area.height),
        (2, caption as u16 - 3, 13, 3),
        "the cells the graphic was given are the rows the markdown reserved: {shown:#?}"
    );
    assert_eq!(
        usize::from(app.viewport().area.y) + placed[0].at + placed[0].rows,
        caption + 1,
        "the block is the graphic's rows and its caption's, which is the row \
         under the graphic's last: {shown:#?}"
    );
    assert_eq!(
        shown[caption - 3..caption].to_vec(),
        vec![marker_row(); 3],
        "the graphic is drawn on every row of its block, and only there: {shown:#?}"
    );
    assert_eq!(
        shown[caption - 4],
        "  ⠋ 0.0s",
        "the block starts right under the answer's marker: {shown:#?}"
    );
}

/// The drawn rows that carry a graphic: the position marker in the column's last
/// cell is not one of them.
fn drawn(rows: &[String]) -> usize {
    rows.iter().filter(|row| row.contains(DRAWN)).count()
}

/// The keys a reader scrolls with: the wheel is three rows a notch and the page
/// keys a screenful.
fn wheel(app: &mut App, down: bool) {
    let kind = if down {
        MouseEventKind::ScrollDown
    } else {
        MouseEventKind::ScrollUp
    };
    app.mouse(MouseEvent {
        kind,
        column: 1,
        row: 1,
        modifiers: KeyModifiers::NONE,
    });
}

#[test]
fn a_picture_inside_a_quote_is_drawn_where_its_own_edge_is() {
    let mut app = pictured(Path::new(FIXTURES));
    answered(&mut app, "> ![a chart](narrow.png)\n");
    let shown = column(&screen(&mut app, 100, 30));
    let caption = shown
        .iter()
        .position(|row| row.contains("│ a chart ‹narrow.png›"))
        .unwrap_or_else(|| panic!("the quoted caption is in the transcript: {shown:#?}"));

    let area = app.viewport().pictures[0].area;
    assert_eq!(
        area.x, 4,
        "the graphic hangs off the quote's edge, under its caption, and not two \
         columns to its left: {shown:#?}"
    );
    assert_eq!(
        area.y,
        caption as u16 - 3,
        "and the rows it was measured are the block's: {shown:#?}"
    );
}

#[test]
fn an_open_overlay_stops_the_picture_and_closing_it_draws_it_again() {
    let mut app = pictured(Path::new(FIXTURES));
    answered(&mut app, "![a chart](narrow.png)\n");
    let shown = column(&screen(&mut app, 100, 30));
    assert!(
        drawn(&shown) > 0,
        "the picture is drawn while the frame is bare: {shown:#?}"
    );

    // An overlay dims cells, and a graphic is not a cell: nothing may be drawn
    // behind it.
    press(&mut app, KeyCode::Char('?'));
    assert!(app.overlay().is_some(), "? opens the help overlay");
    let shown = column(&screen(&mut app, 100, 30));
    assert_eq!(
        drawn(&shown),
        0,
        "no picture is drawn behind an open overlay: {shown:#?}"
    );
    assert_eq!(
        app.viewport().pictures.len(),
        1,
        "the block is still placed where it is; only the painting stopped: {shown:#?}"
    );

    press(&mut app, KeyCode::Esc);
    assert!(app.overlay().is_none(), "Esc closes it");
    let shown = column(&screen(&mut app, 100, 30));
    assert!(
        drawn(&shown) > 0,
        "and the picture is drawn again once it is closed: {shown:#?}"
    );
}

#[test]
fn a_picture_partly_scrolled_out_is_drawn_nowhere_and_whole_once_it_is_in() {
    let mut app = pictured(Path::new(FIXTURES));
    answered(&mut app, "![a chart](square.png)\n\n");
    for note in 1..=20 {
        app.note_info(format!("a note {note}"));
    }
    // The first turn, where the picture is: twelve rows of it and its caption,
    // whole in a window of a screenful.
    press(&mut app, KeyCode::Home);
    let shown = column(&screen(&mut app, 100, 30));
    let whole = app.viewport().pictures.clone();
    assert_eq!(whole.len(), 1, "the whole picture is placed: {shown:#?}");
    assert_eq!(
        whole[0].area.height, 12,
        "with the rows the store measured, not a cropped part of them: {shown:#?}"
    );
    assert!(drawn(&shown) > 0, "and it is drawn on them: {shown:#?}");

    // Three rows into its block: part of it is above the window, so no row of
    // it is drawn at an edge.
    wheel(&mut app, true);
    let shown = column(&screen(&mut app, 100, 30));
    assert_eq!(
        drawn(&shown),
        0,
        "a block partly scrolled out is drawn nowhere: {shown:#?}"
    );
    assert!(
        app.viewport().pictures.iter().all(|picture| !picture.drawn),
        "and none of it is drawn: {shown:#?}"
    );
    assert!(
        shown.iter().any(|row| row.contains("a chart ‹square.png›")),
        "its caption still says where it is: {shown:#?}"
    );
    let plain = &app.viewport().rows;
    assert_eq!(
        plain[0], "a chart ‹square.png›",
        "and a copy of the window is its caption once, on the block's first \
         row: {plain:?}"
    );
    assert!(
        plain[1..=10].iter().all(String::is_empty),
        "with no filler on the rest of its rows: {plain:?}"
    );

    // And back: whole again, not cropped into place.
    wheel(&mut app, false);
    let shown = column(&screen(&mut app, 100, 30));
    assert_eq!(
        app.viewport().pictures.len(),
        1,
        "scrolled back in, the picture is placed whole: {shown:#?}"
    );
    assert!(
        drawn(&shown) > 0,
        "and drawn on the rows it was placed in: {shown:#?}"
    );
}

#[test]
fn a_picture_that_opens_an_answer_takes_its_rows_and_then_its_caption() {
    let mut app = pictured(Path::new(FIXTURES));
    answered(&mut app, "![a chart](narrow.png)\n\nthe next block\n");
    let shown = column(&screen(&mut app, 100, 30));

    let caption = shown
        .iter()
        .position(|row| row.contains("a chart ‹narrow.png›"))
        .unwrap_or_else(|| panic!("the caption is in the transcript: {shown:#?}"));
    assert!(
        shown[caption - 3..caption]
            .iter()
            .all(|row| row.trim_end().ends_with(DRAWN)),
        "the three rows the store measured carry the picture: {shown:#?}"
    );
    assert_eq!(
        shown[caption - 4],
        "  \u{280B} 0.0s",
        "and the block starts under the answer's marker: {shown:#?}"
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
    // draws no placeholder box.
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
