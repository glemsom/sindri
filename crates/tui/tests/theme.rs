//! The theme's promises, held by something other than the renderers: every
//! colour the frame paints is one the theme names, no renderer carries a
//! literal of its own, and the faint grey is legible on every surface
//! (ADR-0016).

#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken contract reports itself"
)]

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::Color;
use sindri_core::agent::Event;
use sindri_tui::driver::{CallOver, Finished, Side, Spilled, Stop};
use sindri_tui::render;
use sindri_tui::theme;
use sindri_tui::{App, Launch};

fn app() -> App {
    App::new(
        Launch {
            session: "4f1c8f2e-8a24-4b93-9c0e-0d3f0e5c1a77".to_string(),
            temp: "/state/sindri/sessions/4f1c8f2e/tmp".to_string(),
            branch: Some("t20-the-surface".to_string()),
            sandbox: "bwrap · 1 writable".to_string(),
            workspace: "~/git/sindri".to_string(),
            provider: "anthropic".to_string(),
            model: "claude-sonnet-4-6".to_string(),
            max_turns: 250,
            context_limit: Some(200_000),
        },
        None,
    )
}

/// A frame with one of everything that paints: prose and inline code, a diff
/// fence, a call's two sides, an error note, the rail, the composer, and a
/// modal on the raised tier above them.
fn painted() -> Buffer {
    let mut app = app();
    app.event(Event::TextDelta {
        turn: 1,
        text: "A `code` word and a fence:\n\n```diff\n-old\n+new\n```\n".to_string(),
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
    let mut terminal = Terminal::new(TestBackend::new(100, 40)).expect("a test terminal");
    terminal
        .draw(|frame| render::draw(&mut app, frame))
        .expect("the frame draws");
    terminal.backend().buffer().clone()
}

#[test]
fn every_colour_the_frame_paints_comes_from_the_theme() {
    let buf = painted();
    let named = |colour: Color| colour == Color::Reset || theme::PALETTE.contains(&colour);
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
    let surfaces: Vec<Color> = [theme::PANEL, theme::RAISED]
        .into_iter()
        .filter(|surface| {
            (0..buf.area.height).any(|y| (0..buf.area.width).any(|x| buf[(x, y)].bg == *surface))
        })
        .collect();
    assert_eq!(
        surfaces.len(),
        2,
        "the frame paints all three tiers: the canvas, a panel and raised"
    );
}

/// The theme is the only file that may name a colour: a renderer that writes
/// one down is a renderer that has left the theme behind.
#[test]
fn no_renderer_hard_codes_a_colour() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    let mut stack = vec![root];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("the source tree") {
            let path = entry.expect("a source entry").path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs")
                && path.file_name().is_some_and(|name| name != "theme.rs")
            {
                files.push(path);
            }
        }
    }
    assert!(files.len() > 10, "the source tree was found: {files:?}");
    let named = [
        "Color::Rgb(0x",
        "Color::Black",
        "Color::White",
        "Color::Gray",
        "Color::DarkGray",
        "Color::Red",
        "Color::Green",
        "Color::Blue",
        "Color::Yellow",
        "Color::Magenta",
        "Color::Cyan",
    ];
    for file in files {
        let source = std::fs::read_to_string(&file).expect("a source file");
        // A unit test may name a colour to prove a style is copied without
        // one; the renderer above it may not.
        let source = source
            .split("#[cfg(test)]")
            .next()
            .unwrap_or_default()
            .to_string();
        for needle in named {
            assert!(
                !source.contains(needle),
                "{} names a colour ({needle}) instead of asking the theme",
                file.display()
            );
        }
    }
}

/// WCAG's relative luminance, so the ratio below is the one a reader's eye
/// is owed and not a guess.
fn luminance(colour: Color) -> f64 {
    let Color::Rgb(r, g, b) = colour else {
        panic!("a contrast ratio needs a truecolour");
    };
    let channel = |value: u8| {
        let value = f64::from(value) / 255.0;
        if value <= 0.039_28 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
}

fn ratio(fg: Color, bg: Color) -> f64 {
    let (brighter, darker) = {
        let (a, b) = (luminance(fg), luminance(bg));
        if a > b { (a, b) } else { (b, a) }
    };
    (brighter + 0.05) / (darker + 0.05)
}

#[test]
fn faint_clears_four_and_a_half_to_one_on_the_panel_and_on_the_canvas() {
    // The canvas is the terminal's own black: the frame paints nothing there.
    let canvas = Color::Rgb(0, 0, 0);
    for surface in [theme::PANEL, canvas] {
        let cleared = ratio(theme::FAINT, surface);
        assert!(
            cleared >= 4.5,
            "faint on {surface:?} is {cleared:.2}:1, under the 4.5:1 a body needs"
        );
    }
}
