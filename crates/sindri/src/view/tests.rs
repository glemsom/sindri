//! What a golden frame cannot hold: the cell the draft's caret waits in, which a frame's cells name by
//! ink and the pass by a position, and the bands under the cells, which a fixture cannot read at all.

use sindri_core::policy::Enforcement;

use sindri_core::redact::SecretSet;
use sindri_core::run::Exit;

use ratatui::layout::Position;
use ratatui::style::Color;

use std::path::PathBuf;

use super::*;
use crate::composer::{AIR_ROWS, MAX_ROWS, MIN_ROWS, PLACEHOLDER};
use crate::ledger::Boot;

use crate::capability::caps::{inked, pictured, pointing, sized};
use crate::ink::{Face, Look};
use crate::palette::{Band, Edge};
use crate::pictures::PLACEHOLDER as PLACED;
use crate::plane::SEAM;
use crate::pulse::STEP_MS;
use crate::readback::under;
use crate::rows::SIGIL;
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

/// The one cell of `area` that reads back as the mark: the block the draft's caret stands in, as the
/// frame draws it rather than as the view counts it.
fn caret_cell(buffer: &Buffer, palette: &Palette, area: Rect) -> Option<(u16, u16)> {
    let mut found = None;
    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            let cell = buffer.cell((x, y)).expect("a cell of the area");
            if Look::of(cell, palette) != Some(Look::plain(Ink::Mark)) {
                continue;
            }
            assert!(
                found.is_none(),
                "{area:?} carries one block and not two: {:?} and ({x}, {y})",
                found
            );
            found = Some((x, y));
        }
    }
    found
}

/// The cell the pass placed the terminal's cursor in, counted as the frame's cells are.
fn caret_of(painted: &Painted) -> Option<(u16, u16)> {
    painted.caret().map(|at| (at.x, at.y))
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
    drafted(Facts::default().asked(), palette, lines, cols, rows)
}

/// The same band under `facts`, which is what says whether a run is live on it.
fn drafted(facts: Facts, palette: Palette, lines: usize, cols: u16, rows: u16) -> (Buffer, Layout) {
    let draft: Vec<String> = (1..=lines).map(|line| line.to_string()).collect();
    let mut view = View::of(inked(palette), facts, Transcript::none());
    view.paste(&draft.join("\n"));
    let (buffer, told) = painted_and_told(&mut view, cols, rows);
    (buffer, laid_out(&told))
}

/// The facts of a band a run is live in, standing on the step `elapsed_ms` of the pulse reaches.
fn live(elapsed_ms: u64) -> Facts {
    Facts::default()
        .running()
        .pulsed(Some(Pulse::live(elapsed_ms)))
}

/// The columns the band's edge gives its rule: the first of them, and how many there are, read off
/// the row the frame drew rather than counted off the layout again.
fn rule_span(buffer: &Buffer, band: Rect) -> (u16, u16) {
    let row = drawn_glyphs(buffer, band.y);
    let first = row
        .find([HAIRLINE, EDGE_HEAD])
        .unwrap_or_else(|| panic!("the edge of the band is not ruled: {row:?}"));
    let columns = row
        .chars()
        .filter(|glyph| [HAIRLINE, EDGE_HEAD].contains(glyph))
        .count();
    (band.x + pen::cells(&row[..first]), pen::columns(columns))
}

/// The columns of an edge row the draft's count is hung in, and none when no count hangs there: the
/// rule's last column names the one the count begins at, and the far corner is no column of it.
fn count_span(buffer: &Buffer, band: Rect) -> Range<u16> {
    let (first, columns) = rule_span(buffer, band);
    first.saturating_add(columns)..band.right().saturating_sub(1)
}

/// The columns of an edge row the edge itself draws: its word, its rule, its head and a corner at each
/// end, and never a column of the count hung on the corner beside them.
fn edge_columns(buffer: &Buffer, band: Rect) -> Vec<u16> {
    let counted = count_span(buffer, band);
    let close = band.right().saturating_sub(1);
    (band.x..close)
        .filter(|x| !counted.contains(x))
        .chain([close])
        .collect()
}

/// The column the head of a live run's edge stands in, read off the row it was drawn on.
fn head_at(buffer: &Buffer, band: Rect) -> u16 {
    let row = drawn_glyphs(buffer, band.y);
    let at = row
        .find(EDGE_HEAD)
        .expect("a live run's edge carries a head");
    band.x + pen::cells(&row[..at])
}

#[test]
fn the_composer_is_drawn_from_its_head_down_to_the_rails_own_tone() {
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
                        "row {row} of the {} at {cols}x{rows} sits on no shade of the band, \
                             so the band is not one raised surface",
                        band.height
                    );
                }
            }
            assert_eq!(
                under(&buffer, band.x, band.y),
                palette.band(Band::Composer),
                "the top row of the {} at {cols}x{rows} is not the head the band rises to",
                band.height
            );
            assert_eq!(
                under(&buffer, band.x, band.bottom() - 1),
                palette.band(Band::Rail),
                "the bottom row of the {} at {cols}x{rows} is not the rail's own tone, so the \
                     band is parted from the surface it stands on",
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

            for x in edge_columns(&buffer, band) {
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
fn no_row_of_the_composer_band_is_drawn_on_an_edge() {
    let palette = Palette::TRUECOLOR;
    for lines in [1, 5] {
        let (buffer, laid) = composer(palette, lines, 120, 40);
        let band = laid.composer;
        let on_edge = |edge: Edge, y: u16| {
            (band.x..band.right()).all(|x| under(&buffer, x, y) == palette.edge(edge))
        };

        assert_eq!(
            band.height,
            if lines == 1 { MIN_ROWS } else { MAX_ROWS },
            "the draft of {lines} lines is a band of {} rows, so this draws neither end of \
                 the heights a band takes",
            band.height
        );
        for edge in Edge::ALL {
            assert!(
                (band.y..band.bottom()).all(|y| !on_edge(edge, y)),
                "a {} band is drawn on its {edge:?} edge, and an edge is the light a boundary \
                     is raised out of or the shadow it is set down into, never a row of the band",
                band.height
            );
        }
        assert_eq!(
            under(&buffer, band.x, band.bottom() - 1),
            palette.band(Band::Rail),
            "a {} band comes down to a tone other than the rail's own",
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
fn a_frame_with_one_row_to_give_the_composer_draws_it_as_its_head() {
    let palette = Palette::TRUECOLOR;
    let (buffer, laid) = composer(palette, 1, 60, 2);
    let band = laid.composer;

    assert_eq!(band.height, 1, "a two-row frame gives the composer one row");
    assert_eq!(
        under(&buffer, band.x, band.y),
        palette.band(Band::Composer),
        "one row has no room for both ends, so it is drawn as the head and never on the frame's \
             own ground"
    );
}

#[test]
fn sixty_columns_show_the_raised_band_and_lose_no_hint_from_the_footer() {
    let palette = Palette::TRUECOLOR;
    let (buffer, laid) = composer(palette, 1, 60, 24);
    let band = laid.composer;

    assert_eq!(
        under(&buffer, band.x, band.y),
        palette.band(Band::Composer),
        "a sixty-column frame draws no head on the composer's top row"
    );
    assert_eq!(
        under(&buffer, band.x, band.bottom() - 1),
        palette.band(Band::Rail),
        "a sixty-column frame leaves the composer's foot above the rail's own tone"
    );
    assert_eq!(
        drawn_row(&buffer, laid.footer.y).trim_end(),
        "Enter send · / commands · @ files · ? keys · Ctrl-Q quit",
        "the raised band takes no row the footer had, and every hint that fits in sixty \
             columns is still drawn"
    );
}

#[test]
fn a_composer_at_rest_draws_a_box_that_opens_and_closes_on_its_edge_and_is_footed() {
    let palette = Palette::TRUECOLOR;
    for (lines, counted) in [(0, ""), (1, "1 char"), (3, "5 chars"), (5, "9 chars")] {
        let (buffer, laid) = composer(palette, lines, 120, 40);
        let band = laid.composer;
        let rule = pen::filled(band.width - 2, HAIRLINE);
        let word = PROMPT_EDGE;
        let edged = pen::filled(
            band.width - pen::cells(word) - pen::cells(counted) - 3,
            HAIRLINE,
        );

        assert_eq!(
            drawn_glyphs(&buffer, band.y),
            format!("{EDGE_OPEN}{word} {edged}{counted}{EDGE_CLOSE}"),
            "the {} row band a draft of {lines} lines takes does not open on the mode it is \
                 in and close along its edge",
            band.height
        );
        assert_eq!(
            drawn_glyphs(&buffer, band.bottom() - 1),
            format!("{FOOT_OPEN}{rule}{FOOT_CLOSE}"),
            "the {} row band a draft of {lines} lines takes is not footed",
            band.height
        );
        assert_eq!(
            look_at(&buffer, &palette, band.x, band.y),
            palette.accent_edge(),
            "the corner the edge opens with is not drawn in the look the edge row is"
        );
        assert_eq!(
            look_at(&buffer, &palette, band.x, band.bottom() - 1),
            palette.dim_edge(),
            "the corner the foot opens with is not the dim edge the shadow is drawn in"
        );
    }
}

#[test]
fn the_edge_of_a_band_at_rest_says_prompt_inside_its_opening_corner_in_the_faint_accent() {
    let palette = Palette::TRUECOLOR;
    for (lines, counted) in [(0, ""), (1, "1 char"), (3, "5 chars"), (5, "9 chars")] {
        let (buffer, laid) = composer(palette, lines, 120, 40);
        let band = laid.composer;
        let word = PROMPT_EDGE;
        let rule = pen::filled(
            band.width - pen::cells(word) - pen::cells(counted) - 3,
            HAIRLINE,
        );
        let row = drawn_glyphs(&buffer, band.y);

        assert_eq!(
            row,
            format!("{EDGE_OPEN}{word} {rule}{counted}{EDGE_CLOSE}"),
            "the edge of the {} row band a draft of {lines} lines takes says no mode, or says \
                 it outside the corner",
            band.height
        );
        assert!(
            !row.contains(EDGE_HEAD),
            "a band no run is live in carries no head: its edge is still, and the frame keeps no \
                 clock to move one"
        );
        for x in edge_columns(&buffer, band) {
            assert_eq!(
                look_at(&buffer, &palette, x, band.y),
                palette.accent_edge(),
                "column {x} of the edge row is not the faint accent a band at rest wears: the \
                     word, the rule and the corners are one row in one look"
            );
        }
    }
}

#[test]
fn the_edge_hangs_the_drafts_own_count_on_its_far_corner() {
    let palette = Palette::TRUECOLOR;
    let (buffer, laid) = composer(palette, 1, 120, 40);
    let band = laid.composer;
    let row = drawn_glyphs(&buffer, band.y);
    let ruled = row.strip_suffix("1 char┐").unwrap_or_else(|| {
        panic!("the edge hangs no count of the draft on its far corner: {row:?}")
    });

    assert_eq!(
        ruled.chars().next_back(),
        Some(HAIRLINE),
        "the rule does not end in the column before the count begins: {row:?}"
    );
}

#[test]
fn an_empty_draft_hangs_no_count_beside_a_rail_of_facts() {
    let palette = Palette::TRUECOLOR;
    let (buffer, laid) = drafted(Facts::default().asked().railed(rail()), palette, 0, 120, 40);
    let band = laid.composer;
    let rule = pen::filled(band.width - pen::cells(PROMPT_EDGE) - 3, HAIRLINE);

    assert_eq!(
        drawn_glyphs(&buffer, band.y),
        format!("{EDGE_OPEN}{PROMPT_EDGE} {rule}{EDGE_CLOSE}"),
        "an empty draft bills nothing, so the edge of a band beside a rail hangs no count"
    );
}

#[test]
fn a_frame_too_narrow_for_the_whole_count_draws_none_of_it_and_the_rule_keeps_the_row() {
    let palette = Palette::TRUECOLOR;
    let mut view = View::of(inked(palette), Facts::default().asked(), Transcript::none());
    // A thousand characters bill a count wider than the sixteen columns the box leaves.
    for _ in 0..500 {
        view.paste("a ");
    }
    let (buffer, told) = painted_and_told(&mut view, 18, 24);
    let band = laid_out(&told).composer;
    let rule = pen::filled(band.width - pen::cells(PROMPT_EDGE) - 3, HAIRLINE);

    assert_eq!(
        drawn_glyphs(&buffer, band.y),
        format!("{EDGE_OPEN}{PROMPT_EDGE} {rule}{EDGE_CLOSE}"),
        "a count the row has no room for is refused whole, and the rule keeps the row"
    );
}

#[test]
fn the_count_is_refused_where_the_rule_would_lose_its_last_column_and_drawn_one_column_wider() {
    let palette = Palette::TRUECOLOR;
    // Four lines are seven characters, so the count is the same seven columns at either width.
    let (refused, laid) = composer(palette, 4, 16, 24);
    let band = laid.composer;
    let rule = pen::filled(band.width - pen::cells(PROMPT_EDGE) - 3, HAIRLINE);

    assert_eq!(
        drawn_glyphs(&refused, band.y),
        format!("{EDGE_OPEN}{PROMPT_EDGE} {rule}{EDGE_CLOSE}"),
        "a count that would leave the rule no column of its own is refused whole, so the rule \
         keeps the row"
    );

    let (drawn, laid) = composer(palette, 4, 17, 24);
    let band = laid.composer;

    assert_eq!(
        drawn_glyphs(&drawn, band.y),
        format!("{EDGE_OPEN}{PROMPT_EDGE} {HAIRLINE}7 chars{EDGE_CLOSE}"),
        "one column wider, the count hangs and the rule keeps the one column left below it"
    );
}

#[test]
fn the_count_is_drawn_in_the_quiet_inks_the_frame_already_gives_a_meaning() {
    let palette = Palette::TRUECOLOR;
    let root = tempfile::tempdir().expect("a temp workspace");
    std::fs::create_dir_all(root.path().join("src")).expect("the directories a file sits in");
    std::fs::write(root.path().join("src/rows.rs"), "").expect("a file");
    let mut view = View::of(
        inked(palette),
        Facts::default()
            .asked()
            .working(root.path().to_path_buf(), None),
        Transcript::none(),
    );
    view.paste("read src/rows.rs");
    let (buffer, told) = painted_and_told(&mut view, 120, 40);
    let band = laid_out(&told).composer;
    let counted = pen::cells("1 file · 16 chars");
    let mut x = band.right() - 1 - counted;

    for (width, ink) in [(6, Ink::Muted), (3, Ink::Dim), (8, Ink::Muted)] {
        for _ in 0..width {
            assert_eq!(
                look_at(&buffer, &palette, x, band.y),
                Look::plain(ink),
                "column {x} of the count the edge hangs is not the {} ink its part is drawn in",
                ink.name()
            );
            x += 1;
        }
    }
    assert_eq!(
        x,
        band.right() - 1,
        "the count is drawn whole, and the far corner is the column after it"
    );
}

#[test]
fn the_edge_of_a_band_a_run_is_live_in_says_running_in_the_accent_with_one_head_on_the_rule() {
    let palette = Palette::TRUECOLOR;
    let (buffer, laid) = drafted(live(4_200), palette, 1, 120, 40);
    let band = laid.composer;
    let word = RUNNING_EDGE;
    let (first, columns) = rule_span(&buffer, band);
    let row = drawn_glyphs(&buffer, band.y);

    assert!(
        row.starts_with(&format!("{EDGE_OPEN}{word} ")),
        "a live run's edge does not open on the mode it is in, inside the corner: {row:?}"
    );
    assert!(
        row.ends_with(EDGE_CLOSE),
        "a live run's edge does not rule on to the far corner: {row:?}"
    );
    assert_eq!(
        row.matches(EDGE_HEAD).count(),
        1,
        "a live run's edge carries one head and no more: {row:?}"
    );
    let head = head_at(&buffer, band);
    assert!(
        (first..first + columns).contains(&head),
        "the head stands at column {head}, away from the rule's own columns {first} to {}: the \
             word is never a column of it",
        first + columns
    );
    for x in edge_columns(&buffer, band) {
        assert_eq!(
            look_at(&buffer, &palette, x, band.y),
            palette.lit_edge(),
            "column {x} of a live run's edge row is not the accent lit: the word, the rule and \
                 the head are one row in one look"
        );
    }
}

#[test]
fn every_step_of_the_pulse_moves_the_head_one_column_and_the_rule_s_width_is_one_cycle() {
    let palette = Palette::TRUECOLOR;
    let (buffer, laid) = drafted(live(0), palette, 1, 60, 24);
    let band = laid.composer;
    let (first, columns) = rule_span(&buffer, band);

    assert_eq!(
        head_at(&buffer, band),
        first,
        "a live band with no step behind it stands its head where the rule begins"
    );
    for step in 0..6u64 {
        for at in [step * STEP_MS, step * STEP_MS + STEP_MS - 1] {
            let (buffer, _) = drafted(live(at), palette, 1, 60, 24);
            assert_eq!(
                head_at(&buffer, band),
                first + pen::columns(usize::try_from(step).unwrap_or(0)),
                "step {step} of the pulse, at {at} ms, is not one column further along the rule"
            );
        }
    }
    let (buffer, _) = drafted(live(u64::from(columns) * STEP_MS), palette, 1, 60, 24);
    assert_eq!(
        head_at(&buffer, band),
        first,
        "one cycle of the head is the rule's own width of {columns} columns, and it wraps back \
             to the first"
    );
}

#[test]
fn two_passes_of_one_live_frame_put_the_head_on_one_cell() {
    let palette = Palette::TRUECOLOR;
    let mut view = View::of(inked(palette), live(4_200), Transcript::none());
    view.paste("the draft the band is drawn from");
    let band = laid_out(&painted_and_told(&mut view, 60, 24).1).composer;

    let once = head_at(&painted(&mut view, 60, 24), band);
    let again = head_at(&painted(&mut view, 60, 24), band);

    assert_eq!(
        once, again,
        "the view keeps no clock of its own, so two passes at one elapsed time draw one cell"
    );
}

#[test]
fn a_band_no_run_is_live_in_draws_no_head_however_live_the_pulse_handed_to_it() {
    let palette = Palette::TRUECOLOR;
    let facts = Facts::default().asked().pulsed(Some(Pulse::live(4_200)));
    let (buffer, laid) = drafted(facts, palette, 1, 60, 24);

    assert!(
        !drawn_glyphs(&buffer, laid.composer.y).contains(EDGE_HEAD),
        "the head is the run's, so a band no run is live in says the mode and nothing else"
    );
}

#[test]
fn the_word_and_the_head_recede_to_the_dim_edge_while_the_window_is_away_and_under_a_screen() {
    let palette = Palette::TRUECOLOR;

    let mut view = View::of(inked(palette), live(0).focused(false), Transcript::none());
    view.paste("the draft the band is drawn from");
    let (buffer, told) = painted_and_told(&mut view, 120, 40);
    let band = laid_out(&told).composer;
    assert!(
        drawn_glyphs(&buffer, band.y).contains(EDGE_HEAD),
        "the head is still drawn while nobody reads the window, and only recedes"
    );
    for x in edge_columns(&buffer, band) {
        assert_eq!(
            look_at(&buffer, &palette, x, band.y),
            palette.dim_edge(),
            "column {x} of the edge a window nobody reads is not the dim edge: the word and the \
                 head recede with the rule"
        );
    }

    let mut view = View::of(inked(palette), live(0), Transcript::none());
    view.paste("the draft the band is drawn from");
    view.screen(Mode::Keys);
    let (buffer, told) = painted_and_told(&mut view, 120, 40);
    let band = laid_out(&told).composer;
    let (left, right) = card::border_columns(card_body(&told));
    for x in edge_columns(&buffer, band)
        .into_iter()
        .filter(|x| *x < left || *x > right)
    {
        assert_eq!(
            look_at(&buffer, &palette, x, band.y),
            palette.dim_edge(),
            "column {x} of the edge under {} is not the dim edge, and the card is the surface \
                 that takes the keys",
            Mode::Keys.name()
        );
    }
}

#[test]
fn a_band_with_no_room_for_the_word_and_a_space_and_a_rule_column_draws_the_plain_rule() {
    let palette = Palette::TRUECOLOR;
    let word = PROMPT_EDGE;
    for (cols, named) in [(9u16, false), (10, true)] {
        let (buffer, laid) = composer(palette, 0, cols, 6);
        let band = laid.composer;
        let drawn = drawn_glyphs(&buffer, band.y);
        let expected = match named {
            true => format!("{EDGE_OPEN}{word} {}{EDGE_CLOSE}", pen::filled(1, HAIRLINE)),
            false => format!(
                "{EDGE_OPEN}{}{EDGE_CLOSE}",
                pen::filled(band.width - 2, HAIRLINE)
            ),
        };

        assert_eq!(
            drawn, expected,
            "a band of {cols} columns takes the word where the word, its blank column and one \
                 rule column do not all fit, or gives it up where they do"
        );
    }

    let (buffer, laid) = drafted(live(0), palette, 0, 9, 6);
    let band = laid.composer;
    assert_eq!(
        drawn_glyphs(&buffer, band.y),
        format!(
            "{EDGE_OPEN}{EDGE_HEAD}{}{EDGE_CLOSE}",
            pen::filled(band.width - 3, HAIRLINE)
        ),
        "a band too narrow for the word still travels its head over the plain rule"
    );
}

#[test]
fn one_draft_drawn_idle_and_live_differs_on_the_edge_row_and_the_footer_and_nowhere_in_its_bands() {
    let palette = Palette::TRUECOLOR;
    let mut view = View::of(
        inked(palette),
        Facts::default().asked().pulsed(Some(Pulse::live(4_200))),
        Transcript::none(),
    );
    view.paste("the draft the two frames are drawn from");
    let (resting, resting_layout) = painted_and_told(&mut view, 120, 40);
    view.handed(view.facts().clone().running());
    let (live, live_layout) = painted_and_told(&mut view, 120, 40);
    let laid = laid_out(&resting_layout);
    let band = laid.composer;

    assert_eq!(
        band,
        laid_out(&live_layout).composer,
        "a run being live takes no row and no column the band had at rest"
    );
    for y in band.y..band.bottom() {
        assert_eq!(
            drawn_row(&resting, y) == drawn_row(&live, y),
            y != band.y,
            "row {y} of the band is not what the mode and the head leave of it: the mode is the \
                 edge's word and the run's state is the footer's"
        );
    }
    assert_ne!(
        drawn_row(&resting, laid.footer.y),
        drawn_row(&live, laid.footer.y),
        "the footer of an idle draft and one of a live run say the same keys, so neither says \
             what the frame can honour"
    );
}

#[test]
fn every_row_between_the_edge_and_the_foot_carries_an_upright_in_the_bands_first_and_last_column() {
    let palette = Palette::TRUECOLOR;
    let (buffer, laid, _) = waiting_behind(palette, "a draft", 2, 120, 24);
    let band = laid.composer;

    for y in band.y.saturating_add(1)..band.bottom().saturating_sub(1) {
        assert_eq!(
            buffer.cell((band.x, y)).map(|cell| cell.symbol()),
            Some("\u{2502}"),
            "row {y} of the {} row band has no upright in its first column",
            band.height
        );
        assert_eq!(
            buffer.cell((band.right() - 1, y)).map(|cell| cell.symbol()),
            Some("\u{2502}"),
            "row {y} of the {} row band has no upright in its last column",
            band.height
        );
        assert_eq!(
            look_at(&buffer, &palette, band.right() - 1, y),
            palette.dim_edge(),
            "the upright of row {y} is not the dim edge a side of the box is drawn in"
        );
    }
}

#[test]
fn an_empty_frame_and_a_one_line_draft_draw_the_band_in_three_rows() {
    let mut empty = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::none(),
    );
    let (buffer, told) = painted_and_told(&mut empty, 120, 40);
    let band = laid_out(&told).composer;

    assert_eq!(
        band.height, 3,
        "a frame holding no draft draws the rule, the row the draft sits on and the foot"
    );
    assert_eq!(
        ends(&empty).above,
        1,
        "so the band spends one row above the draft: its rule and no air"
    );
    assert!(
        drawn_glyphs(&buffer, band.y + 1).contains(PLACEHOLDER),
        "and the placeholder stands on the row under the rule"
    );
    assert!(
        drawn_glyphs(&buffer, band.y + 2).starts_with(FOOT_OPEN),
        "with the foot under it and no air row between them"
    );

    let (buffer, laid) = composer(Palette::TRUECOLOR, 1, 120, 40);
    let band = laid.composer;
    let drafted = drawn_glyphs(&buffer, band.y + 1);

    assert_eq!(
        band.height, 3,
        "and a draft of one line takes no more rows than an empty one"
    );
    assert!(
        drafted.starts_with(&format!("{UPRIGHT}{SIGIL}1")),
        "its words stand on the row under the rule: {drafted:?}"
    );
    assert!(
        drawn_glyphs(&buffer, band.y + 2).starts_with(FOOT_OPEN),
        "and the foot is the row under it"
    );
}

#[test]
fn a_draft_on_two_rows_draws_a_row_of_air_on_each_side_of_it_again() {
    let (buffer, laid) = composer(Palette::TRUECOLOR, 2, 120, 40);
    let band = laid.composer;
    let air = format!("{UPRIGHT}{}{UPRIGHT}", pen::filled(band.width - 2, ' '));
    let under_rule = band.y + 1 + AIR_ROWS;

    assert_eq!(
        band.height, 6,
        "a second row of draft brings both rows of air back, and the band grows with it"
    );
    for (y, why) in [
        (band.y + AIR_ROWS, "above the draft"),
        (band.bottom() - AIR_ROWS - 1, "over the foot"),
    ] {
        assert_eq!(
            drawn_glyphs(&buffer, y),
            air,
            "row {y}, {why}, is not air: the box leaves it its two uprights and no glyph"
        );
    }
    let drafted = drawn_glyphs(&buffer, under_rule);
    assert!(
        drafted.starts_with(&format!("{UPRIGHT}{SIGIL}1")),
        "the draft opens on the row under the air: {drafted:?}"
    );
    assert!(
        drawn_glyphs(&buffer, under_rule + 1).contains('2'),
        "and its second row is the one under that"
    );
}

#[test]
fn a_queued_prompt_adds_a_row_and_never_an_air_row() {
    let palette = Palette::TRUECOLOR;
    let (_, alone, _) = waiting_behind(palette, "a draft", 0, 120, 24);
    let (buffer, queued, ends) = waiting_behind(palette, "a draft", 1, 120, 24);

    assert_eq!(
        queued.composer.height,
        alone.composer.height + 1,
        "a prompt waiting behind a draft of one row takes one row of the band and no more"
    );
    assert_eq!(
        draft_rows(&buffer, queued.composer, ends),
        ["\u{203a} a draft", "\u{203a} prompt 1"],
        "so the two rows under the rule are the draft's and the waiting prompt's, with no air \
             between them"
    );
}

#[test]
fn the_transcript_gains_the_two_rows_an_idle_band_gives_up() {
    let mut idle = turns(100);
    idle.paste("one line");
    let (buffer, told) = painted_and_told(&mut idle, 120, 40);
    let small = laid_out(&told);

    let mut wrapped = turns(100);
    wrapped.paste("one\ntwo");
    let (_, told) = painted_and_told(&mut wrapped, 120, 40);
    let tall = laid_out(&told);

    assert_eq!(
        small.transcript.height,
        40 - 1 - MIN_ROWS,
        "a frame of forty rows gives the transcript every row its footer and the band's three \
             do not: the two rows the band's air took are the transcript's"
    );
    assert_eq!(
        tall.transcript.height,
        40 - 1 - tall.composer.height,
        "and a band of {} rows takes its own rows back from the transcript",
        tall.composer.height
    );
    assert_eq!(
        small.transcript.bottom(),
        small.composer.y,
        "and the transcript's band runs down to the composer's rule"
    );
    assert!(
        drawn_row(&buffer, small.composer.y - 1).contains("turn"),
        "so the last row of the transcript is drawn, one row above the band: {:?}",
        drawn_row(&buffer, small.composer.y - 1)
    );
}

#[test]
fn a_draft_exactly_as_wide_as_the_room_fills_its_row_and_touches_neither_upright() {
    let room = 60 - pen::cells(INDENT) - 2;
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::none(),
    );
    view.paste(&pen::filled(room, 'x'));

    let (buffer, told) = painted_and_told(&mut view, 60, 24);
    let band = laid_out(&told).composer;

    assert_eq!(
        drawn_glyphs(&buffer, band.y + ends(&view).above),
        format!("{UPRIGHT}{SIGIL}{}{UPRIGHT}", pen::filled(room, 'x')),
        "a draft of {room} glyphs does not fill the columns the box leaves it"
    );
    assert_eq!(
        view.composer().rows(),
        MIN_ROWS,
        "a draft that fills the room is still one row of draft"
    );
    assert_eq!(
        view.composer().caret(),
        (0, room),
        "and its caret waits at the end of that row, inside the box"
    );
}

#[test]
fn a_queues_rows_stand_inside_the_box_and_the_foot_is_still_the_bands_last_row() {
    let palette = Palette::TRUECOLOR;
    let (buffer, laid, ends) = waiting_behind(palette, "a draft", 3, 120, 24);
    let band = laid.composer;
    let queue = laid.queue;

    assert_eq!(
        queue.bottom(),
        band.bottom() - ends.below,
        "the queue's rows do not end on the row above the foot"
    );
    for y in queue.y..queue.bottom() {
        let row = drawn_glyphs(&buffer, y);
        assert!(
            row.starts_with(UPRIGHT) && row.ends_with(UPRIGHT),
            "row {y} of the queue is not drawn inside the box: {row:?}"
        );
    }
    assert_eq!(
        drawn_glyphs(&buffer, band.bottom() - 1),
        format!(
            "{FOOT_OPEN}{}{FOOT_CLOSE}",
            pen::filled(band.width - 2, HAIRLINE)
        ),
        "the foot is not the band's last row"
    );
    assert_eq!(
        laid.footer.y,
        band.bottom(),
        "and the footer is the row under the whole band"
    );
    assert_eq!(
        under(&buffer, band.x, band.y),
        palette.band(Band::Composer),
        "a band holding a queue still rises to its head"
    );
    assert_eq!(
        under(&buffer, band.x, band.bottom() - 1),
        palette.band(Band::Rail),
        "and still comes down to the rail's own tone, which is the row the queue does not reach"
    );
}

#[test]
fn the_caret_is_placed_above_the_queue_that_stands_under_the_draft() {
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().running(),
        Transcript::none(),
    );
    view.enqueue(prompt("a prompt behind the draft"));
    view.paste("a draft");
    let (_, told) = painted_and_told(&mut view, 120, 24);
    let laid = laid_out(&told);
    let (_, row) = caret_of(&told).expect("the draft's caret has a cell");

    assert_eq!(
        row,
        laid.queue.y - 1,
        "the caret is placed on the draft's last row and never on a prompt the draft did not write"
    );
}

#[test]
fn a_frame_asked_for_no_colour_draws_the_same_box_in_the_same_glyphs() {
    let (coloured, laid) = composer(Palette::TRUECOLOR, 3, 60, 24);
    let (flat, flat_laid) = composer(Palette::SLOTS, 3, 60, 24);
    assert_eq!(laid.composer, flat_laid.composer);
    let band = laid.composer;

    for y in band.y..band.bottom() {
        assert_eq!(
            drawn_glyphs(&flat, y),
            drawn_glyphs(&coloured, y),
            "row {y} of a frame asked for no colour is not the row a coloured frame draws: the \
                 box is no second look for a reader who asked for none"
        );
    }
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

/// A frame whose transcript runs to the seam the rail is drawn across, on a terminal that places
/// pictures and the face switched off or on one that places none: the two differ in the plane under
/// their cells and in no row the rail gives a picture.
fn seamed(draws: bool) -> View {
    View::of(
        match draws {
            true => pictured().faced(false),
            false => Capabilities::default(),
        },
        Facts::default().asked().railed(rail()),
        Transcript::of(vec![Entry::Assistant {
            turn: 1,
            text: SecretSet::default().redact(
                "the last words of the transcript and the first of the rail read as they read \
                 before the seam fell between them, and no column of either is cut",
            ),
        }]),
    )
}

/// The columns of the seam: the transcript's last few, where the rail's plane reaches back over
/// them.
fn seam_of(laid: Layout) -> Rect {
    Rect {
        x: laid.transcript.right() - SEAM,
        width: SEAM,
        ..laid.transcript
    }
}

#[test]
fn a_frame_that_places_a_picture_claims_the_seam_as_the_terminals_own_ground() {
    let palette = Palette::TRUECOLOR;

    let (pictured, told) = painted_and_told(&mut seamed(true), 120, 40);
    let laid = laid_out(&told);
    let seam = seam_of(laid);
    for y in seam.y..seam.bottom() {
        for x in seam.x..seam.right() {
            assert_eq!(
                under(&pictured, x, y),
                palette.ground(None).bg.expect("the terminal's own ground"),
                "row {y}, column {x} of the seam is not the ground the plane is drawn at, so the \
                 transcript's last columns are no part of the fall toward the rail"
            );
        }
    }

    let (placeless, told) = painted_and_told(&mut seamed(false), 120, 40);
    let seam = seam_of(laid_out(&told));
    for y in seam.y..seam.bottom() {
        for x in seam.x..seam.right() {
            assert_eq!(
                under(&placeless, x, y),
                palette.band(Band::Well),
                "row {y}, column {x} is claimed by a plane a terminal with no picture was \
                 handed none of: where no picture is placed the transcript keeps its own ground"
            );
        }
    }
}

#[test]
fn the_seam_draws_the_same_words_in_the_same_inks_with_a_picture_placed_and_with_none() {
    let (pictured_frame, told) = painted_and_told(&mut seamed(true), 120, 40);
    let (placeless, without) = painted_and_told(&mut seamed(false), 120, 40);
    let laid = laid_out(&told);
    let bare = laid_out(&without);

    assert_eq!(
        laid.rail, bare.rail,
        "a terminal that places a picture is handed the rail's own rect and columns, so the seam \
         costs the rail no column and the split is the one it always was"
    );
    let rail = laid.rail.expect("a frame this wide holds a ledger").1.area;
    for y in laid.transcript.y..laid.transcript.bottom() {
        for x in seam_of(laid).x..rail.right() {
            let with = pictured_frame.cell((x, y)).expect("a cell of the frame");
            let not = placeless.cell((x, y)).expect("a cell of the frame");
            assert_eq!(
                (with.symbol(), with.fg, with.modifier),
                (not.symbol(), not.fg, not.modifier),
                "row {y}, column {x} is drawn differently on a terminal that places a picture, so \
                 the shadow the plane falls in shades a word or takes a column from it"
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
        14,
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
    let launch = |draws: bool| {
        let mut view = View::of(
            match draws {
                true => pictured(),
                false => Capabilities::default(),
            },
            Facts::boot(boot(), None),
            Transcript::none(),
        );
        let mut buffer = Buffer::empty(screen(30, 24));
        let told = view.paint(buffer.area, &mut buffer);
        (buffer, laid_out(&told).composer)
    };
    let (pictured, band) = launch(true);
    let (placeless, _) = launch(false);
    let area = pictured.area;

    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            let (with, without) = (&pictured[(x, y)], &placeless[(x, y)]);
            match band.contains(Position::new(x, y)) {
                // The band is the one surface a picture is drawn under, so the two terminals
                // differ there in the ink behind a word and in no word.
                true => assert_eq!(
                    with.symbol(),
                    without.symbol(),
                    "the band's words at ({x}, {y}) are the words a terminal that places no \
                     picture draws"
                ),
                false => assert_eq!(
                    with, without,
                    "at 30 by 24 the frame is the splash screen and no rail beside it, so the \
                     cell at ({x}, {y}) is the one a terminal that places no picture draws"
                ),
            }
        }
    }
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
    let painted = view.paint(area, &mut buffer);
    assert!(
        painted.layout().expect("a laid out frame").rail.is_none(),
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
        Scroll::Back(36),
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
        "a transcript four rows taller than its band is a notch back"
    );
    assert_eq!(
        view.scroll(),
        Scroll::Back(4),
        "but only as far as its first row, which one page reaches"
    );
    assert_eq!(
        view.pointer(&told, Pointer::Wheel(Notch::Up)),
        Outcome::Unchanged,
        "and a notch up already at the first row moves nothing"
    );
    assert_eq!(view.scroll(), Scroll::Back(4), "the row it stopped on");

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
        Scroll::Back(36),
        "a 120x40 frame gives the transcript thirty-six rows — forty less the footer \
             and the composer's three — so one page moves it that far back"
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
        Scroll::Back(36),
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
        "36 rows back \u{b7} End follows the run",
        "the row says how many rows back the frame stands and the key that returns it"
    );
    assert_eq!(
        y, 35,
        "the row stands above the composer's edge row and is the thirty-sixth of forty, \
             rather than a row of the transcript it is a fact about"
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
        "38 rows back \u{b7} End follows the run",
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
        holds(&roomy, "36 rows back \u{b7} End follows the run"),
        "and forty columns hold the whole row"
    );
}

#[test]
fn a_frame_too_short_for_the_row_gives_it_up_before_the_transcript_loses_its_last_row() {
    let mut view = turns(100);
    painted(&mut view, 120, 40);
    view.key(Key::PageUp);

    let close = painted(&mut view, 120, 5);
    assert!(
        !holds(&close, "rows back"),
        "a frame of five rows holds the composer's three and the footer's one, so the row \
             that says where the reader stands is given up and the transcript keeps its row: \
             {:?}",
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
        Scroll::Back(4),
        "forty rows of transcript in a band of thirty-six leaves four rows behind the \
             frame, so the one page back is four rows and not thirty-six"
    );
    painted(&mut view, 120, 40);
    assert_eq!(
        view.key(Key::PageUp),
        Outcome::Edited,
        "the row saying where the frame stands is drawn out of that band, so reaching the \
             first row is the second press rather than the first"
    );
    assert_eq!(view.scroll(), Scroll::Back(5));
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
            Scroll::Back(5),
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
fn the_caret_is_placed_on_the_carets_row_of_the_draft() {
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::none(),
    );
    view.paste("first line\nsecond line");
    let (_, told) = painted_and_told(&mut view, 40, 12);

    assert_eq!(
        view.composer().rows(),
        6,
        "two lines of draft between the band's two rows of air"
    );
    assert_eq!(
        caret_of(&told),
        Some((14, 8)),
        "the caret is placed in the column it takes in the draft, and the draft is set \
             two columns in from the upright"
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
fn the_draft_is_set_behind_its_sigil_one_column_in_from_the_bands_upright() {
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
        draft_column,
        answer_column + 1,
        "the answer is set from column {answer_column} ({answered:?}) and the draft from \
             {draft_column} ({drafted:?}): the draft is set in the answer's own gutter, one \
             column in for the upright the box draws"
    );
    let gutter: String = drafted.chars().take(usize::from(draft_column)).collect();
    assert_eq!(
        gutter, "\u{2502}\u{203a} ",
        "the columns before the draft are not the upright and the sigil that opens it in the \
             gutter"
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
    let rows = 108;
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
    let thumb = thumb_rows(&head);
    assert_eq!(
        thumb,
        (0..15).collect::<Vec<u16>>(),
        "forty frame rows less the footer's one, the composer's three and the row saying \
             where the reader stands leave thirty-five, of which an eighty-row transcript fills \
             fifteen: the thumb opens the band at the head"
    );
    assert_eq!(
        look_at(&head, &Palette::TRUECOLOR, 0, thumb[0]).ink(),
        Ink::Dim,
        "the thumb is a dim cell of the gutter, and not a word's ink"
    );
}

#[test]
fn the_thumb_follows_the_window_down_the_frame() {
    let middle = painted(&mut scrolled(80, Scroll::Back(22)), 120, 40);
    assert_eq!(
        thumb_rows(&middle),
        (10..25).collect::<Vec<u16>>(),
        "half way from the head to the tail stands the thumb half way down the twenty rows \
             the band leaves it"
    );

    let foot = painted(&mut scrolled(80, Scroll::Back(1)), 120, 40);
    assert_eq!(
        thumb_rows(&foot),
        (20..35).collect::<Vec<u16>>(),
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

/// The ends the band asks for, as the frame last drew it.
fn ends(view: &View) -> Ends {
    view.composer().ends()
}

/// The rows of the composer's box the draft is drawn in, with the upright at each end of them taken
/// off.
fn draft_rows(buffer: &Buffer, band: Rect, ends: Ends) -> Vec<String> {
    (band.y.saturating_add(ends.above)..band.bottom().saturating_sub(ends.below))
        .map(|y| {
            drawn_glyphs(buffer, y)
                .trim_matches(UPRIGHT)
                .trim_end()
                .to_owned()
        })
        .collect()
}

/// An idle frame holding `count` waiting prompts behind the draft `draft`, painted at `cols`
/// by `rows`, the frame it laid out and the ends the band asked for.
fn waiting_behind(
    palette: Palette,
    draft: &str,
    count: usize,
    cols: u16,
    rows: u16,
) -> (Buffer, Layout, Ends) {
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
    (buffer, laid_out(&told), ends(&view))
}

#[test]
fn a_queue_of_three_is_drawn_under_the_draft_one_prompt_to_a_row_with_the_newest_at_the_foot() {
    let palette = Palette::TRUECOLOR;
    let (buffer, laid, ends) = waiting_behind(palette, "a draft", 3, 120, 24);
    let band = laid.composer;
    let queue = laid.queue;

    assert_eq!(
        draft_rows(&buffer, band, ends),
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
    let (buffer, laid, ends) = waiting_behind(palette, "a draft", 7, 120, 24);
    let band = laid.composer;
    let queue = laid.queue;

    assert_eq!(
        queue.height,
        crate::queue::MAX_ROWS,
        "the queue is given the four rows it asks for at most"
    );
    assert_eq!(
        draft_rows(&buffer, band, ends),
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
    let (buffer, laid, ends) = waiting_behind(palette, "a draft", 0, 120, 24);
    let band = laid.composer;
    let queue = laid.queue;

    assert_eq!(queue.height, 0, "nothing waiting is no queue row at all");
    assert_eq!(
        band.height, 3,
        "so the band is the draft's own rows: the rule, the one row the draft takes and the \
             foot"
    );
    assert_eq!(
        draft_rows(&buffer, band, ends),
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
    let (_, waited, _) = waiting_behind(palette, "a draft", 3, 120, 24);
    let (_, clear, _) = waiting_behind(palette, "a draft", 0, 120, 24);

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
        draft_rows(&buffer, band, ends(&view)),
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
    let (_, y) = caret_of(&told).expect("a caret is placed for a long draft");
    assert_eq!(
        y,
        band.bottom() - ends(&view).below - 2,
        "and the caret is placed on the last row the band draws the draft in: above the row \
             that counts the rest and above the foot"
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
        draft_rows(&buffer, band, ends(&view)),
        [
            format!("\u{203a} {}", wide(28)),
            format!("  {}", wide(28)),
            format!("  {}", wide(4)),
        ],
        "a fifty-six column room holds twenty-eight wide glyphs, and the sixtieth glyph \
             begins the third row of the draft with the gutter beside it"
    );
    assert_eq!(
        band.height, 7,
        "three rows of draft between the band's two rows of air"
    );
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
    let (empty, at) = place_of(&buffer, PLACEHOLDER);
    let gutter = empty
        .find(PLACEHOLDER)
        .expect("the row the placeholder begins on");
    assert_eq!(
        &empty[..gutter],
        format!("{UPRIGHT}{SIGIL}"),
        "the placeholder {PLACEHOLDER:?} does not stand in the gutter inside the box: \
             {empty:?}"
    );
    let column = at + pen::cells(PLACEHOLDER) - 1;
    assert_eq!(
        look_at(&buffer, &palette, column, band.y + ends(&view).above),
        Look::plain(Ink::Dim),
        "the placeholder is not drawn in the dim ink"
    );

    view.key(Key::Char('a'));
    let buffer = painted(&mut view, 120, 40);
    assert!(
        !drawn_row(&buffer, band.y + ends(&view).above).contains(PLACEHOLDER),
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
        column,
        1 + pen::cells(INDENT),
        "the draft under the card is still set in the gutter, one column in from the \
             band's upright: {drawn:?}"
    );
    assert_eq!(
        look_at(&buffer, &palette, column, band.y + ends(&view).above),
        Look::plain(Ink::Dim),
        "the scrim reads the draft down rather than taking it away: a reader sees the words \
             they will return to"
    );
    assert_eq!(
        caret_of(&told),
        None,
        "and no caret is placed in a band that takes no key"
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
fn a_card_leaves_the_caret_placed_on_the_draft_it_dims() {
    let palette = Palette::TRUECOLOR;
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::none(),
    );
    view.paste("/model");

    let (buffer, told) = painted_and_told(&mut view, 120, 40);
    let band = laid_out(&told).composer;
    let (x, y) = caret_of(&told).expect("a card scrims the band and leaves the caret placed on it");

    assert_eq!(
        y,
        band.y + ends(&view).above,
        "the caret is placed on the band's first draft row"
    );
    assert_eq!(
        look_at(&buffer, &palette, x - 1, y),
        Look::plain(Ink::Dim),
        "one column past the last word of the draft the card draws down"
    );
}

#[test]
fn a_boxed_card_that_reaches_the_drafts_row_holds_the_cell_the_caret_would_be_placed_in() {
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::none(),
    );
    view.paste("/");

    let (buffer, told) = painted_and_told(&mut view, 60, 13);
    let band = laid_out(&told).composer;
    assert!(
        drawn_row(&buffer, band.y + ends(&view).above).contains("Esc"),
        "a card nine rows tall in a thirteen row frame draws its foot on the draft's first row"
    );
    assert_eq!(
        caret_of(&told),
        None,
        "so no cell of the card is the cell the caret is placed in: the band alone carries it"
    );
}

#[test]
fn a_bleeding_card_holds_the_whole_frame_and_places_no_caret() {
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::none(),
    );
    view.paste("/");

    let (buffer, told) = painted_and_told(&mut view, 40, 12);
    let band = laid_out(&told).composer;
    assert!(
        !drawn_row(&buffer, band.y + ends(&view).above).contains(SIGIL),
        "a card too narrow for a border is grounded over the whole frame, the band with it"
    );
    assert_eq!(
        caret_of(&told),
        None,
        "so the caret of the band is placed on none of its cells"
    );
}

#[test]
fn a_screen_is_drawn_with_no_caret_placed_on_the_band() {
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::none(),
    );
    let (_, told) = painted_and_told(&mut view, 120, 40);
    assert!(
        caret_of(&told).is_some(),
        "the empty draft's caret has a cell before the screen opens"
    );

    view.key(Key::Char('?'));
    let (_, told) = painted_and_told(&mut view, 120, 40);

    assert_eq!(
        caret_of(&told),
        None,
        "a screen is the whole frame and the draft behind it takes no key, so no caret is \
         placed on the band"
    );
}

#[test]
fn a_draft_holding_a_wide_glyph_places_the_caret_in_the_column_the_glyph_occupies() {
    assert_eq!(
        placed_at("日本", 40, 12),
        Some((7, 9)),
        "two wide glyphs take four columns, so the caret is placed in the fourth column after \
             the gutter, one column in from the band's upright"
    );
}

/// Where a frame `cols` by `rows` wide places the caret, read off the pass rather than off the
/// view: the band's rows are a fact about the width, so nothing is asked before it is painted.
fn placed_at(draft: &str, cols: u16, rows: u16) -> Option<(u16, u16)> {
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::none(),
    );
    view.paste(draft);
    caret_of(&painted_and_told(&mut view, cols, rows).1)
}

#[test]
fn the_caret_follows_the_draft_onto_the_row_the_wrap_gave_it() {
    let asking = pen::filled(60, 'x');

    assert_eq!(
        placed_at(&asking, 40, 12),
        Some((27, 8)),
        "a thirty-six column room holds thirty-six glyphs of a draft of sixty, so the caret is \
             placed twenty-four columns into the band's second row rather than off the frame"
    );
    assert_eq!(
        placed_at(&asking, 80, 12),
        Some((63, 9)),
        "and a frame wide enough to hold the draft shows it on the one row it takes"
    );
}

#[test]
fn a_caret_at_the_end_of_a_full_row_is_placed_in_the_last_column_inside_the_box() {
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::none(),
    );
    let room = 40 - pen::cells(INDENT) - 2;
    view.paste(&pen::filled(room, 'x'));

    let (buffer, told) = painted_and_told(&mut view, 40, 12);
    let band = laid_out(&told).composer;
    let (column, row) = caret_of(&told).expect("a caret is placed for a draft that fills the box");

    assert_eq!(
        column,
        band.right() - 2,
        "a draft that fills the row's {room} columns leaves the caret no column after it, so \
             it is placed in the last one inside the box rather than on the upright"
    );
    assert_eq!(
        buffer.cell((column + 1, row)).map(|cell| cell.symbol()),
        Some("\u{2502}"),
        "and the column after the caret is the band's right upright"
    );
    assert!(
        row < band.bottom() - ends(&view).below,
        "the caret is placed on a row inside the box and never on the foot"
    );
}

/// The ink the gutter of the row at `x`, `y` is drawn in: the cell its sigil stands in, or the blank
/// the indent leaves there.
fn gutter_ink(buffer: &Buffer, palette: &Palette, x: u16, y: u16) -> Ink {
    look_at(buffer, palette, x, y).ink()
}

#[test]
fn the_sigil_is_lit_on_the_row_the_caret_waits_in_and_on_no_other() {
    let palette = Palette::TRUECOLOR;
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::none(),
    );
    view.paste("one\ntwo\nthree");
    view.key(Key::Up);
    view.key(Key::Up);

    let (buffer, told) = painted_and_told(&mut view, 120, 40);
    let band = laid_out(&told).composer;
    let gutter = band.x + 1;
    let first = band.y + ends(&view).above;
    assert_eq!(
        gutter_ink(&buffer, &palette, gutter, first),
        Ink::Accent,
        "the caret waits on the row the draft opens with, so its sigil is the lit one"
    );
    assert_eq!(
        gutter_ink(&buffer, &palette, gutter, first + 1),
        Ink::Dim,
        "and the row under the caret's keeps the dim ink where its sigil would stand"
    );
    assert_eq!(
        gutter_ink(&buffer, &palette, gutter, first + 2),
        Ink::Dim,
        "as does the row under that one"
    );

    view.key(Key::Down);
    let (buffer, told) = painted_and_told(&mut view, 120, 40);
    let band = laid_out(&told).composer;
    let first = band.y + ends(&view).above;
    assert_eq!(
        gutter_ink(&buffer, &palette, band.x + 1, first),
        Ink::Dim,
        "the caret has left the draft's first row, so its sigil is no longer lit"
    );
    assert_eq!(
        gutter_ink(&buffer, &palette, band.x + 1, first + 1),
        Ink::Dim,
        "and the row the caret waits in is indented in the dim ink, wearing no sigil of its own"
    );
}

#[test]
fn a_placeholder_and_a_waiting_prompt_light_no_sigil_and_keep_the_dim_ink() {
    let palette = Palette::TRUECOLOR;
    let mut empty = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::none(),
    );
    let (buffer, told) = painted_and_told(&mut empty, 120, 40);
    let band = laid_out(&told).composer;
    assert_eq!(
        gutter_ink(&buffer, &palette, band.x + 1, band.y + ends(&empty).above),
        Ink::Dim,
        "a draft holding nothing lights no sigil, so the placeholder keeps the dim ink"
    );

    let (buffer, laid, ends) = waiting_behind(palette, "a draft", 1, 120, 24);
    let prompts = inside(laid.queue);
    assert_eq!(
        gutter_ink(&buffer, &palette, prompts.x, prompts.y),
        Ink::Dim,
        "a waiting prompt is no row the caret waits in, so its sigil keeps the dim ink"
    );
    assert_eq!(
        gutter_ink(
            &buffer,
            &palette,
            laid.composer.x + 1,
            laid.composer.y + ends.above
        ),
        Ink::Accent,
        "and the draft above it is the row the caret waits in, so its sigil is the one that is lit"
    );
}

#[test]
fn every_palette_draws_the_inks_of_a_draft_row_in_its_own_colours() {
    let root = tempfile::tempdir().expect("a temp workspace");
    std::fs::create_dir_all(root.path().join("src")).expect("the directories a file sits in");
    std::fs::write(root.path().join("src/rows.rs"), "").expect("a file");

    for palette in [Palette::SLOTS, Palette::ANSI, Palette::TRUECOLOR] {
        let mut view = View::of(
            inked(palette),
            Facts::default()
                .asked()
                .working(root.path().to_path_buf(), None),
            Transcript::none(),
        );
        view.paste("/model src/rows.rs `a span` plain");
        // A draft that names a command opens the commands card, whose scrim would dim every cell.
        view.key(Key::Escape);
        let (buffer, told) = painted_and_told(&mut view, 120, 40);
        let band = laid_out(&told).composer;
        let (words, at) = place_of(&buffer, "/model src/rows.rs `a span` plain");
        let column = |word: &str| {
            at.saturating_add(pen::cells(
                &words[..words.find(word).expect("a word of the draft row")],
            ))
        };

        for (word, ink) in [
            ("/model", Ink::Accent),
            ("src/rows.rs", Ink::Ok),
            ("`a span`", Ink::Muted),
            ("plain", Ink::Plain),
        ] {
            assert_eq!(
                look_at(&buffer, &palette, column(word), band.y + ends(&view).above),
                Look::plain(ink),
                "{palette:?} draws {word:?} in the {} ink, of the inks it holds itself",
                ink.name()
            );
        }
    }
}

#[test]
fn an_empty_drafts_placeholder_is_drawn_whole_with_no_cell_of_it_reversed() {
    let palette = Palette::TRUECOLOR;
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::none(),
    );

    let (buffer, told) = painted_and_told(&mut view, 120, 24);
    let band = laid_out(&told).composer;
    let (x, y) = caret_cell(&buffer, &palette, band).expect("an empty draft's caret has a cell");
    let (words, at) = place_of(&buffer, PLACEHOLDER);

    assert_eq!(
        y,
        band.y + ends(&view).above,
        "the caret waits on the first row of the draft"
    );
    assert_eq!(
        x,
        band.x + pen::cells(INDENT),
        "and on the blank cell of the gutter, one column short of where the words start"
    );
    assert_eq!(
        x + 1,
        at,
        "so the block stands beside the placeholder and never over it: {words:?}"
    );
    for cell in at..at + pen::cells(PLACEHOLDER) {
        assert_eq!(
            look_at(&buffer, &palette, cell, y),
            Look::plain(Ink::Dim),
            "column {cell} of the placeholder is not the dim ink, so a cell of it is drawn over"
        );
    }
    assert_eq!(
        look_at(&buffer, &palette, x, y),
        Look::plain(Ink::Mark),
        "the cell reads back as the mark alone, so it is a block on the band rather than a bar"
    );
    assert_eq!(
        under(&buffer, x, y),
        palette.step(ends(&view).above, band.height),
        "and it keeps the ground the row it stands on is ramped in"
    );
}

#[test]
fn the_caret_is_placed_before_the_glyph_it_comes_before_and_moves_with_every_key() {
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::none(),
    );
    view.paste("one\ntwo");

    let placed = |view: &mut View| -> (u16, u16, String) {
        let (buffer, told) = painted_and_told(view, 120, 24);
        let band = laid_out(&told).composer;
        let (x, y) = caret_of(&told).expect("a draft that takes the keys places the caret");
        let glyph = buffer
            .cell((x, y))
            .expect("a cell of the caret")
            .symbol()
            .to_owned();
        let above = ends(view).above;
        (x - band.x - 1, y - band.y - above, glyph)
    };
    let column = pen::cells(INDENT);

    assert_eq!(
        placed(&mut view),
        (column + 3, 1, String::from(" ")),
        "the caret is placed one column past the last glyph of the row it waits at the end of"
    );
    view.key(Key::Left);
    assert_eq!(
        placed(&mut view),
        (column + 2, 1, String::from("o")),
        "a key that takes the caret back places it over the glyph it now comes before"
    );
    view.key(Key::Up);
    assert_eq!(
        placed(&mut view),
        (column + 2, 0, String::from("e")),
        "and a key that takes it up a row places it on the glyph that stands there"
    );
    view.key(Key::Char('X'));
    assert_eq!(
        placed(&mut view),
        (column + 3, 0, String::from("e")),
        "a glyph typed in front of the caret leaves it before the glyph it held"
    );
    view.key(Key::Newline);
    assert_eq!(
        placed(&mut view),
        (column, 1, String::from("e")),
        "and the row a newline gives the draft opens with the caret placed on its first column"
    );
}

#[test]
fn the_band_alone_places_the_caret_and_no_answer_or_prompt_does() {
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().running(),
        Transcript::of((1..=3).map(|turn| Entry::Turn { turn })),
    );
    view.enqueue(prompt("a prompt behind the draft"));
    view.paste("a draft");

    let (_, told) = painted_and_told(&mut view, 120, 24);
    let laid = laid_out(&told);
    let (x, row) = caret_of(&told).expect("the draft's caret is placed");
    let at = Position::new(x, row);

    assert!(
        (laid.composer.y + ends(&view).above..laid.queue.y).contains(&row),
        "the caret is placed on a row of the draft, above the prompts that wait under it: row {row}"
    );
    assert!(
        !laid.queue.contains(at),
        "no prompt that waits behind the draft has the caret placed on it: {at:?}"
    );
    assert!(
        !laid.transcript.contains(at),
        "and no answer or turn of the transcript does: {at:?}"
    );
}
