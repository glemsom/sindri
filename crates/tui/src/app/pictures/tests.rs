//! A diagram is laid out off the frame's thread: a fence that has just closed
//! stands as its panel for a tick and is its diagram when the answer comes back,
//! and no frame lays one out. A frame's own cost is the store's count of the
//! bodies it has put through the renderer, and that count is what these tests
//! read.

#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken contract reports itself"
)]

use sindri_core::agent::Event;

use crate::app::App;
use crate::graphics::Graphics;
use crate::picture::{self, Store};
use crate::rail::Launch;
use crate::render::transcript;

/// A diagram the crate draws: flat and wide, so it is drawn at its own size
/// inside the window.
const DIAGRAM: &str = "stateDiagram-v2\n    [*] --> Resting\n    Resting --> [*]\n";

/// A diagram no parser can read: an edge with nothing on the far side.
const BROKEN: &str = "flowchart TD\n    A --> \n";

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

/// A frame that can draw a picture: the made-up Kitty answer a layout test
/// builds, since no test has a terminal.
fn pictured() -> App {
    App::new(launch(), None).with_graphics(Some(Graphics::kitty()))
}

/// A frame whose thread is one mermaid fence the stream has closed: the fence is
/// the whole of what the answer says.
fn diagrammed(body: &str) -> App {
    let mut app = pictured();
    app.event(Event::TextDelta {
        turn: 1,
        text: format!("```mermaid\n{body}```\n"),
    });
    app
}

/// How many fence bodies the store has put through the renderer: the layout a
/// frame must not make, and the figure these tests read to say it made none.
fn renders(app: &App) -> u64 {
    app.pictures().map(Store::renders).unwrap_or_default()
}

/// The drawings the frames so far have asked for, made here as the loop makes
/// them — on a thread of its own there, on this one here — and handed back to
/// the store. A test has no loop, so it is the loop.
fn drained(app: &mut App) -> usize {
    let asked = app.take_drawings();
    for body in &asked {
        let drawing = picture::drawn(body);
        app.drawn(body.clone(), drawing);
    }
    asked.len()
}

/// The words of the rows the window built: what the writer drew, before a
/// block's copy puts its source over the row it starts at.
fn written(view: &transcript::View) -> Vec<String> {
    view.rows.iter().map(transcript::plain).collect()
}

/// The row of `rows` that says `needle`, where one does.
fn row_of(rows: &[String], needle: &str) -> Option<usize> {
    rows.iter().position(|row| row.contains(needle))
}

#[test]
fn no_frame_lays_a_diagram_out() {
    let mut app = diagrammed(DIAGRAM);
    let first = transcript::window(&mut app, 100, 30);
    assert_eq!(
        renders(&app),
        0,
        "the frame that lays a fence out lays no diagram out"
    );
    assert_eq!(first.remeasured, 1, "the frame measured the entry it drew");
    assert!(
        row_of(&written(&first), "Resting").is_some(),
        "and what it measured is the panel: the fence's own source {:?}",
        first.plain
    );

    // A repaint asks for nothing either: a frame with a diagram in the thread
    // costs what a frame with no picture in it costs.
    let again = transcript::window(&mut app, 100, 30);
    assert_eq!(renders(&app), 0, "a repaint lays nothing out");
    assert_eq!(again.remeasured, 0, "and measures no entry a second time");
}

/// A fence that closes mid-stream is its diagram within a tick, with nothing
/// typed and no resize: the frame stands the panel up and asks, the answer
/// arrives, and the diagram's rows replace the panel's where they were.
#[test]
fn a_fence_that_closed_mid_stream_becomes_its_diagram_with_nothing_typed() {
    let mut app = pictured();
    app.event(Event::TextDelta {
        turn: 1,
        text: format!("```mermaid\n{DIAGRAM}"),
    });
    let arriving = transcript::window(&mut app, 100, 30);
    assert!(
        arriving.pictures.is_empty(),
        "a fence still arriving is no diagram: {:?}",
        arriving.plain
    );
    assert_eq!(renders(&app), 0, "and is asked for no diagram");

    // The closing delimiter lands, and the next frame is the panel it was — and
    // the frame makes no layout of its own.
    app.event(Event::TextDelta {
        turn: 1,
        text: "```\n".to_string(),
    });
    let closed = transcript::window(&mut app, 100, 30);
    assert_eq!(
        renders(&app),
        0,
        "the frame that closes the fence lays nothing out"
    );
    assert!(closed.pictures.is_empty(), "the panel stands in for a tick");
    let panel = written(&closed);
    assert!(
        panel.get(1).is_some_and(|row| row.contains("╭─ mermaid")),
        "the panel titled with the fence opens under the answer's marker: {panel:?}"
    );
    assert!(
        row_of(&panel, "Resting").is_some(),
        "and the fence's own source is in it: {panel:?}"
    );

    // The drawing is made, as the loop makes it, and the answer is kept.
    assert_eq!(drained(&mut app), 1, "the fence asked for one drawing");
    let diagram = transcript::window(&mut app, 100, 30);
    assert_eq!(renders(&app), 1, "and it was made once");
    assert_eq!(
        diagram.pictures.len(),
        1,
        "the answer turned the panel into a block: {:?}",
        diagram.plain
    );
    assert_eq!(
        diagram.pictures[0].laid.row, 1,
        "and the diagram's rows stand in the panel's place, under the marker: {:?}",
        diagram.plain
    );
    assert!(
        row_of(&written(&diagram), "‹mermaid›").is_some(),
        "the caption under it still names the source: {:?}",
        diagram.plain
    );
    assert!(
        row_of(&written(&diagram), "Resting").is_none(),
        "and the panel's source has gone: {:?}",
        diagram.plain
    );
}

/// A diagram that will not draw is its panel with the parser's reason under it,
/// in the same words it was ever said in — and between the ask and the answer it
/// is the panel alone, because "not drawn yet" is silence and never a refusal.
#[test]
fn a_diagram_that_will_not_draw_is_its_panel_and_then_the_parser_s_reason() {
    let mut app = diagrammed(BROKEN);
    let asked = transcript::window(&mut app, 100, 30);
    assert_eq!(renders(&app), 0, "the frame lays nothing out");
    assert!(
        row_of(&written(&asked), "flowchart TD").is_some(),
        "the source is on screen: {:?}",
        asked.plain
    );
    assert!(
        row_of(&written(&asked), "expected").is_none(),
        "and nothing is refused before the parser has spoken: {:?}",
        asked.plain
    );

    assert_eq!(drained(&mut app), 1, "the fence asked for one drawing");
    let refused = transcript::window(&mut app, 100, 30);
    assert!(
        row_of(&written(&refused), "flowchart TD").is_some(),
        "no cell of the source is swallowed: {:?}",
        refused.plain
    );
    assert!(
        row_of(&written(&refused), "expected").is_some(),
        "and the parser's own words are under it: {:?}",
        refused.plain
    );
    assert!(
        refused.pictures.is_empty(),
        "nothing is measured, so there is nothing to draw"
    );
}

/// Leaving, cancelling or `/new` with a drawing in flight does not wait for it:
/// the answer is dropped rather than joined, and a thread that has never heard
/// of the fence it was asked for is no thread's business.
#[test]
fn a_drawing_in_flight_when_the_thread_goes_is_dropped() {
    let mut app = diagrammed(DIAGRAM);
    transcript::window(&mut app, 100, 30);
    assert_eq!(renders(&app), 0, "the frame asked and waited for nothing");

    app.renewed(Ok(launch()));
    assert_eq!(drained(&mut app), 1, "the drawing is still made");
    let fresh = transcript::window(&mut app, 100, 30);
    assert!(
        fresh.pictures.is_empty(),
        "and nothing of the thread it was asked for is drawn: {:?}",
        fresh.plain
    );
}
