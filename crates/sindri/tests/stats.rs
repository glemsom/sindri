//! `/stats` from the outside: opened by a `/stats` line, closed by `Esc`, and drawn on a card over the
//! bands it was opened from rather than in place of them. The screen's own rows are the module's to
//! prove; the frame's half is that a reader can reach the screen and that it draws what it was handed.

use ratatui::Terminal;
use ratatui::backend::TestBackend;

use sindri::commands::{self, Command};
use sindri::composer::{Key, Outcome};
use sindri::keys::Mode;
use sindri::view::View;

#[allow(dead_code)] // the harness draws every frame; this binary asks it for the run only
mod fixture; // one frame rendered at a fixed size into the text a fixture holds

use fixture::{GUID, document, facts};

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
fn the_bands_the_screen_stands_over_are_painted_under_its_card() {
    let mut view = opened();
    let rows = painted(&mut view, 120, 40);
    let last = rows.len() - 1;

    assert!(
        rows[last].starts_with("Enter send"),
        "the footer is painted in the gutter the card leaves it, so the reader sees the frame \
         they will return to: {:?}",
        rows[last]
    );
    assert!(
        rows[last - 2].starts_with('─'),
        "and so is the composer's band, its hairline running where the gutter reaches it: {:?}",
        rows[last - 2]
    );
    assert!(
        rows[0].contains("/stats"),
        "the card is what the eye lands on: {:?}",
        rows[0]
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
        "the screen is named in the top border of the card it is drawn on: {rows:#?}"
    );
    let sides = |row: &str| row.matches('│').count();
    assert!(
        rows[1..rows.len() - 1].iter().all(|row| sides(row) == 2),
        "and a frame handed nothing has no row to draw between the card's two sides: {rows:#?}"
    );
    assert!(
        rows.last().is_some_and(|row| row.contains("Esc")),
        "the card's bottom border names the key that leaves, a frame of no row and all: {rows:#?}"
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
