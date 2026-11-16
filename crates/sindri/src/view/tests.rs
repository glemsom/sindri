//! What a golden frame cannot hold: the cell the terminal's cursor is put on, and the bands
//! under the cells, which a fixture reads by ink and never by background.

use sindri_core::policy::Enforcement;

use sindri_core::redact::SecretSet;
use sindri_core::run::Exit;

use ratatui::style::Color;

use std::path::PathBuf;

use super::*;
use crate::composer::{MAX_ROWS, MIN_ROWS, PLACEHOLDER};
use crate::ledger::Boot;

use crate::capability::caps::{inked, pictured, pointing, sized};
use crate::ink::{Face, Look};
use crate::palette::{Band, Edge};
use crate::pictures::PLACEHOLDER as PLACED;
use crate::readback::under;
use crate::rows::THUMB;

/// Enough facts that the ledger fills its column top to bottom.
fn rail() -> ledger::Facts {
    ledger::Facts {
        wire: Some(String::from("anthropic")),
        model: Some(String::from("claude-sonnet-4-5-20250929")),
        think: Some(String::from("high")),
        turns: Some((6, 250)),
        ctx: Some(crate::ledger::Ctx {
            size: 48_200,
            window: Some(200_000),
        }),
        cache: Some(88),
        workspace: Some(PathBuf::from("/srv/workspace")),
        branch: Some(String::from("spec/1-capability")),
        loaded: vec![String::from("bash"), String::from("records")],
        sandbox: Some(Enforcement::Bubblewrap),
        session: Some(String::from("9f2c7a41-6a1e-4b3d-8f5c-0d2e7a4b9c61")),
        temp: Some(PathBuf::from("/srv/sessions/9f2c7a41/tmp")),
        ..ledger::Facts::default()
    }
}

/// The look the cell at `x`, `y` of the frame reads back as.
fn look_at(buffer: &Buffer, palette: &Palette, x: u16, y: u16) -> Look {
    Look::of(buffer.cell((x, y)).expect("a cell of the frame"), palette)
        .unwrap_or_else(|| panic!("row {y}, column {x} reads back as no look"))
}

/// The frame a pass laid out, read off the value it handed back.
fn laid_out(painted: &Painted) -> Layout {
    painted
        .layout()
        .expect("a pass that drew a frame hands back the frame it laid out")
}

/// The card's body, as the pass that drew it laid the cells out.
fn card_body(painted: &Painted) -> Rect {
    laid_out(painted)
        .card
        .expect("a frame drawn under a card hands back the body the card gave it")
}

/// A frame holding a draft of `lines` lines painted in `palette` at `cols` by `rows`, and where
/// that pass laid it out.
fn composer(palette: Palette, lines: usize, cols: u16, rows: u16) -> (Buffer, Layout) {
    let draft: Vec<String> = (1..=lines).map(|line| line.to_string()).collect();
    let mut view = View::of(inked(palette), Facts::default().asked(), Transcript::none());
    view.paste(&draft.join("\n"));
    let (buffer, told) = painted_and_told(&mut view, cols, rows);
    (buffer, laid_out(&told))
}

#[test]
fn the_composer_is_lit_along_its_top_and_shadowed_under_its_bottom() {
    let palette = Palette::TRUECOLOR;
    for lines in 1..=5 {
        for (cols, rows) in [(120, 40), (60, 24)] {
            let (buffer, laid) = composer(palette, lines, cols, rows);
            let band = laid.composer;

            for row in 0..band.height {
                for x in band.x..band.right() {
                    assert_eq!(
                        under(&buffer, x, band.y + row),
                        palette.step(row, band.height),
                        "row {row} of the {} at {cols}x{rows} sits on no step of the ramp, \
                             so the band is not one raised surface",
                        band.height
                    );
                }
            }
            assert_eq!(
                under(&buffer, band.x, band.y),
                palette.edge(Edge::Lit),
                "the top row of the {} at {cols}x{rows} is not the lit edge",
                band.height
            );
            assert_eq!(
                under(&buffer, band.x, band.bottom() - 1),
                palette.edge(Edge::Shadow),
                "the bottom row of the {} at {cols}x{rows} is not the shadow",
                band.height
            );
        }
    }
}

#[test]
fn the_composer_that_takes_the_keys_lights_its_edge_row_with_the_accent() {
    let palette = Palette::TRUECOLOR;
    let faint = Look::new(Ink::Accent, Face::Faint);
    for lines in 1..=5 {
        for (cols, rows) in [(120, 40), (60, 24)] {
            let (buffer, laid) = composer(palette, lines, cols, rows);
            let band = laid.composer;

            for x in band.x..band.right() {
                assert_eq!(
                    look_at(&buffer, &palette, x, band.y),
                    faint,
                    "column {x} of the {} row composer's edge row at {cols} columns is not the \
                        accent drawn faint, and the draft is the surface that takes the keys",
                    band.height
                );
            }
        }
    }
}

#[test]
fn no_band_is_lit_twice_or_shadowed_twice_at_either_end_of_its_height() {
    let palette = Palette::TRUECOLOR;
    for lines in [1, 5] {
        let (buffer, laid) = composer(palette, lines, 120, 40);
        let band = laid.composer;
        let lit_and_shadowed = |edge: Edge| -> Vec<u16> {
            (band.y..band.bottom())
                .filter(|y| {
                    (band.x..band.right()).all(|x| under(&buffer, x, *y) == palette.edge(edge))
                })
                .collect()
        };

        assert_eq!(
            band.height,
            if lines == 1 { MIN_ROWS } else { MAX_ROWS },
            "the draft of {lines} lines is a band of {} rows, so this draws neither end of \
                 the heights a band takes",
            band.height
        );
        assert_eq!(
            lit_and_shadowed(Edge::Lit),
            [band.y],
            "a {} band is lit on more than its one top row, so the edge is drawn twice",
            band.height
        );
        assert_eq!(
            lit_and_shadowed(Edge::Shadow),
            [band.y + band.height - 1],
            "a {} band is shadowed on more than its one bottom row",
            band.height
        );
        for y in band.y..band.bottom() {
            for x in band.x..band.right() {
                assert_ne!(
                    under(&buffer, x, y),
                    Color::Reset,
                    "row {y} of the {}-row band leaves column {x} on the frame's own ground, \
                         so the frame is read through its own raised band",
                    band.height
                );
            }
        }
    }
}

#[test]
fn the_ramp_moves_the_surface_under_the_cells_and_adds_no_ink_but_the_accent_edge() {
    let (coloured, laid) = composer(Palette::TRUECOLOR, 3, 120, 40);
    let (flat, flat_laid) = composer(Palette::SLOTS, 3, 120, 40);
    assert_eq!(laid.composer, flat_laid.composer);
    let band = laid.composer;

    let area = coloured.area;
    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            let takes_keys = y == band.y && (band.x..band.right()).contains(&x);
            if takes_keys {
                continue;
            }
            assert_eq!(
                look_at(&coloured, &Palette::TRUECOLOR, x, y),
                look_at(&flat, &Palette::SLOTS, x, y),
                "row {y}, column {x} is read in a colour a frame drawn in none is not: the \
                     ramp is a surface, and the accent edge on the composer's own edge row is the \
                     one ink the draft that takes the keys adds"
            );
        }
    }
}

#[test]
fn the_composer_gives_the_accent_up_while_a_screen_takes_the_keys() {
    let palette = Palette::TRUECOLOR;
    for mode in [
        Mode::Keys,
        Mode::Files,
        Mode::Models,
        Mode::Settings,
        Mode::Stats,
    ] {
        let mut view = View::of(
            Capabilities::default(),
            Facts::default().asked(),
            Transcript::none(),
        );
        view.paste("the draft behind the card");
        view.screen(mode);

        let (buffer, told) = painted_and_told(&mut view, 120, 40);
        let band = laid_out(&told).composer;
        let (left, right) = card::border_columns(card_body(&told));
        assert!(
            band.x < left && right + 1 < band.right(),
            "the card leaves the composer's edge row visible in its gutters at {left} and \
                 {right}, and the band it covers reaches from {} to {}",
            band.x,
            band.right()
        );
        // The composer's edge row is read in the card's two gutters, where the scrim leaves
        // an accent dimmed rather than bright.
        for x in (band.x..left).chain(right + 1..band.right()) {
            assert_eq!(
                look_at(&buffer, &palette, x, band.y),
                palette.dim_edge(),
                "column {x} of the composer's edge row under {} is not the dim edge, and the \
                     card is the surface that takes the keys",
                mode.name()
            );
        }
    }
}

/// The look the composer's edge row is drawn in, in the frame the view paints now.
fn edge_row(view: &mut View, palette: &Palette) -> Look {
    let (buffer, told) = painted_and_told(view, 120, 40);
    let band = laid_out(&told).composer;
    look_at(&buffer, palette, band.x, band.y)
}

#[test]
fn the_accent_edge_recedes_while_the_window_is_unfocused_and_returns_when_it_is_focused() {
    let palette = Palette::TRUECOLOR;
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::none(),
    );
    view.paste("the draft the composer band is drawn from");
    let lit = edge_row(&mut view, &palette);

    view.handed(view.facts().clone().focused(false));
    let away = edge_row(&mut view, &palette);

    view.handed(view.facts().clone().focused(true));
    let back = edge_row(&mut view, &palette);

    assert_eq!(
        lit,
        palette.accent_edge(),
        "a window nobody has reported on is the one being read, so its edge is the accent"
    );
    assert_eq!(
        away,
        palette.dim_edge(),
        "and the accent recedes while the terminal says nobody is looking"
    );
    assert_eq!(back, lit, "and returns when the reader comes back");
}

#[test]
fn a_frame_with_no_colour_draws_no_accent_edge_on_any_surface() {
    let palette = Palette::SLOTS;
    for open in [None, Some(Mode::Keys)] {
        let mut view = View::of(inked(palette), Facts::default().asked(), Transcript::none());
        view.paste("the draft the composer band is drawn from");
        if let Some(mode) = open {
            view.screen(mode);
        }

        let area = screen(120, 40);
        let mut buffer = Buffer::empty(area);
        view.paint(area, &mut buffer);

        for y in area.y..area.bottom() {
            for x in area.x..area.right() {
                assert!(
                    !look_at(&buffer, &palette, x, y).faces().has(Face::Faint),
                    "row {y}, column {x} is dimmed to say it takes the keys, and a reader who \
                         asked for no colour is given focus by the card's border and the scrim"
                );
            }
        }
    }
}

#[test]
fn a_frame_with_no_colour_draws_no_band_and_no_elevation() {
    let palette = Palette::SLOTS;
    let mut view = View::of(
        inked(palette),
        Facts::default().asked().railed(rail()),
        Transcript::of(vec![Entry::Assistant {
            turn: 1,
            text: SecretSet::default().redact("```rust\nfn main() {}\n```\n\nand prose"),
        }]),
    );
    view.paste("the draft");

    let area = screen(120, 40);
    let mut buffer = Buffer::empty(area);
    view.paint(area, &mut buffer);

    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            assert_eq!(
                under(&buffer, x, y),
                Color::Reset,
                "row {y}, column {x} of a frame with no colour is given a ground: a reader \
                     who asked for no colour asked for their own, and every frame they read is \
                     flat"
            );
        }
    }
}

#[test]
fn a_frame_with_no_colour_draws_the_composer_as_a_flat_band() {
    let (buffer, laid) = composer(Palette::SLOTS, 5, 120, 40);
    let band = laid.composer;
    for y in band.y..band.bottom() {
        for x in band.x..band.right() {
            assert_eq!(
                under(&buffer, x, y),
                Color::Reset,
                "row {y} of a frame with no colour draws a ground at column {x}"
            );
        }
    }
}

#[test]
fn a_frame_with_one_row_to_give_the_composer_draws_it_lit() {
    let palette = Palette::TRUECOLOR;
    let (buffer, laid) = composer(palette, 1, 60, 2);
    let band = laid.composer;

    assert_eq!(band.height, 1, "a two-row frame gives the composer one row");
    assert_eq!(
        under(&buffer, band.x, band.y),
        palette.edge(Edge::Lit),
        "one row has no room for both ends, so it is drawn lit and never on the frame's own \
             ground"
    );
}

#[test]
fn sixty_columns_show_the_raised_band_and_lose_no_hint_from_the_footer() {
    let palette = Palette::TRUECOLOR;
    let (buffer, laid) = composer(palette, 1, 60, 24);
    let band = laid.composer;

    assert_eq!(
        under(&buffer, band.x, band.y),
        palette.edge(Edge::Lit),
        "a sixty-column frame draws no lit edge along the top of the composer"
    );
    assert_eq!(
        under(&buffer, band.x, band.bottom() - 1),
        palette.edge(Edge::Shadow),
        "a sixty-column frame leaves no shadow under the composer"
    );
    assert_eq!(
        drawn_row(&buffer, laid.footer.y).trim_end(),
        "Enter send · / commands · @ files · ? keys · Ctrl-Q quit",
        "the raised band takes no row the footer had, and every hint that fits in sixty \
             columns is still drawn"
    );
}

#[test]
fn the_transcript_sits_in_the_well_and_the_rail_beside_it_carries_the_light() {
    let palette = Palette::TRUECOLOR;
    for (cols, rows) in [(120, 40), (100, 40)] {
        let mut view = View::of(
            Capabilities::default(),
            Facts::default().asked().railed(rail()),
            Transcript::of(vec![Entry::Assistant {
                turn: 1,
                text: SecretSet::default().redact("the transcript the rail is drawn beside"),
            }]),
        );

        let (buffer, told) = painted_and_told(&mut view, cols, rows);
        let laid = laid_out(&told);
        let transcript = laid.transcript;
        let (shape, rail) = laid.rail.expect("a frame this wide holds a ledger");
        let rail = rail.area;
        assert_eq!(shape, Rail::Ledger);
        assert_eq!(
            transcript.right(),
            rail.x,
            "the transcript loses a column to the lit rule: it ends at column {} and the rail \
                 begins at {}",
            transcript.right(),
            rail.x
        );

        for y in transcript.y..transcript.bottom() {
            for x in transcript.x..transcript.right() {
                assert_eq!(
                    under(&buffer, x, y),
                    palette.band(Band::Well),
                    "row {y}, column {x} of the transcript at {cols} columns sits on no well, \
                         so the transcript is no elevation"
                );
            }
        }
        for y in rail.y..rail.bottom() - 1 {
            assert_eq!(
                under(&buffer, rail.x, y),
                palette.edge(Edge::Lit),
                "row {y} of the rail at {cols} columns draws no lit rule on its boundary \
                     with the transcript"
            );
            assert_eq!(
                under(&buffer, rail.x + 1, y),
                palette.band(Band::Rail),
                "row {y} of the rail at {cols} columns is lit past its boundary column, so \
                     the rail is no surface of its own"
            );
        }
        for x in rail.x..rail.right() {
            assert_eq!(
                under(&buffer, x, rail.bottom() - 1),
                palette.edge(Edge::Shadow),
                "column {x} of the rail's last row at {cols} columns is not the shadow under \
                     the band"
            );
        }
    }
}

/// A band is a surface only because a name is read from a foreground and a modifier.
#[test]
fn the_rail_and_a_code_well_are_painted_over_their_own_surface() {
    let palette = Palette::TRUECOLOR;
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked().railed(rail()),
        Transcript::of(vec![Entry::Assistant {
            turn: 1,
            text: SecretSet::default().redact(
                "```rust\nfn main() {\n    fence.words();\n}\n```\n\nand the prose under it",
            ),
        }]),
    );
    view.paste("the draft the composer band is drawn from");

    let area = screen(120, 40);
    let mut buffer = Buffer::empty(area);
    let told = view.paint(area, &mut buffer);

    let cells = |band: Band| -> Vec<(u16, u16)> {
        (area.y..area.bottom())
            .flat_map(|y| (area.x..area.right()).map(move |x| (x, y)))
            .filter(|(x, y)| {
                buffer
                    .cell((*x, *y))
                    .is_some_and(|cell| cell.bg == palette.band(band))
            })
            .collect()
    };

    for (band, at_least) in [(Band::Rail, 100), (Band::Well, 100)] {
        let on = cells(band);
        assert!(
            on.len() >= at_least,
            "{band:?} paints a surface under its cells, and it covered {} of them",
            on.len()
        );
        for (x, y) in on {
            let cell = buffer.cell((x, y)).expect("a cell of the frame");
            assert!(
                Look::of(cell, &palette).is_some(),
                "row {y}, column {x} sits on {band:?} and reads back as no ink: in the \
                     background {:?}, the foreground {:?}",
                cell.bg,
                cell.fg
            );
        }
    }

    let last = laid_out(&told).footer.y;
    let banded = (area.x..area.right()).find(|x| {
        let cell = buffer.cell((*x, last)).expect("a cell of the footer");
        [Band::Rail, Band::Well]
            .into_iter()
            .any(|band| cell.bg == palette.band(band))
    });
    assert_eq!(
        banded, None,
        "the footer sits on no band, and column {banded:?} of it sits on one"
    );
}

/// The launch a boot frame is drawn from: a config's wire and model, the workspace the launch
/// stands in and its branch, and the enforcement its policy holds.
fn boot() -> Boot {
    Boot {
        wire: Some(sindri_core::wire::Wire::Anthropic),
        model: Some(String::from("claude-sonnet-4-5")),
        workspace: PathBuf::from("/srv/workspace"),
        branch: Some(String::from("main")),
        enforcement: Enforcement::Bubblewrap,
    }
}

/// The rows of the transcript band a boot frame draws, from its left edge and cut where the
/// rail's columns begin: the rail has rows of its own, and they are not the splash screen's.
fn splash(cols: u16, rows: u16) -> Vec<String> {
    let (buffer, band) = splash_frame(cols, rows, false);
    band_of(&buffer, band)
}

/// The cells a splash screen frame `cols` by `rows` is painted into, and what it draws: a
/// terminal that places pictures is handed the rail's face beside the block, and one that places
/// none is handed the splash screen alone.
fn splash_frame(cols: u16, rows: u16, draws: bool) -> (Buffer, Rect) {
    let mut view = View::of(
        match draws {
            true => pictured(),
            false => Capabilities::default(),
        },
        Facts::boot(boot(), None),
        Transcript::none(),
    );
    let (buffer, told) = painted_and_told(&mut view, cols, rows);
    (buffer, laid_out(&told).transcript)
}

/// Whether this cell is no glyph of the splash screen's: one a picture is placed over, or the
/// bytes that place it.
fn placed(cell: &ratatui::buffer::Cell) -> bool {
    cell.skip || cell.symbol().contains(PLACED) || cell.symbol().contains('\u{1b}')
}

/// The rows of `band` in `buffer` a splash screen frame drew, its left edge first and cut where
/// the rail's columns begin. A cell a picture is placed over reads as a blank: it is no glyph
/// of the splash screen's, and the bytes that place it are no word either.
fn band_of(buffer: &Buffer, band: Rect) -> Vec<String> {
    (band.y..band.bottom())
        .map(|y| {
            (band.x..band.right())
                .map(|x| {
                    let cell = buffer.cell((x, y)).expect("a cell of the band");
                    match placed(cell) {
                        true => ' ',
                        false => cell.symbol().chars().next().unwrap_or(' '),
                    }
                })
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect()
}

/// The splash screen's block as `band_of` draws it: the rows from the first it fills to the
/// last, with the margin the band gives it on every side trimmed away.
fn block(drawn: &[String]) -> Vec<String> {
    let mut rows: Vec<String> = drawn
        .iter()
        .skip_while(|row| row.is_empty())
        .map(|row| row.trim_start().to_owned())
        .collect();
    while rows.last().is_some_and(|row| row.is_empty()) {
        rows.pop();
    }
    rows
}

/// The rows of `buffer` inside `band` that are a placement of Sindri's face, read off the
/// glyph every cell of a picture is written as.
fn pictured_rows(buffer: &Buffer, band: Rect) -> Vec<u16> {
    (band.y..band.bottom())
        .filter(|y| (band.x..band.right()).any(|x| buffer.cell((x, *y)).is_some_and(placed)))
        .collect()
}

/// The columns of `row` in `buffer` that hold a glyph, which is where the splash screen's block
/// begins and where it ends.
fn filled_columns(buffer: &Buffer, band: Rect, y: u16) -> (u16, u16) {
    let filled: Vec<u16> = (band.x..band.right())
        .filter(|x| {
            buffer
                .cell((*x, y))
                .is_some_and(|cell| !cell.symbol().trim().is_empty())
        })
        .collect();
    match (filled.first(), filled.last()) {
        (Some(left), Some(right)) => (*left, *right),
        _ => panic!(
            "row {y} of the splash screen holds no glyph: {:?}",
            drawn_row(buffer, y)
        ),
    }
}

/// The row of `band` the splash screen's wordmark stands on: the one row of the block that
/// says the product's name.
fn wordmark_row(buffer: &Buffer, band: Rect) -> u16 {
    (band.y..band.bottom())
        .find(|y| drawn_row(buffer, *y).contains("sindri"))
        .expect("the splash screen draws the wordmark")
}

/// The first and last columns of `buffer` inside `band` that a splash screen frame's block
/// holds a glyph in: the widest row of it, which is where the block begins and where it ends.
fn block_columns(buffer: &Buffer, band: Rect, drawn: &[String]) -> (u16, u16) {
    let (first, last) = filled_rows(drawn, band);
    let mut left = band.right();
    let mut right = band.x;
    for y in first..=last {
        if drawn[usize::from(y - band.y)].is_empty() {
            continue;
        }
        let (row_left, row_right) = filled_columns(buffer, band, y);
        left = left.min(row_left);
        right = right.max(row_right);
    }
    (left, right)
}

/// The rows of `band` a splash screen frame fills, the first and the last: the block's own
/// rows, with the blank rows the band gives it above and under left out.
fn filled_rows(drawn: &[String], band: Rect) -> (u16, u16) {
    let filled: Vec<u16> = drawn
        .iter()
        .enumerate()
        .filter(|(_, row)| !row.is_empty())
        .map(|(at, _)| band.y + u16::try_from(at).expect("a row of a frame"))
        .collect();
    match (filled.first(), filled.last()) {
        (Some(first), Some(last)) => (*first, *last),
        _ => panic!("the splash screen fills no row of the band: {drawn:?}"),
    }
}

#[test]
fn a_frame_draws_the_three_values_it_was_built_from() {
    let mut view = View::of(
        inked(Palette::ANSI),
        Facts::default().asked().railed(rail()),
        Transcript::of([Entry::Turn { turn: 1 }]),
    );
    let buffer = painted(&mut view, 120, 40);

    assert_eq!(
        view.capabilities().palette(),
        Palette::ANSI,
        "a frame is drawn in the colours of the terminal it was handed"
    );
    assert!(
        holds(&buffer, "turn 1"),
        "and draws the record it was handed: {:?}",
        frame_rows(&buffer)
    );
    assert!(
        holds(&buffer, "anthropic"),
        "and lays the facts it was handed: {:?}",
        frame_rows(&buffer)
    );
}

#[test]
fn a_frame_draws_the_record_from_the_row_the_reader_stands_at() {
    let turns = (1..=80).map(|turn| Entry::Turn { turn });
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::of(turns).scrolled(Scroll::Back(3)),
    );
    let buffer = painted(&mut view, 120, 40);

    assert_eq!(
        view.scroll(),
        Scroll::Back(3),
        "the record is handed with the row the reader stands at, and the frame keeps it"
    );
    let (notice, _) = notice_of(&buffer);
    assert!(
        notice.starts_with("3 rows back"),
        "and the frame says how far back it stands: {notice:?}"
    );
}

#[test]
fn a_frame_built_with_no_facts_draws_the_splash_screen() {
    let mut view = View::of(
        Capabilities::default(),
        Facts::default(),
        Transcript::none(),
    );
    assert!(
        drawn(&mut view, 120, 40).contains("sindri"),
        "a frame handed no facts is the splash screen, and its wordmark is the screen"
    );
}

#[test]
fn a_frame_handed_a_runs_facts_is_not_the_splash_screen() {
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked().running(),
        Transcript::none(),
    );
    assert!(
        !drawn(&mut view, 120, 40).contains("sindri"),
        "a frame handed a run's facts is that run's frame, and draws no wordmark"
    );
}

#[test]
fn the_frame_a_launch_opens_on_is_a_wordmark_a_line_and_three_commands() {
    assert_eq!(
        block(&splash(120, 40)),
        [
            "sindri",
            "",
            "An AI agent that is a Unix filter and a terminal UI over one core.",
            "",
            "/model     ask the wire what it serves, and name the one to run",
            "/settings  the keys the config holds, and the value each is set to",
            "/stats     the facts the rail is too narrow to hold",
        ],
        "the splash screen is a wordmark, one line saying what the product is, and three \
             commands with the one line each that explains them, set as one block"
    );
}

#[test]
fn the_splash_screen_stands_centred_in_the_band_and_no_row_of_it_is_cut_by_the_edge() {
    for (cols, rows) in [(120, 40), (80, 24), (40, 24), (120, 30)] {
        let (buffer, band) = splash_frame(cols, rows, false);
        let drawn = band_of(&buffer, band);
        let (first, last) = filled_rows(&drawn, band);

        assert!(
            last < band.bottom(),
            "at {cols} by {rows} the block ends on row {last} of a band that ends on {}: no \
                 row of the splash screen is cut by the band's edge",
            band.bottom()
        );
        let (above, under) = (first - band.y, band.bottom() - 1 - last);
        assert!(
            above.abs_diff(under) <= 1,
            "at {cols} by {rows} the band gives the block {above} rows above it and {under} \
                 under it, and a splash screen set in a corner is not a splash screen composed"
        );

        let (left, right) = block_columns(&buffer, band, &drawn);
        let (before, after) = (left - band.x, band.right() - 1 - right);
        assert!(
            before.abs_diff(after) <= 1,
            "at {cols} by {rows} the block stands {before} columns from the band's left edge \
                 and {after} from its right one"
        );
    }
}

#[test]
fn the_ink_of_the_splash_screen_is_the_one_of_a_name_and_of_the_commands_under_it() {
    let palette = Palette::TRUECOLOR;
    let (buffer, band) = splash_frame(120, 40, false);

    let wordmark = wordmark_row(&buffer, band);
    let look =
        |x: u16, y: u16| Look::of(buffer.cell((x, y)).expect("a cell of the band"), &palette);
    let (left, _) = filled_columns(&buffer, band, wordmark);
    assert_eq!(
        look(left, wordmark),
        Some(Look::new(Ink::Accent, Face::Bold)),
        "the wordmark is the one bold row of the frame"
    );
    assert_eq!(look(left, wordmark + 2), Some(Look::plain(Ink::Plain)));
    assert_eq!(
        look(left, wordmark + 4),
        Some(Look::plain(Ink::Accent)),
        "a command is read as the thing to type"
    );
    assert_eq!(
        look(left + 11, wordmark + 4),
        Some(Look::plain(Ink::Dim)),
        "and the line that explains it hangs from one column in a quieter ink"
    );
}

#[test]
fn the_block_stands_on_the_rows_a_terminal_that_places_no_picture_puts_it_on() {
    let (pictured, band) = splash_frame(120, 40, true);
    let (placeless, bare) = splash_frame(120, 40, false);

    assert_eq!(
        band_of(&pictured, band),
        band_of(&placeless, bare),
        "at 120 by 40 the band an answer goes in holds the rows of words it holds where no \
             picture is placed, the wordmark on row {}",
        wordmark_row(&pictured, band)
    );
    assert_eq!(
        wordmark_row(&pictured, band),
        15,
        "and that is the row `boot-120x40` puts the wordmark on"
    );
}

#[test]
fn a_frame_too_small_for_the_block_holds_its_head_and_cuts_no_row_across_it() {
    for (cols, rows, whole) in [(80, 9, (80, 24)), (40, 10, (40, 24)), (24, 12, (24, 24))] {
        let (buffer, band) = splash_frame(cols, rows, false);
        let drawn = band_of(&buffer, band);
        let (taller, taller_band) = splash_frame(whole.0, whole.1, false);
        let all = block(&band_of(&taller, taller_band));

        let held = block(&drawn);
        assert!(
            held.len() < all.len(),
            "the band of a {cols} by {rows} frame is too short for the whole block: {} of \
                 its {} rows",
            held.len(),
            all.len()
        );
        assert_eq!(
            held,
            all[..held.len()],
            "at {cols} by {rows} the splash screen holds the head of its block: the rows a \
                 band cannot hold go whole, and no row of it is cut across by the band's edge"
        );
        for row in &drawn {
            assert!(
                pen::cells(row) <= band.width,
                "a {cols}-column band holds {row:?}, which is wider than it"
            );
        }
    }
}

#[test]
fn no_cell_of_the_band_a_launch_opens_on_is_a_picture() {
    for (cols, rows) in [(120, 40), (120, 14), (30, 24), (80, 24)] {
        let (pictured, band) = splash_frame(cols, rows, true);

        assert!(
            pictured_rows(&pictured, band).is_empty(),
            "at {cols} by {rows} the band an answer goes in holds no picture of the splash \
                 screen's, so no cell of it is a placement: {:?}",
            pictured_rows(&pictured, band)
        );
    }
}

#[test]
fn a_frame_too_narrow_for_a_rail_is_every_cell_a_terminal_that_places_no_picture_draws() {
    let (pictured, _) = splash_frame(30, 24, true);
    let (placeless, _) = splash_frame(30, 24, false);

    assert_eq!(
        pictured, placeless,
        "at 30 by 24 the frame is the splash screen and no rail beside it, so every cell of it \
             is the one a terminal that places no picture draws"
    );
}

#[test]
fn a_splash_screen_narrower_than_its_lines_wraps_them_whole_and_cuts_no_word() {
    for cols in [12, 16, 20, 30, 40, 60] {
        let wrapped = splash(cols, 40).join(" ");
        let words = wrapped.split_whitespace().collect::<Vec<&str>>().join(" ");

        for line in [
            "An AI agent that is a Unix filter and a terminal UI over one core.",
            "ask the wire what it serves, and name the one to run",
            "the keys the config holds, and the value each is set to",
            "the facts the rail is too narrow to hold",
        ] {
            assert!(
                words.contains(line),
                "at {cols} columns the splash screen reads {words:?}, which holds no whole \
                     {line:?}"
            );
        }
    }
}

#[test]
fn eighty_columns_draw_the_splash_screen_whole_and_no_rail_beside_it() {
    let palette = Palette::TRUECOLOR;
    let wide = block(&splash(120, 24));
    let narrow = block(&splash(80, 24));
    assert_eq!(
        narrow, wide,
        "a frame too narrow for a rail draws the same splash screen, and nothing of it is \
             clipped"
    );

    let area = screen(80, 24);
    let mut view = View::of(
        Capabilities::default(),
        Facts::boot(boot(), None),
        Transcript::none(),
    );
    let mut buffer = Buffer::empty(area);
    view.paint(area, &mut buffer);
    let on_rail = (area.y..area.bottom())
        .flat_map(|y| (area.x..area.right()).map(move |x| (x, y)))
        .filter(|(x, y)| {
            buffer
                .cell((*x, *y))
                .is_some_and(|cell| cell.bg == palette.band(Band::Rail))
        })
        .count();
    assert_eq!(
        on_rail, 0,
        "the boot screen is a splash screen and no rail: a strip is a live run's one row, and \
             no run is live at the splash screen"
    );
}

#[test]
fn a_run_sent_from_the_splash_screen_leaves_the_frame_to_the_conversation() {
    let mut view = View::of(
        Capabilities::default(),
        Facts::default(),
        Transcript::none(),
    );
    assert!(
        drawn(&mut view, 120, 40).contains("sindri"),
        "the frame a launch opens on is the splash screen"
    );

    view.settle();

    assert!(
        !drawn(&mut view, 120, 40).contains("sindri"),
        "the splash screen is the first paint pass: a run sent from it replaces it, and it is \
             never drawn again"
    );
}

fn drawn(view: &mut View, cols: u16, rows: u16) -> String {
    let area = screen(cols, rows);
    let mut buffer = Buffer::empty(area);
    view.paint(area, &mut buffer);
    (area.y..area.bottom())
        .map(|y| {
            (area.x..area.right())
                .map(|x| buffer.cell((x, y)).expect("a cell of the frame").symbol())
                .collect::<String>()
        })
        .collect::<Vec<String>>()
        .join("\n")
}

fn screen(cols: u16, rows: u16) -> Rect {
    Rect {
        x: 0,
        y: 0,
        width: cols,
        height: rows,
    }
}

fn drawn_row(buffer: &Buffer, y: u16) -> String {
    let area = buffer.area;
    (area.x..area.right())
        .map(|x| buffer.cell((x, y)).expect("a cell of the row").symbol())
        .collect()
}

/// The row `hints` are drawn as in a footer `width` columns wide, with the queue `depth` deep.
fn hints_drawn(hints: &[Hint], width: u16, depth: &str) -> Buffer {
    let palette = Palette::TRUECOLOR;
    let area = Rect::new(0, 0, width, 1);
    let mut buffer = Buffer::empty(area);
    pen::claim(&mut buffer, area, palette.ground(None));
    hints_row(&mut buffer, area, hints, depth, &palette);
    buffer
}

/// The hints as the footer draws them: each key, one blank, and the words that say what it
/// does.
fn said(hints: &[Hint], depth: &str) -> Vec<String> {
    hints
        .iter()
        .map(|hint| format!("{} {}", hint.key, hint.words(depth)))
        .collect()
}

fn depth(view: &View) -> String {
    view.queue().label().unwrap_or_default()
}

/// The whole set of hints a frame honours, as its footer would draw them were the row wide
/// enough.
fn whole(view: &View) -> Vec<String> {
    said(&view.hints(), &depth(view))
}

fn prompt(text: &str) -> Prompt {
    let mut composer = Composer::new();
    composer.paste(text);
    match composer.key(Key::Enter) {
        Outcome::Send(prompt) => prompt,
        other => panic!("a draft should have been sent, and {other:?} came instead"),
    }
}

fn asking(queued: usize) -> View {
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().running(),
        Transcript::none(),
    );
    for text in ["one", "two", "three"].into_iter().take(queued) {
        view.enqueue(prompt(text));
    }
    view
}

fn drawn_footer(view: &mut View, cols: u16, rows: u16) -> String {
    let (buffer, told) = painted_and_told(view, cols, rows);
    drawn_row(&buffer, laid_out(&told).footer.y)
}

/// The footer row a boot frame draws.
fn footer(cols: u16, rows: u16) -> String {
    drawn_footer(
        &mut View::of(
            Capabilities::default(),
            Facts::default().asked(),
            Transcript::none(),
        ),
        cols,
        rows,
    )
}

#[test]
fn an_idle_frame_names_sending_the_commands_the_files_and_the_keys() {
    assert_eq!(
        footer(120, 24).trim_end(),
        "Enter send · / commands · @ files · ? keys · Ctrl-Q quit · Ctrl-J newline",
        "a boot frame draws every hint it can honour, in survival order and separated by one \
             gap, and no hint a draft in flight would answer instead"
    );
}

#[test]
fn a_live_run_names_cancelling_and_never_sending() {
    assert_eq!(
        drawn_footer(&mut asking(0), 120, 24).trim_end(),
        "Ctrl-C cancel · / commands · @ files · ? keys · Ctrl-Q quit · Ctrl-J newline",
        "a frame with its run in flight draws the key that stops it in the place sending \
             stands in an idle frame, and never names a send that would queue instead"
    );
}

#[test]
fn the_queue_s_depth_is_drawn_where_the_hint_that_recalls_it_stands() {
    assert_eq!(
        drawn_footer(&mut asking(3), 120, 24).trim_end(),
        "Ctrl-C cancel · ↑ 3 queued · / commands · @ files · ? keys · Ctrl-Q quit · \
             Ctrl-J newline",
        "three prompts waiting behind the draft are drawn as the count the queue itself \
             gives, standing where the hint that takes the newest back stands"
    );
    assert!(
        drawn_footer(&mut asking(1), 120, 24).contains("↑ 1 queued"),
        "one prompt waiting says one, and not the number the table was written with"
    );
    for cols in 20..=200u16 {
        assert!(
            !drawn_footer(&mut asking(0), cols, 24).contains('↑'),
            "at {cols} columns a frame with nothing queued draws the key that would recall a \
                 prompt out of an empty queue"
        );
    }
}

#[test]
fn a_hint_a_frame_cannot_answer_is_not_drawn_in_it() {
    for cols in 20..=200u16 {
        assert!(
            !footer(cols, 24).contains("cancel"),
            "at {cols} columns an idle frame draws the key that stops a run, and there is no \
                 run to stop"
        );
        let live = drawn_footer(&mut asking(0), cols, 24);
        assert!(
            !live.contains("Enter") && !live.contains("send"),
            "at {cols} columns a frame with its run in flight draws sending, and a draft sent \
                 now only waits behind the run: {live:?}"
        );
    }
}

/// The hints a footer row reads as: each a key, one blank and the words that say what it does.
fn said_row(row: &str) -> Vec<&str> {
    match row.is_empty() {
        true => Vec::new(),
        false => row.split(GAP).collect(),
    }
}

#[test]
fn each_set_is_cut_in_survival_order_at_four_widths() {
    for mut view in [
        View::of(
            Capabilities::default(),
            Facts::default().asked(),
            Transcript::none(),
        ),
        asking(0),
        asking(3),
    ] {
        let mut cut: Vec<usize> = Vec::new();
        for cols in [120, 72, 52, 40] {
            let drawn = drawn_footer(&mut view, cols, 24);
            cut.push(said_row(drawn.trim_end()).len());
        }

        assert!(
            cut.windows(2).all(|pair| pair[0] > pair[1]) && cut[3] > 0,
            "the four widths a fixture proves cut this frame's set to {cut:?}, which is not \
                 one hint given up at every narrowing"
        );
    }
}

#[test]
fn no_hint_is_cut_mid_word_at_any_width_between_twenty_and_two_hundred() {
    for mut view in [
        View::of(
            Capabilities::default(),
            Facts::default().asked(),
            Transcript::none(),
        ),
        asking(0),
        asking(3),
    ] {
        let set = whole(&view);
        for cols in 20..=200u16 {
            let drawn = drawn_footer(&mut view, cols, 24);
            let row = drawn.trim_end();
            let said = said_row(row);
            let prefix: Vec<&str> = set.iter().take(said.len()).map(String::as_str).collect();

            assert_eq!(
                said, prefix,
                "at {cols} columns the footer reads {row:?}, which is not a run of whole hints \
                     from the front of {set:?}"
            );
            assert!(
                pen::cells(row) <= cols,
                "at {cols} columns the footer's {row:?} is {} cells wide",
                pen::cells(row)
            );
        }
    }
}

#[test]
fn the_slash_is_named_in_every_frame_at_every_width_that_holds_a_hint_after_it() {
    for mut view in [
        View::of(
            Capabilities::default(),
            Facts::default().asked(),
            Transcript::none(),
        ),
        asking(0),
        asking(3),
    ] {
        let set = whole(&view);
        let at = set
            .iter()
            .position(|hint| hint == "/ commands")
            .expect("the table names the slash");
        for cols in 20..=200u16 {
            let drawn = drawn_footer(&mut view, cols, 24);
            let row = drawn.trim_end();
            let held = if row.is_empty() {
                0
            } else {
                row.split(GAP).count()
            };

            assert!(
                held <= at + 1 || row.contains("/ commands"),
                "at {cols} columns the footer reads {row:?}, which holds a hint that stands \
                     after the slash while dropping the key that opens the commands"
            );
        }
    }
}

#[test]
fn a_footer_hint_holding_a_wide_glyph_is_not_drawn_past_the_rows_edge() {
    let hints = [
        Hint {
            key: "Enter",
            meaning: Meaning::Words("send"),
            when: When::Always,
        },
        Hint {
            key: "日本",
            meaning: Meaning::Words("語"),
            when: When::Always,
        },
    ];

    let narrow = hints_drawn(&hints, 19, "");
    assert_eq!(
        drawn_row(&narrow, 0).trim_end(),
        "Enter send",
        "nineteen columns hold the first hint and neither the gap nor the seven the wide one \
             takes, so it is given up whole rather than drawn over the row's edge"
    );

    let wide = hints_drawn(&hints, 20, "");
    let drawn: String = (pen::cells("Enter send") + pen::cells(GAP)..20)
        .map(|x| {
            wide.cell((x, 0))
                .expect("a cell of the row")
                .symbol()
                .to_owned()
        })
        .filter(|symbol| !symbol.trim().is_empty())
        .collect();
    assert_eq!(
        drawn, "日本語",
        "and where the row does hold it, each wide glyph is drawn in the two columns it \
             occupies and the row ends at its last one"
    );
}

#[test]
fn a_hint_the_row_has_no_room_for_is_not_drawn_at_all() {
    // Narrower than any frame a footer is drawn at, the row's width is the whole of the cut: a
    // hint is drawn whole or not at all, and the gap that would have introduced it goes with it.
    assert_eq!(
        footer(15, 24).trim_end(),
        "Enter send",
        "fifteen columns hold the first hint and neither the gap nor the second"
    );
    assert_eq!(
        footer(8, 24).trim_end(),
        "",
        "eight columns hold no hint at all"
    );
}

/// Whether `one` reads louder than `other`: the frame's own order of light, and the weight a
/// look wears on top of it.
fn louder(one: Look, other: Look) -> bool {
    let rank = |look: Look| {
        let ink = match look.ink() {
            Ink::Plain => 3,
            Ink::Muted => 2,
            Ink::Dim => 1,
            _ => 0,
        };
        ink * 2 + u8::from(look.faces().has(Face::Bold))
    };
    rank(one) > rank(other)
}

/// The runs of row `y` from `from` to `to` as the cells are drawn, each with the look it reads
/// back in.
fn runs_of(buffer: &Buffer, palette: &Palette, y: u16, from: u16, to: u16) -> Vec<(Look, String)> {
    let mut runs: Vec<(Look, String)> = Vec::new();
    for x in from..to {
        let look = look_at(buffer, palette, x, y);
        let glyph = buffer
            .cell((x, y))
            .expect("a cell of the row")
            .symbol()
            .to_owned();
        match runs.last_mut() {
            Some((last, text)) if *last == look => text.push_str(&glyph),
            _ => runs.push((look, glyph)),
        }
    }
    runs
}

#[test]
fn the_footer_draws_each_hint_as_a_key_and_the_words_that_say_what_it_does() {
    let palette = Palette::TRUECOLOR;
    for cols in [120, 72] {
        for mut view in [
            View::of(
                Capabilities::default(),
                Facts::default().asked(),
                Transcript::none(),
            ),
            asking(0),
            asking(3),
        ] {
            let area = screen(cols, 24);
            let mut buffer = Buffer::empty(area);
            let told = view.paint(area, &mut buffer);

            let row = laid_out(&told).footer.y;
            let drawn = drawn_row(&buffer, row);
            let depth = depth(&view);
            let mut tail = 0;
            for hint in kept(&view.hints(), cols, &depth) {
                let at = tail + drawn[tail..].find(hint.key).expect("a kept key is drawn");
                let x = pen::cells(&drawn[..at]);
                let key_look = look_at(&buffer, &palette, x, row);
                let does_look = look_at(&buffer, &palette, x + pen::cells(hint.key) + 1, row);
                assert_eq!(
                    key_look,
                    keys::KEY,
                    "at {cols} columns the key {:?} is not drawn in the look a key is drawn in",
                    hint.key
                );
                assert_eq!(
                    does_look,
                    keys::MEANING,
                    "at {cols} columns the words {:?} are not drawn in the look a meaning is",
                    hint.words(&depth)
                );
                tail = at + hint.key.len();
            }
        }
    }
}

#[test]
fn the_card_that_lists_the_keys_draws_every_key_no_quieter_than_the_line_that_explains_it() {
    let palette = Palette::TRUECOLOR;
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::none(),
    );
    view.key(Key::Char('?'));
    let (buffer, told) = painted_and_told(&mut view, 120, 40);

    let body = card_body(&told);
    let mut listed = 0;
    for y in body.y..body.bottom() {
        let runs: Vec<(Look, String)> = runs_of(&buffer, &palette, y, body.x, body.right())
            .into_iter()
            .filter(|(_, text)| !text.trim().is_empty())
            .collect();
        match runs.as_slice() {
            [] | [_] => {}
            [(key, _), (meaning, _), ..] => {
                listed += 1;
                assert_eq!(*key, keys::KEY, "row {y} draws its key in another look");
                assert_eq!(
                    *meaning,
                    keys::MEANING,
                    "row {y} draws the line that explains its key in another look"
                );
            }
        }
    }
    assert_eq!(
        listed,
        Mode::Composer.table().len() + Mode::Keys.table().len(),
        "every key of the stack the card sits on is one row of its list"
    );
}

/// A frame whose transcript is `count` rows of turn dividers, each saying which turn it is:
/// where a row stands in the frame is then a fact of what the row says.
fn turns(count: u32) -> View {
    View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::of((1..=count).map(|turn| Entry::Turn { turn })),
    )
}

/// The same transcript on a terminal that reports a pointer: what a notch moves.
fn turns_taken(count: u32) -> View {
    View::of(
        pointing(),
        Facts::default().asked(),
        Transcript::of((1..=count).map(|turn| Entry::Turn { turn })),
    )
}

/// A frame whose transcript is one turn and a `cargo test` run of `rows` rows, which is a
/// result longer than its fold.
fn folded_run(rows: u32) -> View {
    let output: String = (1..=rows)
        .map(|line| format!("test rows::a_row_of_output_{line} ... ok"))
        .collect::<Vec<String>>()
        .join("\n");
    View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::of(vec![
            Entry::Turn { turn: 1 },
            Entry::Call {
                turn: 1,
                id: None,
                name: SecretSet::default().redact("bash"),
                arguments: SecretSet::default()
                    .redact(r#"{"command": "cargo test -p sindri --test rows"}"#),
            },
            Entry::Result {
                turn: 1,
                id: None,
                name: SecretSet::default().redact("bash"),
                text: SecretSet::default().redact(&output),
                bytes: 640,
                elapsed_ms: 1_800,
                exit: Some(Exit::Code(0)),
            },
        ]),
    )
}

/// Every row the frame draws, top to bottom.
fn frame_rows(buffer: &Buffer) -> Vec<String> {
    let area = buffer.area;
    (area.y..area.bottom())
        .map(|y| drawn_row(buffer, y))
        .collect()
}

#[test]
fn one_key_opens_the_newest_results_fold_and_the_same_key_closes_it_again() {
    let mut view = folded_run(20);
    let folded = painted(&mut view, 120, 40);

    assert!(
        holds(&folded, "16 rows kept back"),
        "the newest result is longer than its fold, so the band says how many rows it keeps \
             back: {:?}",
        frame_rows(&folded)
    );
    assert!(
        !holds(&folded, "a_row_of_output_12"),
        "and the rows it keeps back are the rows it does not draw: the head stops before \
             them and only the last row is drawn"
    );

    assert_eq!(
        view.key(Key::Fold),
        Outcome::Edited,
        "Ctrl-O opens the fold of the newest result, which is a frame to paint"
    );
    let open = painted(&mut view, 120, 40);
    assert!(
        holds(&open, "a_row_of_output_20"),
        "the opened fold draws every row of the output, its last one included: {:?}",
        frame_rows(&open)
    );
    assert!(
        !holds(&open, "rows kept back"),
        "and an open fold keeps nothing back, so it counts nothing"
    );

    assert_eq!(
        view.key(Key::Fold),
        Outcome::Edited,
        "and the same key folds the newest result again"
    );
    let closed = painted(&mut view, 120, 40);
    assert_eq!(
        frame_rows(&closed),
        frame_rows(&folded),
        "a fold closed by its own key is the frame the reader was handed before"
    );
}

#[test]
fn a_fold_a_reader_opened_stays_open_through_a_scroll_and_a_resize() {
    let mut view = folded_run(60);
    painted(&mut view, 120, 40);
    assert_eq!(
        view.key(Key::Fold),
        Outcome::Edited,
        "the fold the reader opens is the newest result's"
    );
    assert!(
        holds(&painted(&mut view, 120, 40), "a_row_of_output_60"),
        "the fold is open, so the band draws the last row of the output"
    );

    assert_eq!(
        view.key(Key::PageUp),
        Outcome::Edited,
        "an opened run of sixty rows is taller than the band, so a page back moves the frame"
    );
    assert!(
        !view.scroll().is_tail(),
        "which is a frame standing off the tail rather than one that never left it"
    );
    let scrolled = painted(&mut view, 120, 40);
    assert!(
        holds(&scrolled, "a_row_of_output_30"),
        "a scroll is no hand on the fold: a row only an opened one draws is still there: {:?}",
        frame_rows(&scrolled)
    );
    assert!(
        !holds(&scrolled, "rows kept back"),
        "and the scroll counts nothing back on a fold the reader opened"
    );

    let resized = painted(&mut view, 100, 40);
    assert!(
        holds(&resized, "a_row_of_output_30"),
        "a narrower band re-lays the opened result whole, rows past the fold's head alike: \
             {:?}",
        frame_rows(&resized)
    );
    assert!(
        !holds(&resized, "rows kept back"),
        "a resize is no hand on the fold either, so nothing is folded again by one"
    );
}

#[test]
fn a_mark_grown_over_a_row_opens_the_fold_it_belongs_to() {
    let mut view = folded_run(20);
    let folded = painted(&mut view, 120, 40);
    let row = (0..folded.area.bottom())
        .find(|y| drawn_row(&folded, *y).contains("16 rows kept back"))
        .expect("the folded result draws the row that counts what it keeps back");
    assert!(
        !holds(&folded, "a_row_of_output_12"),
        "the result begins folded, so the middle of its output is not drawn"
    );

    view.drag(Cell::new(row, 2), Cell::new(row, 2));
    let marked = painted(&mut view, 120, 40);
    assert!(
        !holds(&marked, "a_row_of_output_12"),
        "a drag is a mark and no key at all, so the fold it covers is drawn as it was"
    );

    assert_eq!(
        view.key(Key::ShiftDown),
        Outcome::Edited,
        "the mark grows a row, and a mark that grows is a frame to paint"
    );
    let grown = painted(&mut view, 120, 40);
    assert!(
        holds(&grown, "a_row_of_output_20"),
        "the row the mark grew over belongs to the folded result, so its fold opens and the \
             output is shown whole: {:?}",
        frame_rows(&grown)
    );
    assert!(
        !holds(&grown, "rows kept back"),
        "and an opened fold counts nothing back"
    );
}

/// The cells a frame `cols` by `rows` is painted into.
fn painted(view: &mut View, cols: u16, rows: u16) -> Buffer {
    let area = screen(cols, rows);
    let mut buffer = Buffer::empty(area);
    view.paint(area, &mut buffer);
    buffer
}

/// The cells a frame `cols` by `rows` is painted into, and what that pass told the terminal.
fn painted_and_told(view: &mut View, cols: u16, rows: u16) -> (Buffer, Painted) {
    let area = screen(cols, rows);
    let mut buffer = Buffer::empty(area);
    let told = view.paint(area, &mut buffer);
    (buffer, told)
}

#[test]
fn a_notch_of_the_wheel_takes_the_transcript_back_exactly_as_the_page_key_does() {
    let mut view = turns_taken(100);
    let (_, told) = painted_and_told(&mut view, 120, 40);

    assert_eq!(
        view.pointer(&told, Pointer::Wheel(Notch::Up)),
        Outcome::Edited,
        "a notch of the wheel moves the transcript, which is a frame to paint"
    );
    assert_eq!(
        view.scroll(),
        Scroll::Back(37),
        "and it moves it by the band the transcript is drawn in, exactly as `PgUp` does"
    );
    let (_, told) = painted_and_told(&mut view, 120, 40);
    assert_eq!(
        view.pointer(&told, Pointer::Wheel(Notch::Down)),
        Outcome::Edited,
        "one notch on brings it back"
    );
    assert_eq!(
        view.scroll(),
        Scroll::Tail,
        "which is the tail it started at"
    );
}

#[test]
fn the_wheel_stops_at_the_first_row_and_at_the_tail() {
    let mut view = turns_taken(40);
    let (_, told) = painted_and_told(&mut view, 120, 40);

    assert_eq!(
        view.pointer(&told, Pointer::Wheel(Notch::Up)),
        Outcome::Edited,
        "a transcript three rows taller than its band is a notch back"
    );
    assert_eq!(
        view.scroll(),
        Scroll::Back(3),
        "but only as far as its first row, which one page reaches"
    );
    assert_eq!(
        view.pointer(&told, Pointer::Wheel(Notch::Up)),
        Outcome::Unchanged,
        "and a notch up already at the first row moves nothing"
    );
    assert_eq!(view.scroll(), Scroll::Back(3), "the row it stopped on");

    let mut tail = turns_taken(40);
    let (_, told) = painted_and_told(&mut tail, 120, 40);
    assert_eq!(
        tail.pointer(&told, Pointer::Wheel(Notch::Down)),
        Outcome::Unchanged,
        "a notch down at the tail is a frame that does not move"
    );
    assert_eq!(tail.scroll(), Scroll::Tail);
}

#[test]
fn a_notch_on_a_frame_whose_terminal_reports_no_pointer_moves_nothing() {
    let mut view = turns(100);
    let (_, told) = painted_and_told(&mut view, 120, 40);

    assert_eq!(
        view.pointer(&told, Pointer::Wheel(Notch::Up)),
        Outcome::Unchanged,
        "a terminal that reports no pointer is handed a frame that takes none"
    );
    assert_eq!(
        view.scroll(),
        Scroll::Tail,
        "which is at the tail it was drawn at"
    );
}

/// A frame with the `@` picker open over a workspace of two files, painted at 120 by 40 with
/// the pointer taken: what a press on one of its rows is read against.
fn picking() -> (View, tempfile::TempDir) {
    let workspace = tempfile::tempdir().expect("a temp workspace");
    for file in ["alpha.rs", "zeta.rs"] {
        std::fs::write(workspace.path().join(file), "").expect("a file");
    }
    let mut view = View::of(
        pointing(),
        Facts::default()
            .asked()
            .working(workspace.path().to_path_buf(), None),
        Transcript::none(),
    );
    assert_eq!(
        view.key(Key::Char('@')),
        Outcome::Edited,
        "`@` at the start of the draft opens the picker"
    );
    (view, workspace)
}

/// The row a frame draws `said` on, so a press can land where the words are.
fn row_holding(buffer: &Buffer, said: &str) -> u16 {
    (buffer.area.y..buffer.area.bottom())
        .find(|y| drawn_row(buffer, *y).contains(said))
        .unwrap_or_else(|| panic!("the frame draws {said:?} on no row of it"))
}

#[test]
fn a_press_on_a_file_row_of_the_picker_chooses_that_file() {
    let (mut view, _workspace) = picking();
    let (drawn, told) = painted_and_told(&mut view, 120, 40);
    let row = row_holding(&drawn, "zeta.rs");

    assert_eq!(
        view.pointer(&told, Pointer::Press(Cell::new(row, 40))),
        Outcome::Edited,
        "a press on the second row of the picker chooses that file, and a frame is painted"
    );
    assert_eq!(
        view.composer().text(),
        "zeta.rs ",
        "the draft holds that file's word, which is what `Enter` on the row inserts"
    );
    assert_ne!(
        view.mode(),
        Mode::Files,
        "and choosing a file closes the card it was chosen from"
    );
}

#[test]
fn a_press_on_no_row_of_the_picker_changes_nothing() {
    let (mut view, _workspace) = picking();
    let (drawn, told) = painted_and_told(&mut view, 120, 40);
    let file = row_holding(&drawn, "zeta.rs");
    let query = row_holding(&drawn, "query:");
    let counted = row_holding(&drawn, " of 2");

    for (at, why) in [
        (Cell::new(query, 40), "the row the query is typed on"),
        (Cell::new(counted, 40), "the row that counts the matches"),
        (Cell::new(file, 5), "a cell outside the card"),
    ] {
        assert_eq!(
            view.pointer(&told, Pointer::Press(at)),
            Outcome::Unchanged,
            "a press on {why} chooses no row, so nothing happens"
        );
        assert_eq!(
            view.composer().text(),
            "",
            "and the draft is the one it was"
        );
        assert_eq!(view.mode(), Mode::Files, "with the card still open");
    }
}

#[test]
fn a_press_on_a_commands_row_sends_what_enter_sends() {
    let mut view = View::of(pointing(), Facts::default().asked(), Transcript::none());
    view.key(Key::Char('/'));
    let (drawn, told) = painted_and_told(&mut view, 120, 40);
    let row = row_holding(&drawn, "/quit");

    assert_eq!(
        view.pointer(&told, Pointer::Press(Cell::new(row, 40))),
        Outcome::Quit,
        "the row the press lands on is the one `/quit` hangs from, and it sends what `Enter` \
             on that line sends"
    );
}

#[test]
fn a_press_on_a_commands_row_sends_what_enter_on_the_typed_line_sends() {
    let mut clicked = View::of(pointing(), Facts::default().asked(), Transcript::none());
    for glyph in ['/', 'n'] {
        clicked.key(Key::Char(glyph));
    }
    let (drawn, told) = painted_and_told(&mut clicked, 120, 40);
    let row = row_holding(&drawn, "/settings");

    let mut typed = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::none(),
    );
    for glyph in "/settings".chars() {
        typed.key(Key::Char(glyph));
    }

    assert_eq!(
        clicked.pointer(&told, Pointer::Press(Cell::new(row, 40))),
        typed.key(Key::Enter),
        "a press on a row sends the line that row spells, and sends what `Enter` sends"
    );
    assert_eq!(
        clicked.composer().text(),
        typed.composer().text(),
        "which leaves the frame exactly as the typed line left it"
    );
}

#[test]
fn a_press_on_a_commands_row_marks_it_and_enter_takes_the_marked_row() {
    let mut clicked = View::of(pointing(), Facts::default().asked(), Transcript::none());
    clicked.key(Key::Char('/'));
    let (drawn, told) = painted_and_told(&mut clicked, 120, 40);
    let row = row_holding(&drawn, "/new");
    let at = told
        .press(Cell::new(row, 40))
        .expect("the row the card draws `/new` on hangs a choice");

    let mut typed = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::none(),
    );
    typed.paste("/new");

    assert!(
        clicked.chosen(at),
        "a press chooses the row of the card it landed on"
    );
    assert_eq!(
        clicked.card().map(commands::Card::spelling),
        Some(String::from("/new")),
        "and the mark stands on that row rather than on the `/help` a bare word names"
    );
    assert_eq!(
        clicked.key(Key::Enter),
        typed.key(Key::Enter),
        "so `Enter` takes the marked row and answers what the typed line `/new` answers"
    );
    assert_eq!(
        clicked.mode(),
        typed.mode(),
        "and leaves the frame where it did"
    );
}

#[test]
fn a_page_up_takes_the_transcript_back_by_the_band_it_is_drawn_in() {
    let mut view = turns(100);
    painted(&mut view, 120, 40);

    assert_eq!(
        view.scroll(),
        Scroll::Tail,
        "a frame painted once is at the tail"
    );
    assert_eq!(
        view.key(Key::PageUp),
        Outcome::Edited,
        "a page up is a frame to paint"
    );
    assert_eq!(
        view.scroll(),
        Scroll::Back(37),
        "a 120x40 frame gives the transcript thirty-seven rows — forty less the footer \
             and the composer's two — so one page moves it that far back"
    );
    painted(&mut view, 120, 40);
    assert_eq!(
        view.key(Key::PageDown),
        Outcome::Edited,
        "and a page down brings it back"
    );
    assert_eq!(
        view.scroll(),
        Scroll::Tail,
        "one page on from one page back is the tail the frame started at, though the row \
             saying where it stands was drawn out of that same band"
    );
}

/// A frame painted at the tail and then taken a page back, which is the state every return to
/// the tail is asked to leave.
fn scrolled_back() -> View {
    let mut view = turns(100);
    painted(&mut view, 120, 40);
    view.key(Key::PageUp);
    assert_eq!(
        view.scroll(),
        Scroll::Back(37),
        "the frame under these tests is one page off the tail to begin with"
    );
    view
}

/// The one row a frame draws saying the transcript is scrolled back, and the row it stands on.
fn notice_of(buffer: &Buffer) -> (String, u16) {
    let area = buffer.area;
    for y in area.y..area.bottom() {
        let row = drawn_row(buffer, y);
        if row.contains("rows back") || row.contains("1 row back") {
            return (row, y);
        }
    }
    panic!(
        "no row of the frame says how many rows back it stands: {:?}",
        (area.y..area.bottom())
            .map(|y| drawn_row(buffer, y))
            .collect::<Vec<_>>()
    );
}

#[test]
fn a_frame_off_the_tail_says_how_far_back_it_stands_and_the_key_that_follows_the_run() {
    let palette = Palette::TRUECOLOR;
    let mut view = scrolled_back();
    let buffer = painted(&mut view, 120, 40);

    let (drawn, y) = notice_of(&buffer);
    assert_eq!(
        drawn.trim_end(),
        "37 rows back \u{b7} End follows the run",
        "the row says how many rows back the frame stands and the key that returns it"
    );
    assert_eq!(
        y, 36,
        "the row stands above the composer's edge row, which is the thirty-seventh of \
             forty, rather than in the transcript it is a fact about"
    );

    let back = drawn.find("rows back").expect("the count");
    assert_eq!(
        look_at(&buffer, &palette, pen::cells(&drawn[..back]), y),
        keys::MEANING,
        "the count is drawn in the look a meaning hangs in"
    );
    let end = drawn.find("End").expect("the key");
    let column = pen::cells(&drawn[..end]);
    assert_eq!(
        look_at(&buffer, &palette, column, y),
        keys::KEY,
        "the key that follows the run again is drawn in the look a key is drawn in"
    );
    assert_eq!(
        look_at(&buffer, &palette, column - pen::cells(GAP), y),
        Look::plain(Ink::Dim),
        "and the gap between them is dimmer than either, so the row reads as one sentence"
    );
}

#[test]
fn one_row_back_is_said_in_the_singular() {
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::of((1..=100).map(|turn| Entry::Turn { turn })).scrolled(Scroll::Back(1)),
    );
    let buffer = painted(&mut view, 120, 40);

    let (drawn, _) = notice_of(&buffer);
    assert_eq!(
        drawn.trim_end(),
        "1 row back \u{b7} End follows the run",
        "one row is a count of one row, and not of one rows"
    );
}

#[test]
fn a_frame_at_the_tail_draws_no_row_saying_it_is_scrolled_back() {
    let mut view = turns(100);
    let buffer = painted(&mut view, 120, 40);

    assert_eq!(
        view.scroll(),
        Scroll::Tail,
        "the frame these cells were painted from stands at the tail"
    );
    assert!(
        !holds(&buffer, "rows back") && !holds(&buffer, "follows the run"),
        "a frame at the tail has nothing to say about where it stands: {:?}",
        (buffer.area.y..buffer.area.bottom())
            .map(|y| drawn_row(&buffer, y))
            .collect::<Vec<_>>()
    );
}

#[test]
fn a_row_that_lands_while_the_reader_is_scrolled_back_holds_the_window_still() {
    let mut view = turns(100);
    view.text("a first word of an answer");
    painted(&mut view, 120, 40);
    view.key(Key::PageUp);
    let before = painted(&mut view, 120, 40);

    view.text(&pen::filled(200, 'x'));
    let after = painted(&mut view, 120, 40);

    assert_eq!(
        drawn_row(&after, 0),
        drawn_row(&before, 0),
        "a row that lands at the tail must not roll the window forward under a reader who \
             is reading back"
    );
    let (now, _) = notice_of(&after);
    assert_eq!(
        now.trim_end(),
        "39 rows back \u{b7} End follows the run",
        "the two rows the answer landed are added to the count of rows back: the frame \
             holds the rows it was reading rather than sliding with the tail"
    );
}

#[test]
fn a_band_that_lays_the_history_out_again_does_not_move_the_reader_off_their_rows() {
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::of((1..=20).map(|turn| Entry::Assistant {
            turn,
            text: SecretSet::default().redact(pen::filled(300, 'w')),
        })),
    );
    painted(&mut view, 120, 40);
    view.key(Key::PageUp);
    let back = view.scroll();
    assert_ne!(
        back,
        Scroll::Tail,
        "the frame stands off the tail to begin with"
    );

    painted(&mut view, 80, 40);

    assert_eq!(
        view.scroll(),
        back,
        "a narrower band wraps the same history into more rows, and a reader's window is \
             not thrown forward by their own resize"
    );
}

#[test]
fn a_row_too_narrow_for_the_words_that_say_it_is_drawn_whole_or_not_at_all() {
    let mut view = turns(100);
    painted(&mut view, 120, 40);
    view.key(Key::PageUp);

    let narrow = painted(&mut view, 20, 40);
    assert!(
        !holds(&narrow, "rows back") && !holds(&narrow, "End follows"),
        "a twenty-column row holds neither the count nor the key whole, so it holds \
             neither of them at all"
    );

    let roomy = painted(&mut view, 40, 40);
    assert!(
        holds(&roomy, "37 rows back \u{b7} End follows the run"),
        "and forty columns hold the whole row"
    );
}

#[test]
fn a_frame_too_short_for_the_row_gives_it_up_before_the_transcript_loses_its_last_row() {
    let mut view = turns(100);
    painted(&mut view, 120, 40);
    view.key(Key::PageUp);

    let close = painted(&mut view, 120, 4);
    assert!(
        !holds(&close, "rows back"),
        "a frame of four rows holds the composer's two, so the row that says where the \
             reader stands is given up and the transcript keeps its row: {:?}",
        (close.area.y..close.area.bottom())
            .map(|y| drawn_row(&close, y))
            .collect::<Vec<_>>()
    );
    assert!(
        drawn_row(&close, 0).starts_with("─ turn "),
        "and the transcript's one row is still drawn in the row above the composer: {:?}",
        drawn_row(&close, 0)
    );
}

#[test]
fn a_new_entry_leaves_the_frame_at_the_tail() {
    let mut view = scrolled_back();

    view.text("the first word of an answer");

    assert_eq!(
        view.scroll(),
        Scroll::Tail,
        "an answer that opens its row puts the frame back at the tail without being asked"
    );
}

#[test]
fn an_answer_that_opens_its_row_under_a_thought_leaves_the_frame_at_the_tail() {
    let mut view = turns(100);
    view.reasoning("a thought the run says first");
    painted(&mut view, 120, 40);
    view.key(Key::PageUp);
    assert_ne!(
        view.scroll(),
        Scroll::Tail,
        "the frame under this test stands off the tail"
    );

    view.text("and the answer it introduces");

    assert_eq!(
        view.scroll(),
        Scroll::Tail,
        "an answer opens a row of its own under the thought, so it is a new entry as the \
             run's first word is"
    );
}

#[test]
fn a_thought_that_opens_its_row_leaves_the_frame_at_the_tail() {
    let mut view = scrolled_back();

    view.reasoning("the first word of a thought");

    assert_eq!(
        view.scroll(),
        Scroll::Tail,
        "the run's first reasoning is a new entry as its answer is"
    );
}

#[test]
fn a_sent_prompt_leaves_the_frame_at_the_tail() {
    let mut view = scrolled_back();

    view.settle();

    assert_eq!(
        view.scroll(),
        Scroll::Tail,
        "a prompt the reader sends is a run they mean to watch, so the frame is handed \
             back to its tail"
    );
}

#[test]
fn a_settled_run_leaves_the_frame_at_the_tail() {
    let mut view = scrolled_back();

    view.stopped(&Stop::Final);

    assert_eq!(
        view.scroll(),
        Scroll::Tail,
        "a run that has settled puts the frame back at the tail"
    );
}

#[test]
fn a_page_up_at_the_first_row_stops_there_rather_than_wrapping_to_the_tail() {
    let mut view = turns(40);
    painted(&mut view, 120, 40);

    assert_eq!(
        view.scroll(),
        Scroll::Tail,
        "the frame is painted once and stands at the tail"
    );
    assert_eq!(
        view.key(Key::PageUp),
        Outcome::Edited,
        "one page back from the tail moves the frame as far as the transcript reaches"
    );
    assert_eq!(
        view.scroll(),
        Scroll::Back(3),
        "forty rows of transcript in a band of thirty-seven leaves three rows behind the \
             frame, so the one page back is three rows and not thirty-seven"
    );
    painted(&mut view, 120, 40);
    assert_eq!(
        view.key(Key::PageUp),
        Outcome::Edited,
        "the row saying where the frame stands is drawn out of that band, so reaching the \
             first row is the second press rather than the first"
    );
    assert_eq!(view.scroll(), Scroll::Back(4));
    let head = painted(&mut view, 120, 40);
    let first = drawn_row(&head, 0);
    assert!(
        first.starts_with(&format!("{THUMB} turn 1 ")),
        "the transcript's first row stands at the top of the band with the thumb in the cell \
             the rule's corner gave up and the turn's words past it, so a reader who paged back \
             sees where they stand over the turn they stand on: {first:?}"
    );
    for _ in 0..3 {
        assert_eq!(
            view.key(Key::PageUp),
            Outcome::Unchanged,
            "a press at the first row has nowhere to go, so it repaints nothing"
        );
        assert_eq!(
            view.scroll(),
            Scroll::Back(4),
            "and the frame stays on the first row rather than rolling round to the tail"
        );
    }
}

#[test]
fn a_page_down_at_the_tail_stops_there_rather_than_wrapping_to_the_first_row() {
    let mut view = turns(40);
    painted(&mut view, 120, 40);

    assert_eq!(
        view.key(Key::PageDown),
        Outcome::Unchanged,
        "a frame at the tail has no row on to move to"
    );
    assert_eq!(view.scroll(), Scroll::Tail);
}

#[test]
fn end_returns_the_frame_to_the_tail_and_the_caret_to_its_line_at_it() {
    let mut view = turns(100);
    painted(&mut view, 120, 40);
    view.key(Key::PageUp);

    assert_eq!(
        view.key(Key::End),
        Outcome::Edited,
        "off the tail, `End` is the tail of the run and a frame to paint"
    );
    assert_eq!(view.scroll(), Scroll::Tail);

    view.paste("a line of my own");
    view.key(Key::Home);
    assert_eq!(
        view.key(Key::End),
        Outcome::Edited,
        "at the tail the same key is the draft's own end of line"
    );
    assert_eq!(
        view.composer().caret(),
        (0, 16),
        "so the caret waits after the last glyph of the draft rather than moving the \
             frame it is already at the tail of"
    );
}

#[test]
fn a_ctrl_u_and_a_ctrl_d_are_the_same_two_pages() {
    let mut view = turns(100);
    painted(&mut view, 120, 40);
    view.key(Key::PageUp);
    let paged = view.scroll();

    view.key(Key::PageDown);
    assert_eq!(view.scroll(), Scroll::Tail);
    assert_eq!(
        view.key(Key::Clear),
        Outcome::Edited,
        "`Ctrl-U` is a page back as `PgUp` is"
    );
    assert_eq!(
        view.scroll(),
        paged,
        "the two spellings of one page are one act, so they move the frame by the same rows"
    );
    assert_eq!(view.key(Key::Forward), Outcome::Edited);
    assert_eq!(
        view.scroll(),
        Scroll::Tail,
        "and `Ctrl-D` brings it on as `PgDn` does"
    );
}

#[test]
fn the_card_that_lists_the_keys_names_the_two_pages_and_the_tail() {
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::none(),
    );
    view.key(Key::Char('?'));
    let (buffer, told) = painted_and_told(&mut view, 120, 40);
    let held = card_body(&told);
    let body = (held.y..held.bottom())
        .map(|y| drawn_row(&buffer, y))
        .collect::<Vec<_>>()
        .join("\n");

    for said in [
        "PgUp",
        "the transcript a band back",
        "Ctrl-U",
        "PgDn",
        "the transcript a band on",
        "Ctrl-D",
        "End",
        "the tail of the run, or the end of the line",
    ] {
        assert!(
            body.contains(said),
            "the card that lists the keys does not name {said:?}: the keys the frame \
                 answers are named where keys are named"
        );
    }
}

#[test]
fn a_key_is_drawn_louder_than_the_words_that_say_what_pressing_it_does() {
    assert!(
        louder(keys::KEY, keys::MEANING),
        "a key is drawn in {:?} and its meaning in {:?}: a key is the thing to press, so it is \
             never the quieter of the two things a hint says",
        keys::KEY,
        keys::MEANING
    );
}

#[test]
fn every_hint_the_table_writes_is_drawn_in_the_frame_that_honours_it() {
    let mut drawn: Vec<&'static str> = Vec::new();
    for view in [
        View::of(
            Capabilities::default(),
            Facts::default().asked(),
            Transcript::none(),
        ),
        asking(0),
        asking(3),
    ] {
        let depth = depth(&view);
        for width in 20..=200u16 {
            for hint in kept(&view.hints(), width, &depth) {
                drawn.push(hint.key);
            }
        }
    }
    for hint in HINTS {
        assert!(
            drawn.contains(&hint.key),
            "no frame between twenty and two hundred columns draws {:?}, so it is a key the \
                 table writes down and no reader is ever shown",
            hint.key
        );
    }
}

#[test]
fn every_hint_names_a_key_the_composer_answers_or_a_glyph_that_opens_the_commands_card() {
    for hint in HINTS {
        let bound = Mode::Composer
            .table()
            .iter()
            .any(|binding| binding.name.split(", ").any(|name| name == hint.key));
        let opens = match hint.key.chars().next() {
            Some(glyph) if pen::cells(hint.key) == 1 => {
                let mut view = View::of(
                    Capabilities::default(),
                    Facts::default().asked(),
                    Transcript::none(),
                );
                view.key(Key::Char(glyph));
                view.card().is_some() && view.composer().text() == hint.key
            }
            _ => false,
        };
        assert!(
            bound || opens,
            "the footer names {:?} and nothing answers to it: a hint is drawn in a row only \
                 where it is a key that works, or a glyph the draft takes into a card",
            hint.key
        );
    }
}

#[test]
fn the_terminal_cursor_lands_on_the_carets_row_of_the_draft() {
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::none(),
    );
    view.paste("first line\nsecond line");
    let area = screen(40, 12);
    let mut buffer = Buffer::empty(area);
    let painted = view.paint(area, &mut buffer);

    assert_eq!(view.composer().rows(), 3);
    assert_eq!(
        view.cursor(&painted),
        Some((13, 10)),
        "the caret's column is the one it takes in the draft, and the draft is set two \
             columns in"
    );
}

/// The row holding `words`, and the column of its first glyph: what the frame drew, read back
/// from the cells rather than from the constants the frame was drawn from.
fn place_of(buffer: &Buffer, words: &str) -> (String, u16) {
    let area = buffer.area;
    for y in area.y..area.bottom() {
        let row = drawn_row(buffer, y);
        if row.contains(words) {
            let at = row.find(words).expect("the row the words begin on");
            let column = pen::cells(&row[..at]);
            return (row, column);
        }
    }
    panic!("no row of the frame draws {words:?}");
}

#[test]
fn the_draft_is_set_in_the_column_an_answer_is_set_in_behind_a_sigil() {
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::of(vec![Entry::Assistant {
            turn: 1,
            text: SecretSet::default().redact("an answer opens here"),
        }]),
    );
    view.paste("a draft opens here");

    let area = screen(120, 40);
    let mut buffer = Buffer::empty(area);
    view.paint(area, &mut buffer);

    let (answered, answer_column) = place_of(&buffer, "an answer opens here");
    let (drafted, draft_column) = place_of(&buffer, "a draft opens here");
    assert_eq!(
        draft_column, answer_column,
        "the answer is set from column {answer_column} ({answered:?}) and the draft from \
             {draft_column} ({drafted:?}), so the reader's own words do not line up with the \
             run's"
    );
    let gutter: String = drafted.chars().take(usize::from(draft_column)).collect();
    assert_eq!(
        gutter, "\u{203a} ",
        "the columns before the draft are not the sigil that opens it in the gutter"
    );
}

/// The frame the picture fixtures draw: an answer naming a diagram outside the workspace and a
/// tall field the workspace's own svg draws, read from the tail a long way back.
fn field(rows: u16) -> Buffer {
    let mut view = View::of(
        pictured(),
        Facts::default().asked().working(
            PathBuf::from(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/pictures"
            )),
            None,
        ),
        Transcript::of(vec![Entry::Assistant {
            turn: 1,
            text: SecretSet::default().redact(
                "![a diagram outside the workspace](../outside-the-workspace.svg)\n\n\
                     ![a tall field](tall.svg)",
            ),
        }])
        .scrolled(Scroll::Back(70)),
    );
    painted(&mut view, 120, rows)
}

/// The first row of `buffer` between `from` and `to` a picture is placed over, if one is.
fn placed_row(buffer: &Buffer, from: u16, to: u16) -> Option<u16> {
    (from..to).find(|y| {
        (0..buffer.area.width).any(|x| placed(buffer.cell((x, *y)).expect("a cell of the frame")))
    })
}

#[test]
fn a_row_a_picture_is_placed_over_keeps_the_cell_that_places_it() {
    // A band this tall places the hundred-row field whole, where a fixture's frame refuses it.
    let rows = 107;
    let buffer = field(rows);

    let over =
        placed_row(&buffer, 0, rows).expect("a band this tall places the tall field over a row");
    assert!(
        placed(buffer.cell((0, over)).expect("a cell of the picture's row")),
        "row {over} of the band carries the bytes that place the picture in its first cell, \
             and a cell of the gutter was written over them: the picture that row stands on is \
             cut, and the reader is handed a thumb where the tall field should be"
    );
    assert!(
        drawn_row(&buffer, 0).starts_with(THUMB),
        "row 0 stands beside no picture and carries the thumb, which is where a reader off \
             the tail reads where they stand: {:?}",
        drawn_row(&buffer, 0)
    );
}

/// The frame the `picture` fixture draws at 120x40 places nothing: the tall field's block is
/// taller than the band, so not one byte of it reaches a cell.
#[test]
fn the_thumb_keeps_its_cell_on_a_row_whose_picture_was_refused() {
    let buffer = field(40);

    assert_eq!(
        placed_row(&buffer, 0, 40),
        None,
        "the tall field's block is taller than the band, so it is placed over no cell at all"
    );
    let over = (0..40)
        .find(|y| drawn_row(&buffer, *y).contains("a tall field"))
        .expect("the tall field's caption stands on a row of the band");
    let row = drawn_row(&buffer, over);
    assert!(
        row.starts_with(THUMB),
        "row {over} stands on the block the tall field was refused over and carries no byte \
             of it: giving its first cell up leaves the reader a hole in the thumb for a picture \
             that is not there, where every other row of the thumb says where they stand"
    );
}

/// The frame rows whose first column is the thumb.
fn thumb_rows(buffer: &Buffer) -> Vec<u16> {
    (buffer.area.y..buffer.area.bottom())
        .filter(|y| drawn_row(buffer, *y).starts_with(THUMB))
        .collect()
}

/// A frame whose transcript is `count` rows of turn dividers, read from `scroll`.
fn scrolled(count: u32, scroll: Scroll) -> View {
    View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::of((1..=count).map(|turn| Entry::Turn { turn })).scrolled(scroll),
    )
}

#[test]
fn a_frame_off_the_tail_draws_the_thumb_in_the_gutter_for_the_rows_the_geometry_says() {
    let at_tail = painted(&mut turns(80), 120, 40);
    assert_eq!(
        thumb_rows(&at_tail),
        Vec::<u16>::new(),
        "a frame at the tail draws no thumb: the reader follows the newest rows, and a \
             thumb there would sit on them"
    );

    let head = painted(&mut scrolled(80, Scroll::Back(44)), 120, 40);
    assert_eq!(
        thumb_rows(&head),
        (0..16).collect::<Vec<u16>>(),
        "forty frame rows less the footer's one, the composer's two and the row saying where \
             the reader stands leave thirty-six, of which an eighty-row transcript fills sixteen: \
             the thumb opens the band at the head"
    );
    assert_eq!(
        look_at(&head, &Palette::TRUECOLOR, 0, 0).ink(),
        Ink::Dim,
        "the thumb is a dim cell of the gutter, and not a word's ink"
    );
}

#[test]
fn the_thumb_follows_the_window_down_the_frame() {
    let middle = painted(&mut scrolled(80, Scroll::Back(22)), 120, 40);
    assert_eq!(
        thumb_rows(&middle),
        (10..26).collect::<Vec<u16>>(),
        "half way from the head to the tail stands the thumb half way down the twenty rows \
             the band leaves it"
    );

    let foot = painted(&mut scrolled(80, Scroll::Back(1)), 120, 40);
    assert_eq!(
        thumb_rows(&foot),
        (20..36).collect::<Vec<u16>>(),
        "one row off the tail stands the thumb at the foot of the band, so the reader sees it \
             arrive there"
    );
}

#[test]
fn the_thumb_takes_no_column_the_words_were_drawn_in() {
    let at_tail = painted(&mut turns(80), 120, 40);
    let off = painted(&mut scrolled(80, Scroll::Back(7)), 120, 40);
    let words =
        |buffer: &Buffer, y: u16| -> String { drawn_row(buffer, y).chars().skip(2).collect() };

    let beside = thumb_rows(&off);
    assert!(
        !beside.is_empty(),
        "the frame off the tail drew no thumb, so nothing is proven about the words it stands \
             beside"
    );
    for y in beside {
        assert!(
            (0..40).any(|other| words(&at_tail, other) == words(&off, y)),
            "the row the thumb stands beside, row {y}, reads as no row of this transcript at \
                 the tail: the cell past the gutter was written over a word"
        );
    }
}

#[test]
fn a_frame_too_narrow_for_a_gutter_draws_no_thumb() {
    let narrow = painted(&mut scrolled(80, Scroll::Back(44)), 2, 40);
    assert_eq!(
        thumb_rows(&narrow),
        Vec::<u16>::new(),
        "a band too narrow for a gutter keeps its words and gives the thumb up"
    );
    assert!(
        drawn_row(&narrow, 0).starts_with("tu"),
        "the band of two columns holds no gutter and sets the turn's own words in its first \
             column, which the thumb has left alone: {:?}",
        drawn_row(&narrow, 0)
    );
}

/// A frame whose first entry is a thought of one wide glyph, which a band this narrow sets in
/// its first column because the gutter would leave it no room.
fn wide(cols: u16) -> Buffer {
    let mut entries = vec![Entry::Reasoning {
        turn: 1,
        text: SecretSet::default().redact("\u{8003}"),
    }];
    entries.extend((1..=79).map(|turn| Entry::Turn { turn }));
    // A window taken further back than the transcript reaches lands on its first row.
    painted(
        &mut View::of(
            Capabilities::default(),
            Facts::default().asked(),
            Transcript::of(entries).scrolled(Scroll::Back(u32::MAX)),
        ),
        cols,
        40,
    )
}

#[test]
fn a_band_that_leaves_a_row_no_gutter_keeps_its_word_and_gives_up_the_thumb() {
    let buffer = wide(3);

    assert_eq!(
        thumb_rows(&buffer),
        Vec::<u16>::new(),
        "three columns leave a wide glyph no room beside the gutter, so the row gives the \
             gutter up and the thumb has no column of its own to stand in"
    );
    let said: Vec<String> = (0..40)
        .map(|y| drawn_row(&buffer, y))
        .filter(|row| row.starts_with('\u{8003}'))
        .collect();
    assert_eq!(
        said.len(),
        1,
        "the thought's wide glyph stands in the first column of its own row, which the thumb \
             has left to it rather than writing over: {:?}",
        (0..40)
            .map(|y| drawn_row(&buffer, y))
            .collect::<Vec<String>>()
    );

    let roomy = wide(4);
    assert!(
        !thumb_rows(&roomy).is_empty(),
        "and four columns hold the gutter beside that glyph, so the thumb is drawn again"
    );
}

/// Whether any row of the frame holds `said`.
fn holds(buffer: &Buffer, said: &str) -> bool {
    (buffer.area.y..buffer.area.bottom()).any(|y| drawn_row(buffer, y).contains(said))
}

#[test]
fn the_terminal_that_sizes_text_decides_whether_a_heading_is_drawn_as_the_cells_it_leaves() {
    let area = screen(60, 12);
    for (sizes, said) in [(true, "A   g a t e"), (false, "A gate")] {
        let mut view = View::of(
            match sizes {
                true => sized(),
                false => Capabilities::default(),
            },
            Facts::default().asked(),
            Transcript::of(vec![Entry::Assistant {
                turn: 1,
                text: SecretSet::default().redact("# A gate"),
            }]),
        );
        let mut buffer = Buffer::empty(area);
        view.paint(area, &mut buffer);

        assert!(
            holds(&buffer, said),
            "a frame whose terminal sizes text: {sizes} is not drawn as {said:?}: {:?}",
            (area.y..area.bottom())
                .map(|y| drawn_row(&buffer, y))
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn a_frame_with_a_screen_over_it_draws_its_headings_as_the_words_they_are() {
    let area = screen(100, 30);
    let mut view = View::of(
        sized(),
        Facts::default().asked(),
        Transcript::of(vec![Entry::Assistant {
            turn: 1,
            text: SecretSet::default().redact("# A gate"),
        }]),
    );
    view.key(Key::Char('?'));
    let mut buffer = Buffer::empty(area);
    view.paint(area, &mut buffer);

    assert!(
        holds(&buffer, "A gate"),
        "the frame behind the card is read as the words it holds: {:?}",
        (area.y..area.bottom())
            .map(|y| drawn_row(&buffer, y))
            .collect::<Vec<_>>()
    );
    assert!(
        !holds(&buffer, "A   g a t e"),
        "and no heading of it is drawn large, because a band under a card takes no key"
    );
}

/// A row's glyphs, each wide one taken once although it is drawn in two cells.
fn drawn_glyphs(buffer: &Buffer, y: u16) -> String {
    let area = buffer.area;
    let mut glyphs = String::new();
    let mut x = area.x;
    while x < area.right() {
        let symbol = buffer.cell((x, y)).expect("a cell of the row").symbol();
        glyphs.push_str(symbol);
        let wide = symbol.chars().next().map_or(1, pen::cell);
        x = x.saturating_add(wide.max(1));
    }
    glyphs
}

/// The rows the composer band draws under its edge row, as the frame left them.
fn draft_rows(buffer: &Buffer, band: Rect) -> Vec<String> {
    (band.y.saturating_add(HAIRLINE_ROWS)..band.bottom())
        .map(|y| drawn_glyphs(buffer, y).trim_end().to_owned())
        .collect()
}

/// An idle frame holding `count` waiting prompts behind the draft `draft`, painted at `cols`
/// by `rows`, and the frame it laid out.
fn waiting_behind(
    palette: Palette,
    draft: &str,
    count: usize,
    cols: u16,
    rows: u16,
) -> (Buffer, Layout) {
    let mut view = View::of(
        inked(palette),
        Facts::default().running(),
        Transcript::none(),
    );
    for at in 1..=count {
        view.enqueue(prompt(&format!("prompt {at}")));
    }
    view.paste(draft);
    let (buffer, told) = painted_and_told(&mut view, cols, rows);
    (buffer, laid_out(&told))
}

#[test]
fn a_queue_of_three_is_drawn_under_the_draft_one_prompt_to_a_row_with_the_newest_at_the_foot() {
    let palette = Palette::TRUECOLOR;
    let (buffer, laid) = waiting_behind(palette, "a draft", 3, 120, 24);
    let band = laid.composer;
    let queue = laid.queue;

    assert_eq!(
        draft_rows(&buffer, band),
        [
            "\u{203a} a draft",
            "\u{203a} prompt 1",
            "\u{203a} prompt 2",
            "\u{203a} prompt 3"
        ],
        "three waiting prompts are three rows of the composer's own band, each wearing the \
             sigil the draft wears and the newest at the foot"
    );
    assert_eq!(
        queue.height, 3,
        "and the queue's rows are the band's last three"
    );
    let gutter = pen::cells(INDENT);
    assert_eq!(
        look_at(&buffer, &palette, gutter, queue.bottom() - 1),
        Look::plain(Ink::Dim),
        "a waiting prompt is the dim ink, quieter than the draft above it"
    );
}

#[test]
fn a_queue_longer_than_its_band_draws_the_newest_it_holds_and_the_count_of_the_rest() {
    let palette = Palette::TRUECOLOR;
    let (buffer, laid) = waiting_behind(palette, "a draft", 7, 120, 24);
    let band = laid.composer;
    let queue = laid.queue;

    assert_eq!(
        queue.height,
        crate::queue::MAX_ROWS,
        "the queue is given the four rows it asks for at most"
    );
    assert_eq!(
        draft_rows(&buffer, band),
        [
            "\u{203a} a draft",
            "  4 more queued",
            "\u{203a} prompt 5",
            "\u{203a} prompt 6",
            "\u{203a} prompt 7",
        ],
        "so it draws the newest three and gives its first row to the count of the four the \
             band could not hold"
    );
    let gutter = pen::cells(INDENT);
    assert_eq!(
        look_at(&buffer, &palette, gutter, queue.y),
        Look::plain(Ink::Dim),
        "and the count is drawn in the dim ink, with no sigil of its own"
    );
}

#[test]
fn a_frame_with_nothing_queued_draws_no_queue_row_and_the_transcript_keeps_them_all() {
    let palette = Palette::TRUECOLOR;
    let (buffer, laid) = waiting_behind(palette, "a draft", 0, 120, 24);
    let band = laid.composer;
    let queue = laid.queue;

    assert_eq!(queue.height, 0, "nothing waiting is no queue row at all");
    assert_eq!(
        band.height, 2,
        "so the band is the draft's own rows: its edge and the one row the draft takes"
    );
    assert_eq!(
        draft_rows(&buffer, band),
        ["\u{203a} a draft"],
        "and no row under the draft belongs to a queue"
    );
    assert!(
        !holds(&buffer, "more queued"),
        "a frame with nothing queued says nothing about a queue"
    );

    let mut view = turns(100);
    view.paste("a draft");
    let (buffer, told) = painted_and_told(&mut view, 120, 24);
    let laid = laid_out(&told);
    assert_eq!(
        laid.transcript.bottom(),
        laid.composer.y,
        "the transcript's band runs down to the composer's edge"
    );
    assert!(
        drawn_row(&buffer, laid.composer.y - 1).contains("turn"),
        "and the row above the edge is the transcript's own, drawn by it"
    );
}

#[test]
fn the_transcript_keeps_every_row_the_queue_does_not_need() {
    let palette = Palette::TRUECOLOR;
    let (_, waited) = waiting_behind(palette, "a draft", 3, 120, 24);
    let (_, clear) = waiting_behind(palette, "a draft", 0, 120, 24);

    assert_eq!(
        clear.transcript.height,
        waited.transcript.height + waited.queue.height,
        "the row a queue takes is a row the transcript had, and it is given back the moment \
             nothing waits"
    );
    assert_eq!(
        clear.composer.height,
        waited.composer.height - waited.queue.height,
        "and the queue's rows are drawn in the band the draft had, not over the transcript's"
    );
}

#[test]
fn a_draft_past_the_ceiling_says_how_many_rows_it_keeps_back_and_keeps_the_caret_inside() {
    let palette = Palette::TRUECOLOR;
    let draft: Vec<String> = (1..=9).map(|line| line.to_string()).collect();
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::none(),
    );
    view.paste(&draft.join("\n"));
    let (buffer, told) = painted_and_told(&mut view, 120, 24);
    let band = laid_out(&told).composer;

    assert_eq!(
        draft_rows(&buffer, band),
        ["  6", "  7", "  8", "  9", "  5 rows kept back",],
        "nine lines are drawn four at a time, and the row under them counts the five the band \
             keeps back"
    );
    let gutter = pen::cells(INDENT);
    assert_eq!(
        look_at(&buffer, &palette, gutter, band.bottom() - 1),
        Look::plain(Ink::Dim),
        "the count is the dim ink"
    );
    let (_, y) = view
        .cursor(&told)
        .expect("a caret is drawn for a long draft");
    assert_eq!(
        y,
        band.y + band.height - 2,
        "and the caret waits on the last row the band draws the draft in, not under it"
    );
}

#[test]
fn a_draft_wider_than_the_band_wraps_under_the_gutter_and_never_splits_a_wide_glyph() {
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::none(),
    );
    view.paste(&pen::filled(60, '日'));

    let (buffer, told) = painted_and_told(&mut view, 60, 24);
    let band = laid_out(&told).composer;
    let wide = |count| pen::filled(count, '日');
    assert_eq!(
        draft_rows(&buffer, band),
        [
            format!("\u{203a} {}", wide(29)),
            format!("  {}", wide(29)),
            format!("  {}", wide(2)),
        ],
        "a fifty-eight column room holds twenty-nine wide glyphs, and the sixtieth glyph \
             begins the third row of the draft with the gutter beside it"
    );
    assert_eq!(band.height, 4, "three rows of draft and the band's edge");
}

#[test]
fn an_empty_draft_says_what_the_band_can_do_in_the_dim_ink_until_a_glyph_arrives() {
    let palette = Palette::TRUECOLOR;
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::none(),
    );

    let (buffer, told) = painted_and_told(&mut view, 120, 40);
    let band = laid_out(&told).composer;
    let (empty, _) = place_of(&buffer, PLACEHOLDER);
    let gutter = pen::cells(INDENT);
    assert!(
        empty.ends_with(&pen::filled(120 - gutter - pen::cells(PLACEHOLDER), ' ')),
        "the placeholder {PLACEHOLDER:?} is not the words of the empty band: {empty:?}"
    );
    let column = gutter + pen::cells(PLACEHOLDER) - 1;
    assert_eq!(
        look_at(&buffer, &palette, column, band.y + HAIRLINE_ROWS),
        Look::plain(Ink::Dim),
        "the placeholder is not drawn in the dim ink"
    );

    view.key(Key::Char('a'));
    let buffer = painted(&mut view, 120, 40);
    assert!(
        !drawn_row(&buffer, band.y + HAIRLINE_ROWS).contains(PLACEHOLDER),
        "the placeholder outlived the first glyph the reader typed"
    );
}

#[test]
fn a_card_covers_the_composer_and_leaves_it_readable_and_out_of_reach() {
    let palette = Palette::TRUECOLOR;
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::none(),
    );
    view.paste("a draft of mine");
    view.screen(Mode::Keys);

    let (buffer, told) = painted_and_told(&mut view, 120, 40);
    let band = laid_out(&told).composer;
    let (drawn, column) = place_of(&buffer, "a draft of mine");
    assert_eq!(
        column, 2,
        "the draft under the card is still set in the gutter: {drawn:?}"
    );
    assert_eq!(
        look_at(&buffer, &palette, column, band.y + HAIRLINE_ROWS),
        Look::plain(Ink::Dim),
        "the scrim reads the draft down rather than taking it away: a reader sees the words \
             they will return to"
    );
    assert_eq!(
        view.cursor(&told),
        None,
        "and no caret waits in a band that takes no key"
    );
}

#[test]
fn a_query_wider_than_the_body_is_cut_at_its_edge_and_not_dropped() {
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked().working(std::env::temp_dir(), None),
        Transcript::none(),
    );
    view.key(Key::Char('@'));
    for _ in 0..60 {
        view.key(Key::Char('x'));
    }

    let area = screen(40, 12);
    let mut buffer = Buffer::empty(area);
    view.paint(area, &mut buffer);

    assert_eq!(
        drawn_row(&buffer, 1),
        format!("  query: {}  ", pen::filled(29, 'x')),
        "a forty-column frame gives the card's body thirty-six, so the query is cut at the body's last column rather than dropped or drawn past it"
    );
}

#[test]
fn a_screen_has_no_caret_for_the_terminal_to_put_anywhere() {
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::none(),
    );
    let area = screen(40, 12);
    let mut buffer = Buffer::empty(area);
    let painted = view.paint(area, &mut buffer);
    assert!(
        view.cursor(&painted).is_some(),
        "the empty draft's caret has a cell before the screen opens"
    );

    view.key(Key::Char('?'));
    let painted = view.paint(area, &mut buffer);

    assert_eq!(
        view.cursor(&painted),
        None,
        "a screen is the whole frame and has no draft in it, so the caret is not left \
             floating over a list"
    );
}

#[test]
fn a_draft_holding_a_wide_glyph_puts_the_caret_in_the_column_the_glyph_occupies() {
    assert_eq!(
        caret_at("日本", 40, 12),
        Some((6, 10)),
        "two wide glyphs take four columns, so the caret waits in the fourth from the \
             gutter the draft is set in"
    );
}

/// Where the caret a frame `cols` by `rows` wide puts the terminal's cursor, in a frame the
/// view has painted: the band's rows are a fact about the width, so nothing is asked before it.
fn caret_at(draft: &str, cols: u16, rows: u16) -> Option<(u16, u16)> {
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::none(),
    );
    view.paste(draft);
    let area = screen(cols, rows);
    let mut buffer = Buffer::empty(area);
    let painted = view.paint(area, &mut buffer);
    view.cursor(&painted)
}

#[test]
fn the_caret_follows_the_draft_onto_the_row_the_wrap_gave_it() {
    let asking = pen::filled(60, 'x');

    assert_eq!(
        caret_at(&asking, 40, 12),
        Some((24, 10)),
        "a thirty-eight column room holds thirty-eight glyphs of a draft of sixty, so the \
             caret waits twenty-two columns into the band's second row rather than off the \
             frame"
    );
    assert_eq!(
        caret_at(&asking, 80, 12),
        Some((62, 10)),
        "and a frame wide enough to hold the draft shows it on the one row it takes"
    );
}

#[test]
fn a_caret_past_a_full_row_waits_on_the_bands_last_cell() {
    let asking = pen::filled(38, 'x');

    assert_eq!(
        caret_at(&asking, 40, 12),
        Some((39, 10)),
        "a draft that fills the row's thirty-eight columns leaves the caret no cell after it, \
             and a terminal keeps its cursor on the last one rather than giving it up"
    );
}
