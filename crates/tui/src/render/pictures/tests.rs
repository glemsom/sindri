//! The frame's pictures: a repaint, a scroll, a resize and `/new` lay a picture
//! out again from the store's cache and never read it from disk a second time
//! (ADR-0016 §5, §6, §9).

use super::*;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use sindri_core::agent::Event;

use crate::app::App;
use crate::graphics::Graphics;
use crate::picture::{Refusal, Store, Worth};
use crate::rail::Launch;

/// The pictures a test draws from: committed beside the crate, so a test reads
/// a file and never builds one.
const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/pictures");

fn launch() -> Launch {
    Launch {
        session: "4f1c8f2e".to_string(),
        temp: "/state/sindri/sessions/4f1c8f2e/tmp".to_string(),
        branch: None,
        sandbox: "bwrap".to_string(),
        workspace: "~/git/sindri".to_string(),
        provider: "anthropic".to_string(),
        model: "claude-sonnet-4-6".to_string(),
        max_turns: 250,
        context_limit: None,
    }
}

/// The frame a test draws, as the loop does: a terminal's size and a buffer, and
/// no terminal at all — the Kitty answer is the made-up one a layout test builds
/// (ADR-0016 §1).
fn frame(app: &mut App, width: u16, height: u16) -> Buffer {
    let area = Rect::new(0, 0, width, height);
    let mut buf = Buffer::empty(area);
    crate::render::transcript::draw(app, area, &mut buf);
    buf
}

/// How many times the store has decoded a source: the read a repaint must not
/// pay for.
fn decodes(app: &App) -> u64 {
    app.pictures().map(Store::decodes).unwrap_or_default()
}

/// Whether the buffer carries a graphic on any row of it.
fn drawn(buf: &Buffer) -> bool {
    buf.content
        .iter()
        .any(|cell| cell.symbol().contains('\u{1b}'))
}

fn press(app: &mut App, code: KeyCode) {
    app.key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn answer(app: &mut App) {
    app.event(Event::TextDelta {
        turn: 1,
        text: "![a chart](square.png)\n".to_string(),
    });
}

/// A frame whose thread is the picture's answer and twenty notes under it: the
/// picture's turn is the first, and the tail is a page away from it.
fn pictured(scratch: &std::path::Path) -> App {
    let mut app = App::new(launch(), None)
        .with_graphics(Some(Graphics::kitty()))
        .with_workspace(scratch);
    answer(&mut app);
    for note in 1..=20 {
        app.note_info(format!("a note {note}"));
    }
    app
}

#[test]
fn a_scroll_and_a_resize_lay_the_picture_out_again_without_reading_it_again() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let plot = scratch.path().join("square.png");
    std::fs::copy(format!("{FIXTURES}/square.png"), &plot).expect("a picture in the workspace");
    let mut app = pictured(scratch.path());

    // The tail first, where the picture is scrolled out: nothing to decode.
    let tail = frame(&mut app, 100, 30);
    assert!(!drawn(&tail), "the tail shows the notes, not the picture");
    assert_eq!(decodes(&app), 0, "nothing was drawn, nothing was read");

    // The first turn: the picture, decoded once.
    press(&mut app, KeyCode::Home);
    let top = frame(&mut app, 100, 30);
    assert!(drawn(&top), "the picture is drawn on its rows");
    assert_eq!(decodes(&app), 1, "the first draw decodes it once");

    // Scrolled out and back: laid out again, from the pixels already held.
    press(&mut app, KeyCode::PageDown);
    let away = frame(&mut app, 100, 30);
    assert!(!drawn(&away), "scrolled out, it is drawn nowhere");
    assert_eq!(decodes(&app), 1, "and the scroll decoded nothing");
    press(&mut app, KeyCode::Home);
    let back = frame(&mut app, 100, 30);
    assert!(drawn(&back), "scrolled back in, it is drawn again");
    assert_eq!(decodes(&app), 1, "from the protocol the store still holds");

    // A resize: the column is wider and the picture keeps its own width, so the
    // block is the same one — laid out again, and not read again.
    let wide = frame(&mut app, 120, 30);
    assert!(drawn(&wide), "the picture survives the resize");
    assert_eq!(decodes(&app), 1, "a resize is no reason to read the file");
}

#[test]
fn new_lays_the_picture_out_again_without_reading_it_again() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    std::fs::copy(
        format!("{FIXTURES}/square.png"),
        scratch.path().join("square.png"),
    )
    .expect("a picture in the workspace");
    let mut app = pictured(scratch.path());
    press(&mut app, KeyCode::Home);
    assert!(drawn(&frame(&mut app, 100, 30)), "the picture is drawn");
    assert_eq!(decodes(&app), 1);

    // `/new`: the thread goes, the store's pixels do not, and the same picture
    // in a new thread is drawn from what is held.
    app.renewed(Ok(launch()));
    assert!(
        !app.entries()
            .iter()
            .any(|entry| matches!(entry, crate::entry::Entry::Answer { .. })),
        "the thread the picture was in is gone"
    );
    answer(&mut app);
    let fresh = frame(&mut app, 100, 30);
    assert!(drawn(&fresh), "the picture of the new thread is drawn");
    assert_eq!(decodes(&app), 1, "and costs no second read");

    // The file is taken away: the next frame asks the store again — a header
    // read is what a frame may afford — and it says why there is nothing to
    // draw, without building a protocol a second time.
    std::fs::remove_file(scratch.path().join("square.png")).expect("the picture is gone");
    let gone = frame(&mut app, 100, 30);
    assert!(
        !drawn(&gone),
        "a source gone since it was measured draws nothing"
    );
    assert_eq!(
        app.viewport().pictures[0].worth,
        Worth::Refused(Refusal::Missing),
        "and its caption carries the reason"
    );
    assert_eq!(decodes(&app), 1, "and nothing was decoded again");
}
