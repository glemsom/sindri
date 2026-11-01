//! The frame's pictures: a repaint, a scroll, a resize and `/new` lay a picture
//! out again from the store's cache and never read it from disk a second time.

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
/// no terminal at all — the Kitty answer is the made-up one a layout test builds.
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

/// How many fence bodies the store has put through the renderer: the five
/// milliseconds a frame full of diagrams must not pay again.
fn renders(app: &App) -> u64 {
    app.pictures().map(Store::renders).unwrap_or_default()
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

/// A diagram a fence in an answer draws: flat and wide, so it is drawn at its
/// own size inside the window.
const DIAGRAM: &str = "stateDiagram-v2\n    [*] --> Resting\n    Resting --> [*]\n";

/// A frame whose thread is a mermaid fence and twenty notes under it: the
/// diagram's turn is the first, and the tail is a page away from it.
fn diagrammed() -> App {
    let mut app = App::new(launch(), None).with_graphics(Some(Graphics::kitty()));
    app.event(Event::TextDelta {
        turn: 1,
        text: format!("```mermaid\n{DIAGRAM}```\n"),
    });
    for note in 1..=40 {
        app.note_info(format!("a note {note}"));
    }
    app
}

#[test]
fn a_repaint_of_a_mermaid_fence_paints_pixels_and_renders_it_once() {
    let mut app = diagrammed();

    // The tail first, where the diagram is scrolled out: nothing to render.
    let tail = frame(&mut app, 100, 30);
    assert!(!drawn(&tail), "the tail shows the notes, not the diagram");
    assert_eq!(decodes(&app), 0, "nothing was drawn, nothing was rendered");

    // The first turn: the diagram, rendered once. The svg behind it is rendered
    // once too — a measure is the same drawing, and measuring one is what a
    // frame does to every entry it lays out.
    press(&mut app, KeyCode::Home);
    let top = frame(&mut app, 100, 30);
    assert!(drawn(&top), "the diagram is drawn on the rows it reserved");
    assert_eq!(decodes(&app), 1, "the first draw renders it once");
    assert_eq!(renders(&app), 1, "and the svg behind it once as well");

    // A repaint of a live run: the same rows, the pixels the store holds.
    let again = frame(&mut app, 100, 30);
    assert!(drawn(&again), "a repaint still paints the diagram");
    assert_eq!(decodes(&app), 1, "and costs no second render");

    // Scrolled out and back: drawn nowhere, then drawn again from what is held.
    press(&mut app, KeyCode::End);
    assert!(!drawn(&frame(&mut app, 100, 30)), "scrolled out, nowhere");
    assert_eq!(decodes(&app), 1, "and the scroll rendered nothing");
    press(&mut app, KeyCode::Home);
    assert!(
        drawn(&frame(&mut app, 100, 30)),
        "scrolled back, drawn again"
    );
    assert_eq!(decodes(&app), 1, "from the pixels the store still holds");
    assert_eq!(renders(&app), 1, "and the svg the store still holds");
}

#[test]
fn a_delta_below_a_closed_diagram_recosts_neither_the_diagram_nor_its_pixels() {
    let mut app = App::new(launch(), None).with_graphics(Some(Graphics::kitty()));
    app.event(Event::TextDelta {
        turn: 1,
        text: format!("```mermaid\n{DIAGRAM}```\n\nA paragraph that"),
    });
    press(&mut app, KeyCode::Home);
    assert!(drawn(&frame(&mut app, 100, 30)), "the diagram is drawn");
    assert_eq!(renders(&app), 1, "and rendered once");

    // The delta lands in the paragraph under the diagram, which closed above it.
    app.event(Event::TextDelta {
        turn: 1,
        text: " goes on, and on, as the stream says more of it.".to_string(),
    });
    let view = crate::render::transcript::window(&mut app, 100, 30);
    assert!(drawn(&frame(&mut app, 100, 30)), "the diagram still drawn");
    assert_eq!(
        renders(&app),
        1,
        "a fence closed above the delta is neither laid out nor rendered again"
    );
    assert_eq!(decodes(&app), 1, "and its pixels are the ones held");
    assert!(
        view.recounted <= 8,
        "the delta recounts the rows it landed in, not the diagram above it: {}",
        view.recounted
    );
}

/// A diagram drawn top to bottom and long with it: ten nodes, one under the
/// next, whose height is the dimension that carries its words. Committed beside
/// the crate so the store, the writer and the frame all measure one diagram.
const PORTRAIT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/fixtures/diagrams/ten-nodes.mmd"
));

/// The words of every row of a drawn frame, so a test can read what the reader
/// reads.
fn words(buf: &Buffer) -> Vec<String> {
    (buf.area.top()..buf.area.bottom())
        .map(|y| {
            (buf.area.left()..buf.area.right())
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect()
}

/// A thread that is one mermaid fence and nothing else: the diagram's block is
/// the whole answer, so the window is filled with it and its caption.
fn portrait() -> App {
    let mut app = App::new(launch(), None).with_graphics(Some(Graphics::kitty()));
    app.event(Event::TextDelta {
        turn: 1,
        text: format!("```mermaid\n{PORTRAIT}```\n"),
    });
    app
}

#[test]
fn a_diagram_taller_than_the_window_says_so_instead_of_a_blank_block() {
    let mut app = portrait();
    let short = frame(&mut app, 100, 30);
    let shown = words(&short);

    assert!(
        !drawn(&short),
        "the block is forty-one rows and the window thirty: no graphic is whole: {shown:?}"
    );
    assert_eq!(decodes(&app), 0, "and no pixel was spent to learn that");
    assert!(
        shown.iter().any(|row| row.contains("over the window's 30")),
        "the block says why it is drawn nowhere rather than sitting blank: {shown:?}"
    );
    assert!(
        shown.iter().any(|row| row.trim() == "‹mermaid›"),
        "and the caption under it still names the source: {shown:?}"
    );
}

#[test]
fn a_diagram_whole_in_a_tall_window_is_drawn_and_says_nothing_about_it() {
    let mut app = portrait();
    let tall = frame(&mut app, 100, 60);
    let shown = words(&tall);

    assert!(
        drawn(&tall),
        "the block is whole here, so it is drawn: {shown:?}"
    );
    assert_eq!(
        decodes(&app),
        1,
        "once, from the diagram the store rendered"
    );
    assert!(
        !shown.iter().any(|row| row.contains("over the window's 30")),
        "and a drawn diagram carries no apology: {shown:?}"
    );
}

#[test]
fn a_portrait_diagram_is_drawn_whole_in_a_forty_column_window_of_eighty_rows() {
    // The acceptance of #120: a 40-column body on an 80-row terminal. The
    // diagram takes the rows its own aspect asks of that column — the width,
    // not the allowance, is what it is capped by here — and all of them are
    // inside the window, so the reader is looking at the diagram.
    let mut app = portrait();
    let acceptance = frame(&mut app, 40, 80);
    let worth = app.viewport().pictures[0].worth.clone();
    let shown = words(&acceptance);

    assert_eq!(
        worth.rows(),
        29,
        "the body is thirty-eight columns inside the gutter, so the column — not \
         the allowance — drew it: {worth:?}"
    );
    assert!(
        worth.rows() > crate::picture::MAX_ROWS,
        "and it is still more than a picture's twenty: {worth:?}"
    );
    assert!(
        drawn(&acceptance),
        "and the whole of it is drawn: {shown:?}"
    );
    assert_eq!(decodes(&app), 1, "from one render, once");
    assert!(
        !shown.iter().any(|row| row.contains("over the window's 30")),
        "with nothing to apologise for: {shown:?}"
    );
}
