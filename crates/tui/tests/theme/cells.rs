//! The law for the frame's cells, as `pixels` is the law for its pictures: every
//! colour the frame paints is one the theme names, every cell sits on a surface
//! the theme paints, and no cell leaves its ink to the terminal. Between them
//! they are one claim — every byte of the frame is a colour the theme chose —
//! which is why they share one frame with one of everything in it.

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use sindri_core::agent::Event;
use sindri_tui::driver::{CallOver, Finished, Side, Spilled, Stop};
use sindri_tui::graphics::Graphics;
use sindri_tui::render;
use sindri_tui::theme;
use sindri_tui::{App, Face, Launch};

use crate::common;

fn launch() -> Launch {
    Launch {
        branch: Some("t20-the-surface".to_string()),
        ..common::launch()
    }
}

/// The surfaces a cell of a painted frame may sit on: the three tiers, and a
/// diff row's wash — a tint over the panel rather than a fourth tier.
const SURFACES: [Color; 5] = [
    theme::CANVAS,
    theme::PANEL,
    theme::RAISED,
    theme::DIFF_ADDED_BG,
    theme::DIFF_REMOVED_BG,
];

fn app() -> App {
    App::new(launch(), None)
}

/// The same busiest frame with both pictures on it: a terminal that speaks Kitty
/// graphics, so the strip is lit in the margin and the face is placed in the
/// rail's top.
fn pictured(width: u16, height: u16) -> Buffer {
    let mut app = App::new(launch(), Face::kitty()).with_graphics(Some(Graphics::kitty()));
    busy(&mut app, width, height)
}

/// A frame with one of everything that paints, `app`'s to fill: prose and inline
/// code, a highlighted fence and a diff fence, a call's two sides, an error
/// note, the rail, the composer, and a modal on the raised tier above them.
fn busy(app: &mut App, width: u16, height: u16) -> Buffer {
    app.event(Event::TextDelta {
        turn: 1,
        text: "A `code` word and two fences:\n\n```rust\n// a note\nfn main() -> u32 {\n    let answer: u32 = 42;\n    println!(\"answer {}\");\n    answer\n}\n```\n\n```diff\n-old\n+new\n```\n"
            .to_string(),
    });
    app.event(Event::ToolCall {
        turn: 1,
        command: Some("make".to_string()),
    });
    app.output(Side::Stdout, "compiling");
    app.output(Side::Stderr, "warning: unused import");
    app.over(CallOver::Exited(1), Spilled::default());
    app.finished(Finished {
        stopped: Stop::Error,
        turns: 1,
        error: Some("provider error (HTTP 529): overloaded".to_string()),
        ..Finished::default()
    });
    app.key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('?'),
        crossterm::event::KeyModifiers::NONE,
    ));
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("a test terminal");
    let mut drawn = Buffer::empty(Rect::new(0, 0, width, height));
    terminal
        .draw(|frame| {
            render::draw(app, frame);
            // The frame's own cells: a backend's buffer is the diff from the
            // frame before it, and a picture's escape is wider than the cell it
            // fills, so the diff cannot answer for the cells beside one.
            drawn = frame.buffer_mut().clone();
        })
        .expect("the frame draws");
    drawn
}

#[test]
fn every_colour_the_frame_paints_comes_from_the_theme() {
    let buf = busy(&mut app(), 100, 70);
    // The theme answers for both kinds of ink it allows: a colour `PALETTE`
    // names, and a step of a walk between two of them (`theme::RAMPS`).
    let named = theme::owns;
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            let cell = &buf[(x, y)];
            assert!(
                named(cell.fg),
                "({x},{y}) {:?} is painted in a colour the theme does not name: {:?}",
                cell.symbol(),
                cell.fg
            );
            assert!(
                named(cell.bg),
                "({x},{y}) {:?} sits on a surface the theme does not name: {:?}",
                cell.symbol(),
                cell.bg
            );
        }
    }
    let surfaces: Vec<Color> = [theme::CANVAS, theme::PANEL, theme::RAISED]
        .into_iter()
        .filter(|surface| {
            (0..buf.area.height).any(|y| (0..buf.area.width).any(|x| buf[(x, y)].bg == *surface))
        })
        .collect();
    assert_eq!(
        surfaces.len(),
        3,
        "the frame paints all three tiers: the canvas, a panel and raised"
    );
}

/// The canvas is the theme's, not the terminal's: every cell of a frame sits on
/// a surface the theme paints, so a light terminal or one with a background
/// image never shows through a hole a renderer left. A narrow frame and a wide
/// one alike leave no unpainted row — the fill happens before the layout, so a
/// region the frame does not draw into is still the theme's.
///
/// A diff row's wash is the one thing beside the three tiers, and it is not a
/// fourth: it is a tint over the panel, which is why it is named here as a tint
/// and not as a surface. `GLOSSARY.md` holds the fence's gutter and its rows to
/// that wash, and `tests/markdown/prose.rs` holds the spans to it.
#[test]
fn every_cell_of_a_painted_frame_sits_on_a_surface_the_theme_paints() {
    for (width, height) in [(40, 10), (200, 60)] {
        let buf = busy(&mut app(), width, height);
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                let bg = buf[(x, y)].bg;
                assert!(
                    SURFACES.contains(&bg),
                    "({x},{y}) at {width}x{height} is {:?} on {bg:?} — the terminal's \
                     own background, or a surface the theme does not paint",
                    buf[(x, y)].symbol()
                );
            }
        }
    }
}

/// The ink is the theme's too, as the surface above is: a cell that names no
/// foreground is drawn in the terminal's default, and a reader is free to set
/// that to red. Together with the canvas law above, this is the whole claim —
/// every byte of the frame is a colour the theme chose — so a cell that paints
/// nothing is a bug now, not a fact.
#[test]
fn no_cell_of_a_painted_frame_leaves_its_ink_to_the_terminal() {
    for (width, height) in [(40, 10), (200, 60)] {
        let buf = busy(&mut app(), width, height);
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                let cell = &buf[(x, y)];
                assert_ne!(
                    cell.fg,
                    Color::Reset,
                    "({x},{y}) at {width}x{height} is {:?} with no ink of its own — the \
                     terminal's default foreground, which the theme did not choose",
                    cell.symbol()
                );
            }
        }
    }
}

/// The cell law over the pictures: an escape covers cells without painting
/// them, so the backdrop those cells carry is the theme's and never the
/// terminal's.
#[test]
fn the_cell_law_holds_over_a_frame_with_the_strip_lit_and_the_face_shown() {
    let buf = pictured(100, 40);
    let mut covered = 0;
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            let cell = &buf[(x, y)];
            let at = format!("({x},{y})");
            assert!(
                theme::owns(cell.fg),
                "{at} is painted in a colour the theme does not name: {:?}",
                cell.fg
            );
            assert!(
                SURFACES.contains(&cell.bg),
                "{at} is {:?} on {}, a surface the theme does not paint",
                cell.symbol(),
                cell.bg
            );
            covered += usize::from(cell.symbol().contains('\u{10EEEE}'));
        }
    }
    assert!(
        covered > 20,
        "the strip is lit and the face is placed: {covered} placeholder cells"
    );
}
