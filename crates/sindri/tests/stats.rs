//! The `/stats` screen from the outside: the facts the rail gives up, drawn whole on the
//! frame's whole band — opened by a `/stats` line, closed by `Esc`, and painted instead of the
//! bands under it rather than over them.
//!
//! The screen's own rows and its layout are the module's to prove (`src/stats.rs` reads them
//! without a buffer); what is proved here is the frame's half: that a reader can reach the
//! screen, that it draws what the frame was handed, and that it is a screen and not a band.

use std::path::PathBuf;

use ratatui::Terminal;
use ratatui::backend::TestBackend;

use sindri::commands::{self, Command};
use sindri::composer::{Key, Outcome};
use sindri::keys::Mode;
use sindri::rail::{Ctx, Facts};
use sindri::view::View;
use sindri_core::policy::{Enforcement, PolicyRecord};
use sindri_core::records::{Dials, SessionDocument, Usage};
use sindri_core::run::{State as RunState, Stop};
use sindri_core::wire::Wire;

/// The GUID of the session the screen's run is writing: the whole of it, which is one of the
/// facts no rail column holds.
const GUID: &str = "9f2c7a41-6a1e-4b3d-8f5c-0d2e7a4b9c61";

/// The facts a run's rail draws, in the values a live run holds them.
fn facts() -> Facts {
    Facts {
        wire: Some(String::from("anthropic")),
        model: Some(String::from("claude-sonnet-4-5-20250929")),
        think: Some(String::from("high")),
        turns: Some((6, 250)),
        ctx: Some(Ctx {
            size: 48_200,
            window: Some(200_000),
        }),
        cache: Some(88),
        workspace: Some(PathBuf::from("/home/glemsom/src/sindri")),
        branch: Some(String::from("spec/1-capability")),
        skills: Some(2),
        sandbox: Some(Enforcement::Bubblewrap),
        session: Some(String::from(GUID)),
        temp: Some(PathBuf::from(format!(
            "/home/glemsom/.local/state/sindri/sessions/{GUID}/tmp"
        ))),
    }
}

/// The record a stopped run wrote, as the core holds it.
fn document() -> SessionDocument {
    SessionDocument {
        session: String::from(GUID),
        wire: Wire::Anthropic,
        model: String::from("claude-sonnet-4-5-20250929"),
        enforcement: Enforcement::Bubblewrap,
        policy: PolicyRecord {
            enforcement: Enforcement::Bubblewrap,
            writable: vec![String::from("/home/glemsom/src/sindri")],
            timeout_seconds: 120,
            command_budget_ms: 3_600_000,
            output_budget_bytes: 8 * 1024 * 1024,
            description: String::from("the boundary the model was told about"),
        },
        workspace: PathBuf::from("/home/glemsom/src/sindri"),
        branch: Some(String::from("spec/1-capability")),
        commit: Some(String::from("16becb5")),
        dials: Dials {
            max_turns: 250,
            command_budget_ms: 3_600_000,
            output_budget_bytes: 8 * 1024 * 1024,
        },
        start: String::from("2026-01-01T00:00:00Z"),
        end: Some(String::from("2026-01-01T00:04:00Z")),
        turns: 6,
        usage: Usage {
            input: Some(48_200),
            output: Some(3_100),
            total: Some(51_300),
            cached_input: Some(12_000),
            reasoning: Some(900),
        },
        stopped: RunState::Stopped(Stop::Final),
        route: Some(sindri_core::provider::Route::Chat),
        thinking: true,
    }
}

/// The rows of the frame as a reader sees it, every cell of every row.
fn painted(view: &mut View, cols: u16, rows: u16) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(cols, rows)).expect("a terminal");
    terminal
        .draw(|frame| view.paint(frame.area(), frame.buffer_mut()))
        .expect("a frame");
    let buffer = terminal.backend().buffer();
    (0..rows)
        .map(|y| {
            (0..cols)
                .map(|x| {
                    buffer
                        .cell((x, y))
                        .unwrap_or_else(|| panic!("row {y}, column {x} is outside the frame"))
                        .symbol()
                })
                .collect()
        })
        .collect()
}

/// The frame a screen is opened on: a run's facts and its record handed in, and `/stats` typed
/// and sent the way a reader opens it.
fn opened() -> View {
    let mut view = View::boot();
    view.set_facts(facts());
    view.set_record(&document());
    view.paste("/stats");
    assert_eq!(
        view.key(Key::Enter),
        Outcome::Edited,
        "a `/stats` line is the frame's to answer, and a screen opening is a frame to paint"
    );
    view
}

#[test]
fn a_slash_stats_line_opens_the_screen_and_escape_gives_the_frame_back() {
    let mut view = opened();

    assert_eq!(
        view.mode(),
        Mode::Stats,
        "the stats screen is on top of the stack, over the draft it was opened from"
    );
    assert_eq!(
        view.key(Key::Escape),
        Outcome::Edited,
        "Esc closes it, so there is a frame to paint without it"
    );
    assert_eq!(
        view.mode(),
        Mode::Composer,
        "and the draft has the keys again"
    );

    view.paste("why");
    assert_eq!(
        view.composer().text(),
        "why",
        "the screen closed onto the draft it opened over, untouched"
    );
}

#[test]
fn the_screen_draws_the_facts_no_rail_column_holds_whole() {
    let mut view = opened();
    let rows = painted(&mut view, 120, 40);

    for asked in [
        "/stats", GUID, "route", "input", "total", "start", "end", "commit",
    ] {
        assert!(
            rows.iter().any(|row| row.contains(asked)),
            "the screen draws `{asked}`, which is a fact the rail gives up: {rows:#?}"
        );
    }
}

#[test]
fn a_band_too_short_for_the_rows_counts_the_ones_it_cannot_hold() {
    let mut view = opened();
    let rows = painted(&mut view, 80, 24);

    assert!(
        rows.iter().any(|row| row.contains("more")),
        "a band that cannot hold every row counts them in the rail's own words instead of \
         drawing one in part: {rows:#?}"
    );
    for asked in ["session", "temp"] {
        assert!(
            !rows.iter().any(|row| row.contains(asked)),
            "the rows that went are the ones at the end of the screen, and `{asked}` is one \
             of them: {rows:#?}"
        );
    }
}

#[test]
fn the_screen_paints_none_of_the_bands_under_it() {
    let mut view = opened();
    let rows = painted(&mut view, 120, 40);
    let screen = rows.join("\n");

    for band in ["Enter send", "? keys", "/ commands"] {
        assert!(
            !screen.contains(band),
            "a screen is the whole frame, so the footer's `{band}` is not painted under it: \
             {screen}"
        );
    }
    assert!(
        rows.last()
            .is_some_and(|row| row.trim().is_empty() || !row.contains('─')),
        "and the composer's hairline is not painted under it either: {screen}"
    );
}

#[test]
fn a_frame_handed_no_record_and_no_fact_still_opens_the_screen_with_its_title_and_no_row() {
    let mut view = View::boot();
    view.paste("/stats");
    view.key(Key::Enter);

    let rows = painted(&mut view, 80, 24);

    assert_eq!(
        view.mode(),
        Mode::Stats,
        "a frame that knows nothing still has a screen to open"
    );
    assert!(
        rows.first().is_some_and(|row| row.contains("/stats")),
        "the screen is headed by its own name: {rows:#?}"
    );
    assert!(
        rows[1..].iter().all(|row| row.trim().is_empty()),
        "and a frame handed nothing has no row to draw: {rows:#?}"
    );
}

#[test]
fn slash_status_names_no_command_and_slash_stats_names_the_screen() {
    assert_eq!(
        commands::read("/stats"),
        Some(Command::Stats),
        "`/stats` is the word a reader types to ask what a run cost"
    );
    assert_eq!(
        commands::read("/status"),
        None,
        "`/status` is the run's own five words, and no command names a screen after it"
    );
    assert_eq!(
        commands::read("/stats extra"),
        Some(Command::Stats),
        "the command takes no argument, and one typed after it is not a second command"
    );
}
