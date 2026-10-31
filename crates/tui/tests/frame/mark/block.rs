//! The mark over the one block whose copy differs from the screen: a picture.
//! A drag that reaches a block takes it whole — the graphic's rows and its
//! caption together — and its words once, and a mark behind an overlay that
//! covers the frame, or a click that opens a row, lights nothing at all.

use super::*;

use crossterm::event::KeyCode;
use ratatui::buffer::Buffer;
use sindri_tui::theme;

/// The pictures a test draws from: committed beside the crate.
const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/pictures");

/// A frame that can draw a picture, in the workspace the fixtures are in: a
/// block is only a block where a graphic is drawn, so the mark over one needs
/// a terminal that speaks Kitty graphics.
fn pictured(text: &str, clipboard: Taking) -> App {
    let mut app = App::with_clipboard(launch(), None, Box::new(clipboard))
        .with_graphics(Some(sindri_tui::graphics::Graphics::kitty()))
        .with_workspace(std::path::Path::new(FIXTURES));
    app.event(Event::TextDelta {
        turn: 1,
        text: text.to_string(),
    });
    app.finished(Finished {
        stopped: Stop::Final,
        turns: 1,
        ..Finished::default()
    });
    app
}

/// The columns a row of the block is washed on, and the ones it is not: a mark
/// that lights the wrong rectangle cannot pass.
fn washed(cells: &Buffer, y: u16, width: u16) -> Vec<u16> {
    (0..width)
        .filter(|x| cells[(*x, y)].bg == theme::MARK)
        .collect()
}

#[test]
fn a_drag_that_starts_inside_a_pictures_block_washes_the_whole_block() {
    let clipboard = Taking::default();
    let mut app = pictured(
        "![a chart](narrow.png)\n\nthe next block\n",
        clipboard.clone(),
    );
    app.note_info("a question");
    let rows = screen(&mut app, 100, 30);
    let caption = row_of(&rows, "a chart ‹narrow.png›") as u16;
    let (top, bottom) = (caption - 3, caption);
    let quiet = buffer(&mut app, 100, 30);

    // The press lands on the second row of the graphic: deep in the block, and
    // in a row that holds no words at all.
    mouse(
        &mut app,
        MouseEventKind::Down(MouseButton::Left),
        6,
        top + 1,
    );
    mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), 9, bottom);
    let cells = frame_cells(&mut app, 100, 30);
    for y in top..=bottom {
        assert!(
            !washed(&cells, y, 100).is_empty(),
            "row {y} of the block — the graphic's and the caption's — is washed: {rows:#?}"
        );
    }
    assert_eq!(
        washed(&cells, top, 100),
        washed(&cells, bottom, 100),
        "the block is one region taken, not a wash on the row the drag touched"
    );
    let begins = (0..100)
        .find(|x| quiet[(*x, bottom)].symbol() != " ")
        .expect("the caption is drawn on its row");
    assert_eq!(
        washed(&cells, top, 100).first().copied(),
        Some(begins),
        "lit from the column its graphic and its caption begin at, and not from the \
         gutter left of them"
    );
    assert!(
        washed(&cells, top - 1, 100).is_empty(),
        "and the row above the block is not part of it"
    );
    assert!(
        washed(&cells, bottom + 1, 100).is_empty(),
        "nor the row below"
    );
    assert_eq!(
        cells[(2, bottom)].symbol(),
        quiet[(2, bottom)].symbol(),
        "the caption is still the caption under the wash"
    );

    mouse(&mut app, MouseEventKind::Up(MouseButton::Left), 9, bottom);
    assert_eq!(
        clipboard.saw(),
        vec!["a chart ‹narrow.png›".to_string()],
        "and the copy is the block's words, once"
    );
}

#[test]
fn a_released_block_mark_stands_and_the_note_counts_the_rows_it_covers() {
    let clipboard = Taking::default();
    let mut app = pictured(
        "![a chart](narrow.png)\n\nthe next block\n",
        clipboard.clone(),
    );
    let rows = screen(&mut app, 100, 30);
    let caption = row_of(&rows, "a chart ‹narrow.png›") as u16;
    let (top, bottom) = (caption - 3, caption);
    mouse(
        &mut app,
        MouseEventKind::Down(MouseButton::Left),
        6,
        top + 1,
    );
    mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), 9, bottom);
    mouse(&mut app, MouseEventKind::Up(MouseButton::Left), 9, bottom);

    // The note the copy leaves lands under the mark and pushes the thread up:
    // the mark goes with the block, which is still lit whole and nowhere else.
    let rows = screen(&mut app, 100, 30);
    let caption = row_of(&rows, "a chart ‹narrow.png›") as u16;
    let cells = frame_cells(&mut app, 100, 30);
    assert_eq!(
        (0..30)
            .filter(|y| !washed(&cells, *y, 100).is_empty())
            .collect::<Vec<_>>(),
        (caption - 3..=caption).collect::<Vec<_>>(),
        "the block is still washed whole after the button comes up, and on the \
         rows it is drawn on: {rows:#?}"
    );
    assert_eq!(
        clipboard.saw(),
        vec!["a chart ‹narrow.png›".to_string()],
        "and the copy is still the block's words, once"
    );
    assert!(
        crate::common::notes(&app).contains("copy sent: 4 rows"),
        "the note counts the rows the mark covers — the graphic's and the \
         caption's — and not the one line a block is taken as: {:?}",
        crate::common::notes(&app)
    );
}

#[test]
fn no_mark_is_painted_behind_an_overlay_that_covers_the_frame() {
    let clipboard = Taking::default();
    let mut app = answered(
        clipboard.clone(),
        "alpha bravo charlie\n\ndelta echo foxtrot",
    );
    let rows = screen(&mut app, 100, 20);
    let first = row_of(&rows, "alpha bravo") as u16;
    let last = row_of(&rows, "delta echo") as u16;
    mouse(&mut app, MouseEventKind::Down(MouseButton::Left), 4, first);
    mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), 7, last);
    assert_ne!(
        washes(&buffer(&mut app, 100, 20)),
        0,
        "the drag lights rows"
    );

    // `?` covers the frame with the bindings: a wash under it is a lit cell
    // nobody can see, so the frame paints none — and an overlay taking the
    // screen is the reader's next gesture, so the mark goes with the frame it
    // was drawn on rather than waiting behind the box.
    press(&mut app, KeyCode::Char('?'), KeyModifiers::NONE);
    assert!(app.overlay().is_some(), "the help overlay is open");
    assert!(
        app.selection().is_empty(),
        "an overlay takes the screen and the mark with it"
    );
    assert_eq!(
        washes(&frame_cells(&mut app, 100, 20)),
        0,
        "nothing is drawn behind an overlay, so nothing is lit behind one"
    );
}

#[test]
fn a_click_that_opens_a_spill_row_starts_no_mark_and_lights_nothing() {
    let spills = tempfile::tempdir().expect("a temp directory");
    let path = spills.path().join("stdout");
    let full: String = (0..500).map(|line| format!("full line {line}\n")).collect();
    std::fs::write(&path, full).expect("write the spill file");

    let clipboard = Taking::default();
    let mut app = App::with_clipboard(launch(), None, Box::new(clipboard.clone()));
    app.event(Event::ToolCall {
        turn: 1,
        command: Some("seq 500".to_string()),
    });
    app.over(
        CallOver::Exited(0),
        Spilled {
            truncated: true,
            files: vec![path],
        },
    );
    let rows = screen(&mut app, 100, 20);
    let row = row_of(&rows, "output too large") as u16;
    mouse(&mut app, MouseEventKind::Down(MouseButton::Left), 4, row);
    assert!(
        app.overlay().is_some(),
        "the click opened the untruncated output"
    );
    assert!(
        app.selection().is_empty(),
        "a click that opens a row is a click and not a drag"
    );
    mouse(&mut app, MouseEventKind::Up(MouseButton::Left), 4, row);
    assert!(
        clipboard.saw().is_empty(),
        "and there was nothing to copy: {:?}",
        clipboard.saw()
    );

    // The same default, on a call's row: it opens the call and starts no mark.
    let mut app = App::with_clipboard(launch(), None, Box::new(clipboard));
    app.event(Event::ToolCall {
        turn: 1,
        command: Some("cargo test".to_string()),
    });
    for line in 1..=20 {
        app.output(Side::Stdout, &format!("line {line}"));
    }
    app.over(CallOver::Exited(0), Spilled::default());
    let rows = screen(&mut app, 100, 20);
    let row = row_of(&rows, "exit 0") as u16;
    mouse(&mut app, MouseEventKind::Down(MouseButton::Left), 4, row);
    assert!(
        app.selection().is_empty(),
        "the row opens, it is not marked"
    );
    assert_eq!(
        washes(&frame_cells(&mut app, 100, 20)),
        0,
        "and the frame is not washed where the click landed"
    );
}
