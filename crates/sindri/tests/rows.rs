mod frame;

use std::path::PathBuf;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

use sindri::capability::Capabilities;
use sindri::composer::{Key, Outcome};
use sindri::facts;
use sindri::graphics::CellSize;
use sindri::ink::{Ink, Look};
use sindri::ledger::Boot;
use sindri::link::Links;
use sindri::markdown;
use sindri::palette::{Band, Palette};
use sindri::pen;
use sindri::picture::Surface;
use sindri::pulse::Pulse;
use sindri::rows::{Fence, Measure, Row, Rows, Scroll, Sideways, Window, layout, layout_on};
use sindri::size::Sizes;
use sindri::view::{Transcript, View};
use sindri_core::config::Config;
use sindri_core::open::Ground;
use sindri_core::policy::{Boundary, Enforcement};
use sindri_core::provider::Deltas;
use sindri_core::records::{Entry, Facts, Record, read_transcript, transcript_path};
use sindri_core::redact::SecretSet;
use sindri_core::run::{Budget, Exit, Limits, Stop, Turn};
use sindri_core::session::Session;
use sindri_core::wire::Wire;

fn thought(which: u32) -> Entry {
    Entry::Reasoning {
        turn: which,
        text: SecretSet::default().redact(format!("turn {which}")),
    }
}

fn answer(which: u32) -> Entry {
    Entry::Assistant {
        turn: which,
        text: SecretSet::default().redact(
            "The closing fence is what ends a code block, so a fence still streaming is \
             rendered as one until that line arrives: a half-written diagram never costs \
             a raster pass and never flickers while it is being typed.",
        ),
    }
}

fn transcript() -> Vec<Entry> {
    let mut entries = Vec::new();
    for which in 1..=3 {
        entries.push(thought(which));
        entries.push(answer(which));
    }
    entries
}

fn rows_shown(window: &Window) -> u16 {
    window.pieces().iter().map(|piece| piece.rows).sum()
}

#[test]
fn a_width_change_invalidates_the_cache_and_reflows_once() {
    let mut rows = Rows::new();
    for entry in transcript() {
        rows.push(entry);
    }

    rows.measure(Measure::new(80, Surface::none(), None));
    let wide = rows.window(Scroll::Tail, 6);
    let shown = wide.pieces().len();
    assert!(
        shown > 0,
        "a window at the tail shows the end of the transcript"
    );
    assert_eq!(rows.lays(), shown, "the first pass lays out what it shows");

    let again = rows.window(Scroll::Tail, 6);
    assert_eq!(again.pieces().len(), shown);
    assert_eq!(
        rows.lays(),
        shown,
        "a paint at an unchanged width re-lays nothing"
    );

    let newest = rows.len() - 1;
    let wide_rows = rows.rows(newest).len();

    rows.measure(Measure::new(40, Surface::none(), None));
    let narrow = rows.window(Scroll::Tail, 6);
    let reflowed = narrow.pieces().len();
    assert!(reflowed > 0);
    assert_eq!(
        rows.lays(),
        shown + reflowed,
        "a width change lays each visible entry out once"
    );
    let narrow_rows = rows.rows(newest).len();
    assert!(
        narrow_rows > wide_rows,
        "the answer wraps into more rows at 40 columns than at 80: {narrow_rows} against {wide_rows}"
    );
    assert_eq!(
        rows.lays(),
        shown + reflowed,
        "reading the rows costs nothing extra"
    );

    rows.measure(Measure::new(80, Surface::none(), None));
    let restored = rows.window(Scroll::Tail, 6).pieces().len();
    assert_eq!(rows.lays(), shown + reflowed + restored);
}

#[test]
fn a_surface_that_changed_relays_the_band_once_and_one_that_did_not_relays_nothing() {
    let mut rows = Rows::new();
    rows.push(Entry::Assistant {
        turn: 1,
        text: SecretSet::default().redact("```mermaid\nflowchart TD\n  A-->B\n```"),
    });
    rows.measure(Measure::new(80, Surface::none(), None));
    rows.window(Scroll::Tail, 10);

    let words = rows.rows(0).len();
    assert_eq!(
        rows.lays(),
        1,
        "the fence is laid out once, at the width the band draws it at"
    );
    assert!(
        rows.rows(0).iter().all(|row| row.picture().is_none()),
        "and a terminal that places no picture is given the fence as the code it is"
    );

    rows.measure(Measure::new(80, Surface::none(), None));
    rows.window(Scroll::Tail, 10);
    assert_eq!(
        rows.lays(),
        1,
        "a surface that changed nothing drops no row and lays none out again"
    );

    rows.measure(Measure::new(
        80,
        Surface::of(None, CellSize::new(10, 20), true, false),
        None,
    ));
    rows.window(Scroll::Tail, 10);
    assert_eq!(
        rows.lays(),
        2,
        "a surface that changed drops what the cache held and lays the band out once"
    );

    let picture = rows
        .rows(0)
        .iter()
        .find_map(|row| row.picture())
        .map(|picture| picture.rows());
    let block = rows.rows(0).len();
    let picture = picture.expect("the fence is a diagram on a terminal that places pictures");
    assert!(
        block >= usize::from(picture),
        "and the rows laid are the diagram's block, as tall as the picture placed over it: \
         {block} rows of the answer, {picture} of the picture"
    );
    assert!(
        block >= words,
        "which is the taller of the two: the fence's own {words} rows are under the picture"
    );

    rows.rows(0);
    assert_eq!(
        rows.lays(),
        2,
        "and reading the rows back after it costs no third layout"
    );
}

#[test]
fn a_measure_the_cache_holds_lays_nothing_and_reflows_nothing() {
    let mut rows = Rows::new();
    for entry in transcript() {
        rows.push(entry);
    }

    let reflowed = rows.measure(Measure::new(80, Surface::none(), None));
    assert!(
        reflowed,
        "no band has been handed to a fresh cache yet, so the first measure is a reflow"
    );
    let shown = rows.window(Scroll::Tail, 6).pieces().len();
    assert!(
        shown > 0,
        "a window at the tail shows the end of the transcript"
    );
    assert_eq!(rows.lays(), shown, "the first pass lays out what it shows");

    let reflowed = rows.measure(Measure::new(80, Surface::none(), None));
    assert!(
        !reflowed,
        "a measure the cache already holds is a band that did not reflow"
    );
    let again = rows.window(Scroll::Tail, 6).pieces().len();
    assert_eq!(again, shown, "and the same entries are the window");
    assert_eq!(
        rows.lays(),
        shown,
        "a measure at the facts already held lays nothing"
    );
}

#[test]
fn a_measure_whose_width_moved_drops_every_row_and_reflows_once() {
    let mut rows = Rows::new();
    for entry in transcript() {
        rows.push(entry);
    }

    rows.measure(Measure::new(80, Surface::none(), None));
    let wide = rows.window(Scroll::Tail, 6).pieces().len();
    assert!(
        wide > 0,
        "a window at the tail shows the end of the transcript"
    );
    assert_eq!(rows.lays(), wide, "the first pass lays out what it shows");

    let reflowed = rows.measure(Measure::new(40, Surface::none(), None));
    assert!(
        reflowed,
        "a measure whose width moved is a band that reflowed, and says so"
    );
    let narrow = rows.window(Scroll::Tail, 6).pieces().len();
    assert!(narrow > 0, "and the band is drawn again at the new width");
    assert_eq!(
        rows.lays(),
        wide + narrow,
        "the one call drops every laid row and lays each visible entry out once"
    );

    let newest = rows.len() - 1;
    rows.rows(newest);
    assert_eq!(
        rows.lays(),
        wide + narrow,
        "and reading them back costs no third layout"
    );
}

#[test]
fn a_measure_whose_surface_or_ground_moved_drops_every_row_without_a_reflow() {
    let tree = tempfile::tempdir().expect("a temp workspace");
    let drawn = Surface::of(None, CellSize::new(10, 20), true, false);

    let mut rows = Rows::new();
    for entry in transcript() {
        rows.push(entry);
    }
    rows.measure(Measure::new(80, Surface::none(), None));
    let plain = rows.window(Scroll::Tail, 6).pieces().len();
    assert_eq!(
        rows.lays(),
        plain,
        "the band is laid out once, at the surface the measure gave it"
    );

    let reflowed = rows.measure(Measure::new(80, drawn.clone(), None));
    assert!(
        !reflowed,
        "a measure whose surface moved is no reflow: the width did not move"
    );
    let retuned = rows.window(Scroll::Tail, 6).pieces().len();
    assert_eq!(
        rows.lays(),
        plain + retuned,
        "a surface that moved drops every laid row and lays each visible entry out once"
    );

    let mut standing = Rows::new();
    for entry in transcript() {
        standing.push(entry);
    }
    standing.measure(Measure::new(80, drawn.clone(), None));
    let ungrounded = standing.window(Scroll::Tail, 6).pieces().len();
    assert_eq!(
        standing.lays(),
        ungrounded,
        "the band is laid out once, before a ground is handed to it"
    );

    let reflowed = standing.measure(Measure::new(80, drawn, Some(&ground(tree.path()))));
    assert!(
        !reflowed,
        "a measure whose ground moved is no reflow: the width did not move"
    );
    let stood = standing.window(Scroll::Tail, 6).pieces().len();
    assert_eq!(
        standing.lays(),
        ungrounded + stood,
        "a ground that moved drops every laid row and lays each visible entry out once"
    );
}

#[test]
fn a_measure_that_moves_one_fact_lays_once_and_a_repeat_lays_nothing() {
    let tree = tempfile::tempdir().expect("a temp workspace");
    let drawn = Surface::of(None, CellSize::new(10, 20), true, false);
    let stood = ground(tree.path());
    let mut rows = Rows::new();
    for entry in transcript() {
        rows.push(entry);
    }

    rows.measure(Measure::new(80, Surface::none(), None));
    let wide = rows.window(Scroll::Tail, 6).pieces().len();
    assert_eq!(
        rows.lays(),
        wide,
        "the band is laid out once at the width it was given"
    );

    rows.measure(Measure::new(80, Surface::none(), None));
    assert_eq!(
        rows.lays(),
        wide,
        "a width the cache already holds drops nothing and lays nothing"
    );

    rows.measure(Measure::new(40, Surface::none(), None));
    let narrow = rows.window(Scroll::Tail, 6).pieces().len();
    assert_eq!(
        rows.lays(),
        wide + narrow,
        "a measure that moves the width reflows once"
    );

    rows.measure(Measure::new(40, drawn.clone(), None));
    let retuned = rows.window(Scroll::Tail, 6).pieces().len();
    assert_eq!(
        rows.lays(),
        wide + narrow + retuned,
        "a measure that moves the surface lays the band out once"
    );
    rows.measure(Measure::new(40, drawn.clone(), None));
    assert_eq!(
        rows.lays(),
        wide + narrow + retuned,
        "a surface the cache already holds lays nothing"
    );

    rows.measure(Measure::new(40, drawn.clone(), Some(&stood)));
    let grounded = rows.window(Scroll::Tail, 6).pieces().len();
    assert_eq!(
        rows.lays(),
        wide + narrow + retuned + grounded,
        "a measure that moves the ground lays the band out once"
    );
    rows.measure(Measure::new(40, drawn, Some(&stood)));
    assert_eq!(
        rows.lays(),
        wide + narrow + retuned + grounded,
        "a ground the cache already holds lays nothing"
    );
}

#[test]
fn scrolling_the_whole_transcript_lays_each_entry_out_once() {
    let history = 260;
    let mut rows = Rows::new();
    rows.measure(Measure::new(80, Surface::none(), None));
    for which in 1..=history {
        rows.push(thought(which));
    }

    let room = 40;
    let mut back = 0;
    loop {
        let window = rows.window(Scroll::Back(back), room);
        let expected = u32::from(room).min(history - back.min(history));
        assert_eq!(
            u32::from(rows_shown(&window)),
            expected,
            "the window {back} rows back shows exactly the rows it has"
        );
        assert!(
            rows.lays() <= history as usize,
            "an entry was laid out twice: {} layouts after {back} rows back",
            rows.lays()
        );
        let at_head = window
            .pieces()
            .first()
            .is_some_and(|piece| piece.entry == 0 && piece.skip == 0);
        if at_head {
            break;
        }
        back += u32::from(room);
        assert!(
            back < 4 * history,
            "the walk back never reached the head of the transcript"
        );
    }
    assert_eq!(
        rows.lays(),
        history as usize,
        "the whole history was laid out, one entry at a time"
    );

    for back in (0..=260).rev() {
        rows.window(Scroll::Back(back), room);
    }
    assert_eq!(
        rows.lays(),
        history as usize,
        "scrolling a cached history costs no layout at all"
    );
}

#[test]
fn an_entry_above_the_window_is_never_laid_out() {
    let mut rows = Rows::new();
    rows.measure(Measure::new(80, Surface::none(), None));
    for which in 1..=250 {
        rows.push(thought(which));
    }

    let window = rows.window(Scroll::Tail, 40);
    let shown = window.pieces().len();
    assert_eq!(
        shown, 40,
        "a one-row entry each, so the last 40 are the window"
    );
    assert_eq!(rows.lays(), shown, "and nothing above them was laid out");
    assert!(
        rows.lays() < 250,
        "a 250-entry transcript on a 40-row terminal pays for {} entries",
        rows.lays()
    );
}

#[test]
fn the_whole_height_costs_a_layout_only_for_what_it_has_not_seen() {
    let mut rows = Rows::new();
    rows.measure(Measure::new(80, Surface::none(), None));
    for which in 1..=250 {
        rows.push(thought(which));
    }
    rows.window(Scroll::Tail, 40);

    assert_eq!(rows.height(), 250);
    assert_eq!(
        rows.lays(),
        250,
        "measuring everything lays out everything it lacks"
    );
    assert_eq!(rows.height(), 250);
    assert_eq!(rows.lays(), 250, "and the second measurement is free");
}

#[test]
fn every_row_but_a_fences_line_fits_the_width_it_was_laid_out_for() {
    let secrets = SecretSet::default();
    let entries = [
        Entry::Turn { turn: 1000 },
        Entry::Call {
            turn: 1,
            id: None,
            name: secrets.redact("a-tool-whose-name-is-longer-than-the-band-it-is-drawn-in"),
            arguments: secrets.redact(""),
        },
        Entry::Result {
            turn: 1,
            id: None,
            name: secrets.redact("bash"),
            text: secrets.redact("unbreakableandunforgivingwordthatnospacecansplit\n日本語の答え"),
            bytes: u64::MAX,
            elapsed_ms: u64::MAX,
            exit: Some(Exit::Code(u8::MAX.into())),
        },
        Entry::Assistant {
            turn: 1,
            text: secrets.redact(""),
        },
        Entry::Prompt {
            text: secrets.redact("a prompt whose words are longer than the band it is drawn in"),
        },
        Entry::Reasoning {
            turn: 1,
            text: secrets
                .redact("a thought, in its own gutter\nand its own ink\n```\nand a fence\n"),
        },
        answer(1),
    ];

    for cols in [2, 3, 5, 6, 7, 8, 40, 120] {
        for entry in &entries {
            for row in layout(entry, cols) {
                if row.fence().is_some() {
                    continue;
                }
                let drawn = row.width();
                assert!(
                    drawn <= cols,
                    "a row {drawn} columns wide was laid out for {cols}: {row:?}"
                );
            }
        }

        let mut live = Rows::new();
        live.measure(Measure::new(cols, Surface::none(), None));
        let head = Pulse::live(4_200).head();
        live.text("a braille marker at the head of the newest answer, and its words");
        live.live(Some(&head));
        for row in live.rows(0) {
            let drawn = row.width();
            assert!(
                drawn <= cols,
                "a row {drawn} columns wide was laid out for the marker's answer at {cols}: {row:?}"
            );
        }
    }
}

fn answered(text: &str) -> Entry {
    Entry::Assistant {
        turn: 1,
        text: SecretSet::default().redact(text),
    }
}

const TWO_FENCES: &str = "```rust\nlet a_line_wider_than_the_band_it_is_drawn_in = 1;\n```\n\n\
                           between the two fences\n\n```rust\nlet short = 2;\n```";

fn walks(rows: &mut Rows, at: usize) -> Vec<(Option<usize>, u16)> {
    rows.rows(at)
        .iter()
        .map(|row| {
            (
                row.fence().map(Fence::id),
                row.fence().map_or(0, Fence::walk),
            )
        })
        .collect()
}

fn walked(rows: &[(Option<usize>, u16)], fence: usize) -> Vec<u16> {
    rows.iter()
        .filter(|(id, _)| *id == Some(fence))
        .map(|(_, walk)| *walk)
        .collect()
}

#[test]
fn a_walk_moves_every_row_of_one_fence_and_leaves_the_other_fence_and_the_other_entry_alone() {
    let mut rows = Rows::new();
    rows.measure(Measure::new(20, Surface::none(), None));
    rows.push(answered(TWO_FENCES));
    rows.push(answered(TWO_FENCES));
    let before = walks(&mut rows, 0);
    let beside = walks(&mut rows, 1);
    assert!(
        !walked(&before, 0).is_empty() && !walked(&before, 1).is_empty(),
        "the answer holds two fences to walk: {before:?}"
    );

    assert!(rows.sideways(0, 0, Sideways::Forward));
    let moved = walks(&mut rows, 0);
    assert_eq!(
        walked(&moved, 0),
        vec![8; walked(&before, 0).len()],
        "every row of the fence the reader walked moved by the one step: {moved:?}"
    );
    assert_eq!(
        walked(&moved, 1),
        vec![0; walked(&before, 1).len()],
        "and the fence beside it did not move"
    );
    assert_eq!(
        walks(&mut rows, 1),
        beside,
        "and no row of the entry beside it moved either"
    );
    assert!(rows.sideways(0, 0, Sideways::Back), "the walk goes back");
    assert_eq!(walks(&mut rows, 0), before, "and the fence is where it was");
}

#[test]
fn a_fence_walked_as_far_as_its_widest_line_goes_no_further() {
    let mut rows = Rows::new();
    rows.measure(Measure::new(20, Surface::none(), None));
    rows.push(answered(TWO_FENCES));
    let widest = rows
        .rows(0)
        .iter()
        .filter(|row| row.fence().map(Fence::id) == Some(0))
        .map(Row::width)
        .max()
        .expect("a fence with a line of it drawn");

    let mut steps = 0;
    while rows.sideways(0, 0, Sideways::Forward) {
        steps += 1;
        assert!(steps < 40, "the walk ran past the widest line of its fence");
    }
    assert!(steps > 0, "a fence wider than the band is walked at all");
    assert_eq!(
        walked(&walks(&mut rows, 0), 0).into_iter().max(),
        Some(widest.saturating_sub(20)),
        "the walk stops where the tail of the widest line stands at the band's edge"
    );
    assert!(
        !rows.sideways(0, 0, Sideways::Forward),
        "and a fence walked that far goes no further"
    );
}

#[test]
fn a_walk_outlives_a_line_laid_again_and_a_width_change_drops_it() {
    let mut rows = Rows::new();
    rows.measure(Measure::new(20, Surface::none(), None));
    rows.text("```rust\nlet a_line_wider_than_the_band_it_is_drawn_in = 1;\n");
    rows.live(None);
    assert!(
        rows.sideways(0, 0, Sideways::Forward),
        "the line the run is still writing is wider than the band, so the reader walks it"
    );

    rows.text("and a word more");
    rows.live(None);
    assert_eq!(
        walked(&walks(&mut rows, 0), 0).into_iter().max(),
        Some(8),
        "the fragment the wire sent laid the line again, and the reader's walk is still on it"
    );

    rows.measure(Measure::new(24, Surface::none(), None));
    assert!(
        walked(&walks(&mut rows, 0), 0)
            .iter()
            .all(|walk| *walk == 0),
        "a width change drops the walk with the rows it was made at"
    );
}

#[test]
fn the_readers_own_words_open_the_exchange_and_no_row_names_them() {
    let secrets = SecretSet::default();
    let rows = layout(
        &Entry::Prompt {
            text: secrets.redact("triage this"),
        },
        40,
    );

    assert_eq!(
        words(&rows[0]),
        "",
        "a prompt laid out on its own opens on a blank row: {:?}",
        rows[0]
    );
    assert_eq!(
        words(&rows[1]),
        "┃›triage this",
        "and its own words follow down the spine they are quoted by, with the sigil in the column \
         beside it"
    );
    assert_eq!(inks(&rows[1]), [Ink::Dim, Ink::Plain, Ink::Plain]);
    assert_eq!(
        rows.len(),
        2,
        "and nothing stands above the words but that row"
    );
    assert!(
        !rows.iter().any(|row| words(row).contains('─')),
        "no row of the exchange draws a rule: {:?}",
        rows.iter().map(words).collect::<Vec<String>>()
    );
}

#[test]
fn a_prompt_row_claims_its_inset_ground_and_leaves_the_right_margin_on_the_well() {
    let palette = Palette::TRUECOLOR;
    let area = Rect::new(0, 0, 40, 1);
    let well = Row::of([(String::from("a word of the conversation"), Ink::Muted)]);
    let asked =
        Row::of([(String::from("the reader's own words"), Ink::Plain)]).standing(Band::Prompt);

    assert_eq!(
        asked.stands_on(),
        Some(Band::Prompt),
        "a row handed a band stands on it"
    );
    assert_eq!(
        well.stands_on(),
        None,
        "and a row handed none stands on none"
    );

    for (row, ground) in [
        (&asked, palette.band(Band::Prompt)),
        (&well, palette.band(Band::Well)),
    ] {
        let mut buffer = Buffer::empty(area);
        pen::claim(&mut buffer, area, palette.ground(Some(Band::Well)));
        row.paint(
            area,
            &mut buffer,
            &palette,
            &mut Links::none(),
            &mut Sizes::none(),
        );
        for x in area.x..area.right() {
            let cell = buffer.cell((x, 0)).expect("a cell of the band");
            assert_eq!(
                cell.bg,
                if x >= area.right() - sindri::rows::MARGIN {
                    palette.band(Band::Well)
                } else {
                    ground
                },
                "column {x} of {row:?} is drawn on {:?}: a row handed a band claims the whole \
                 width of it before its words, and a row handed none is drawn exactly as it was, \
                 over the ground it found",
                cell.bg
            );
        }
    }
}

#[test]
fn a_sent_prompt_draws_its_spine_down_every_row_it_occupies() {
    let rows = layout(
        &Entry::Prompt {
            text: SecretSet::default().redact(
                "triage the retry test, and say which line of the patch the test disagrees with",
            ),
        },
        40,
    );
    let said: Vec<String> = rows.iter().map(words).collect();
    assert!(
        rows.len() > 2,
        "a prompt wider than the band is more than one row of words: {said:?}"
    );
    assert_eq!(
        rows[0].glyph_columns(),
        Vec::new(),
        "the row that separates the exchange from the one above it is a blank row of the well, \
         with no spine and no sigil: {:?}",
        rows[0]
    );

    for (at, row) in rows[1..].iter().enumerate() {
        assert_eq!(
            drawn_at(row, 0),
            Some('┃'),
            "row {at} of the prompt's words opens on the spine every row of them is quoted by: \
             {said:?}"
        );
        assert_eq!(
            words_column(row),
            Some(2),
            "and its words begin in the column an answer's prose begins in: {row:?}"
        );
        assert_eq!(
            inks(row),
            [Ink::Dim, Ink::Plain, Ink::Plain],
            "the spine is drawn dim and the reader's own words in the plain ink: {row:?}"
        );
    }
    assert_eq!(
        drawn_at(&rows[1], 1),
        Some('›'),
        "the sigil stands in the column beside the spine on the row the prompt is asked on: {:?}",
        rows[1]
    );
    for (at, row) in rows[2..].iter().enumerate() {
        assert_eq!(
            drawn_at(row, 1),
            Some(' '),
            "and every row under it holds a blank in that column, so the prompt reads as one \
             quoted block: row {at} is {row:?}"
        );
    }
}

#[test]
fn no_row_of_an_answer_a_call_a_result_or_a_stop_draws_a_spine_in_its_first_column() {
    let entries = [
        answer(1),
        call(1),
        result(1, Exit::Code(0)),
        result(1, Exit::Code(1)),
        stop_entry(Stop::Cancelled),
        stop_entry(failed()),
    ];

    for entry in entries {
        for row in layout(&entry, 40) {
            assert_ne!(
                drawn_at(&row, 0),
                Some('┃'),
                "a row of {entry:?} draws the spine a prompt's words are quoted by, and the \
                 spine belongs to the reader's own words alone: {row:?}"
            );
        }
    }
}

#[test]
fn the_prompt_stands_on_the_prompt_band_and_no_other_row_stands_on_one() {
    let secrets = SecretSet::default();
    for cols in [40, 8, 3, 2] {
        let rows = layout(
            &Entry::Prompt {
                text: secrets.redact(
                    "triage the retry test, and say which line of the patch it disagrees with",
                ),
            },
            cols,
        );
        assert_eq!(
            rows[0].stands_on(),
            None,
            "the blank row that separates an exchange from the one above it is a row of the well \
             and never of the prompt at {cols} columns: {:?}",
            rows[0]
        );
        for row in &rows[1..] {
            assert_eq!(
                row.stands_on(),
                Some(Band::Prompt),
                "every row the prompt's words occupy stands on the prompt band at {cols} \
                 columns, a band too narrow for the gutter included: {row:?}"
            );
        }
    }

    for entry in every_kind(&secrets) {
        if matches!(entry, Entry::Prompt { .. }) {
            continue;
        }
        for row in layout(&entry, 40) {
            assert_eq!(
                row.stands_on(),
                None,
                "a row of {entry:?} stands on the ground the band it is read in lays under it, \
                 and a band of its own is what the reader's own words alone wear: {row:?}"
            );
        }
    }
}

fn band(entries: Vec<Entry>, cols: u16, rows: u16) -> Vec<String> {
    let mut view = View::of(
        Capabilities::default(),
        facts::Facts::default().asked(),
        Transcript::of(entries),
    );
    above_the_composer(&mut view, cols, rows)
}

fn above_the_composer(view: &mut View, cols: u16, rows: u16) -> Vec<String> {
    let frame = frame::painted(view, cols, rows);
    let composer = frame
        .told()
        .layout()
        .expect("a pass that drew a frame hands back the frame it laid out")
        .composer;
    frame.words()[..usize::from(composer.y)].to_vec()
}

#[test]
fn a_prompt_opens_under_a_blank_row_unless_it_opens_the_band() {
    let turn = |said: &str| Entry::Assistant {
        turn: 1,
        text: SecretSet::default().redact(said),
    };
    let drawn = band(
        vec![
            asked_for(1),
            turn("one answer"),
            asked_for(2),
            call(2),
            timed(2, 4_100),
            call(2),
            timed(2, 100),
            call(2),
            timed(2, 100),
            asked_for(3),
            turn("three answer"),
            stop_entry(Stop::Cancelled),
            asked_for(4),
            turn("four answer"),
        ],
        60,
        24,
    );
    let whole = drawn.join("\n");
    let prompt = |which: u32| {
        let said = format!("┃›prompt {which}");
        drawn
            .iter()
            .position(|row| row.contains(&said))
            .unwrap_or_else(|| panic!("the band draws {said:?}: {whole}"))
    };

    assert_eq!(
        prompt(1),
        0,
        "the first exchange of a conversation has nothing above it to be separated from: {whole}"
    );
    assert_eq!(
        drawn[0], "┃›prompt 1",
        "so the prompt's own words open the band: {whole}"
    );
    for (which, after) in [
        (2, "an answer"),
        (3, "a settled work of one row"),
        (4, "a stop"),
    ] {
        assert_eq!(
            drawn[prompt(which) - 1],
            "",
            "the prompt that stands after {after} has one blank row above it: {whole}"
        );
        assert_ne!(
            drawn[prompt(which) - 2],
            "",
            "and no more than one: {whole}"
        );
    }
    assert_eq!(
        drawn[prompt(2) + 1],
        "⋯ 3 calls · 4.3 s",
        "and no blank row stands between a prompt and the work it asked: {whole}"
    );
    assert_eq!(
        drawn[prompt(3) - 2],
        "⋯ 3 calls · 4.3 s",
        "the work the second prompt asked is the one row it settled as, and the blank row under \
         it is the third prompt's own: {whole}"
    );
    assert!(
        !drawn.iter().any(|row| row.starts_with('─')),
        "no row of the band opens on a rule of its own: {whole}"
    );
}

#[test]
fn a_turn_draws_no_row_at_any_width() {
    for cols in [120, 40, 20, 8, 3, 1] {
        let drawn = layout(&Entry::Turn { turn: 3 }, cols);
        assert!(
            drawn.is_empty(),
            "a turn is no row of the exchange at {cols} columns: {drawn:?}"
        );
    }
}

fn settled_frame(entries: Vec<Entry>, stop: &Stop, cols: u16, rows: u16) -> Vec<String> {
    let mut view = View::of(
        Capabilities::default(),
        facts::Facts::default()
            .asked()
            .pulsed(Some(Pulse::settled(stop, 1_200))),
        Transcript::of(entries),
    );
    view.stopped(stop);
    above_the_composer(&mut view, cols, rows)
}

#[test]
fn three_exchanges_are_three_prompts_and_three_answers_and_no_row_saying_a_run_ended() {
    let secrets = SecretSet::default();
    let mut entries = Vec::new();
    for turn in 1..=3 {
        entries.push(Entry::Prompt {
            text: secrets.redact(format!("prompt {turn}")),
        });
        entries.push(Entry::Turn { turn });
        entries.push(Entry::Assistant {
            turn,
            text: secrets.redact(format!("answer {turn}")),
        });
        entries.push(Entry::Stop { stop: Stop::Final });
    }
    let drawn = settled_frame(entries, &Stop::Final, 60, 20);
    let whole = drawn.join("\n");

    let asked = drawn
        .iter()
        .filter(|row| row.starts_with("┃›prompt "))
        .count();
    assert_eq!(
        asked, 3,
        "a conversation of three prompts opens on three prompts: {whole}"
    );
    assert!(
        !drawn.iter().any(|row| row.starts_with('─')),
        "and no row of the band opens on a rule of its own: {whole}"
    );
    for turn in 1..=3 {
        assert_eq!(
            drawn
                .iter()
                .filter(|row| row.contains(&format!("answer {turn}")))
                .count(),
            1,
            "and each answer the run gave stands once: {whole}"
        );
    }
    let ended = ["turn", "final", "cancelled", "max_turns", "budget", "error"];
    assert!(
        !drawn.iter().any(|row| ended
            .iter()
            .any(|word| row.contains(word) && row.starts_with(&['─', '✓', '✗'][..]))),
        "no row of the band says a run ended: {whole}"
    );
    let answer = drawn
        .iter()
        .position(|row| row == "✓ 1.2s answer 3")
        .unwrap_or_else(|| panic!("the last answer wears the check the run ended on: {whole}"));
    assert_eq!(
        drawn[answer..]
            .iter()
            .take_while(|row| !row.is_empty())
            .count(),
        1,
        "and no row is drawn under it, so the answer is the last row of the exchange: {whole}"
    );
}

#[test]
fn the_stop_row_and_the_mark_at_the_head_of_the_answer_say_one_word() {
    let secrets = SecretSet::default();
    for stop in [
        Stop::MaxTurns,
        Stop::Budget(Budget::Clock),
        Stop::Cancelled,
        failed(),
    ] {
        let entries = vec![
            Entry::Prompt {
                text: secrets.redact("triage this"),
            },
            Entry::Assistant {
                turn: 1,
                text: secrets.redact("the retry test is the question"),
            },
        ];
        let drawn = settled_frame(entries, &stop, 60, 12);
        let whole = drawn.join("\n");
        let word = stop.word();

        assert!(
            drawn
                .iter()
                .any(|row| row.starts_with(&format!("✗ {word} "))
                    && row.contains("the retry test is the question")),
            "{word} is on the mark at the head of the answer it ended: {whole}"
        );
        assert!(
            drawn.iter().any(|row| row == &format!("✗ {word}")),
            "and under it the stop says the same word: {whole}"
        );
    }
}

#[test]
fn a_final_stop_lays_no_row_where_the_answer_it_ended_on_wears_the_check() {
    let secrets = SecretSet::default();
    let entries = vec![Entry::Assistant {
        turn: 1,
        text: secrets.redact("the retry test is the question"),
    }];
    let drawn = settled_frame(entries, &Stop::Final, 60, 8);
    let whole = drawn.join("\n");

    assert!(
        !drawn.iter().any(|row| row.contains("final")),
        "a run that ended on its answer says no row about ending: {whole}"
    );
    assert!(
        drawn
            .iter()
            .any(|row| row == "✓ 1.2s the retry test is the question"),
        "the answer stands under the check and the time the run took: {whole}"
    );
}

#[test]
fn a_run_that_drew_no_answer_draws_the_row_of_the_stop_it_ended_on() {
    let drawn = settled_frame(Vec::new(), &Stop::Cancelled, 60, 8);
    let whole = drawn.join("\n");

    assert!(
        drawn.iter().any(|row| row == "✗ cancelled"),
        "a run stopped before it answered has no answer to wear its mark, so the stop is a row \
         of its own: {whole}"
    );
}

fn words_column(row: &Row) -> Option<u16> {
    row.glyph_columns()
        .into_iter()
        .find(|(_, glyph, _)| glyph.is_alphanumeric() || *glyph == '•')
        .map(|(column, _, _)| column)
}

fn every_kind(secrets: &SecretSet) -> Vec<Entry> {
    vec![
        Entry::Prompt {
            text: secrets.redact("triage the retry test"),
        },
        Entry::Call {
            turn: 1,
            id: None,
            name: secrets.redact("bash"),
            arguments: secrets.redact("ls -la"),
        },
        result(1, Exit::Code(0)),
        Entry::Reasoning {
            turn: 2,
            text: secrets.redact("the fence is the question"),
        },
        Entry::Assistant {
            turn: 2,
            text: secrets
                .redact("Here is the rule:\n```rust\nfn main() {\n}\n```\nand the prose goes on."),
        },
    ]
}

#[test]
fn every_entrys_words_begin_in_one_column() {
    let secrets = SecretSet::default();
    for cols in [120, 80, 40, 20, 8, 3] {
        for entry in every_kind(&secrets) {
            let mut said = 0;
            for row in layout(&entry, cols) {
                let Some(column) = words_column(&row) else {
                    continue;
                };
                said += 1;
                assert_eq!(
                    column, 2,
                    "a row of {entry:?} at {cols} columns begins its words at {column}: {row:?}"
                );
            }
            assert!(said > 0, "{entry:?} at {cols} columns says no word at all");
        }
    }
}

fn drawn_at(row: &Row, column: u16) -> Option<char> {
    row.glyph_columns()
        .into_iter()
        .find(|(at, _, _)| *at == column)
        .map(|(_, glyph, _)| glyph)
}

#[test]
fn the_marker_that_says_what_a_row_is_stands_in_the_gutter() {
    let secrets = SecretSet::default();
    let prompt = layout(
        &Entry::Prompt {
            text: secrets.redact("triage this"),
        },
        40,
    );
    let flight = layout(&call(1), 60);
    let ok = layout(&result(1, Exit::Code(0)), 60);
    let failed = layout(&result(1, Exit::Code(1)), 60);
    let thought = layout(
        &Entry::Reasoning {
            turn: 1,
            text: secrets.redact("a thought"),
        },
        60,
    );
    let answer = layout(
        &Entry::Assistant {
            turn: 2,
            text: secrets.redact("prose\n```rust\nfn main()\n```\n"),
        },
        60,
    );
    let block = answer
        .iter()
        .find(|row| drawn_at(row, 0) == Some('▏'))
        .expect("the fence is rows of its own");

    let marked = [
        (&flight[0], '…'),
        (&ok[0], '✓'),
        (&failed[0], '✗'),
        (&thought[0], '│'),
        (&answer[0], ' '),
        (block, '▏'),
    ];
    for (row, marker) in marked {
        assert_eq!(
            drawn_at(row, 0),
            Some(marker),
            "the marker {marker:?} is not the row's first column: {row:?}"
        );
        assert_eq!(
            drawn_at(row, 1),
            Some(' '),
            "the gutter is two cells, one glyph and one blank: {row:?}"
        );
        assert_eq!(
            words_column(row),
            Some(2),
            "and the words begin past it: {row:?}"
        );
    }

    assert_eq!(
        drawn_at(&prompt[1], 0),
        Some('┃'),
        "the reader's own words are quoted by a spine of their own rather than set in the gutter \
         an answer is: {:?}",
        prompt[1]
    );
    assert_eq!(
        drawn_at(&prompt[1], 1),
        Some('›'),
        "and the sigil stands in the column beside it: {:?}",
        prompt[1]
    );
    assert_eq!(
        words_column(&prompt[1]),
        Some(2),
        "so the prompt's words begin where an answer's prose does: {:?}",
        prompt[1]
    );
}

#[test]
fn a_row_too_narrow_for_a_marker_gives_up_the_marker_before_it_cuts_a_word() {
    let secrets = SecretSet::default();
    let thought = Entry::Reasoning {
        turn: 1,
        text: secrets.redact("thought"),
    };
    for (entry, marker) in [
        (result(1, Exit::Code(0)), '✓'),
        (result(1, Exit::Code(1)), '✗'),
        (call(1), '…'),
        (thought, '│'),
        (
            Entry::Assistant {
                turn: 1,
                text: secrets.redact("```rust\nfn main()\n```"),
            },
            '▏',
        ),
    ] {
        for row in layout(&entry, 2) {
            let Some((column, _, _)) = row.glyph_columns().first().copied() else {
                continue;
            };
            assert_eq!(column, 0, "a band of two columns holds no gutter: {row:?}");
            assert!(
                !row.glyphs().contains(marker),
                "the marker {marker:?} is given up rather than the words: {row:?}"
            );
        }

        for row in layout(&entry, 2) {
            if row.fence().is_some() {
                continue;
            }
            assert!(
                row.width() <= 2,
                "a row of two columns holds no word past them: {row:?}"
            );
        }

        let narrow = layout(&entry, 3);
        let head = narrow
            .iter()
            .find(|row| !row.glyphs().trim().is_empty())
            .unwrap_or_else(|| panic!("three columns hold a word: {narrow:?}"));
        assert_eq!(
            drawn_at(head, 0),
            Some(marker),
            "the first row of words holds the marker {marker:?}: {head:?}"
        );
        assert_eq!(drawn_at(head, 1), Some(' '), "and its blank: {head:?}");
        assert_eq!(
            head.width(),
            3,
            "with one column of words at 2 and no more: {head:?}"
        );
        assert_eq!(
            words_column(head),
            Some(2),
            "the words follow the marker: {head:?}"
        );
    }

    let prompt = Entry::Prompt {
        text: secrets.redact("prompt"),
    };
    for row in layout(&prompt, 2) {
        assert!(
            !row.glyphs().contains('┃') && !row.glyphs().contains('›'),
            "a band of two columns gives up the spine and the sigil rather than a word: {row:?}"
        );
    }
    let narrow = layout(&prompt, 3);
    let head = &narrow[1];
    assert_eq!(
        drawn_at(head, 0),
        Some('┃'),
        "three columns hold the spine of a prompt beside its sigil and one word: {head:?}"
    );
    assert_eq!(
        drawn_at(head, 1),
        Some('›'),
        "and the sigil stands in the column past the spine: {head:?}"
    );
    assert_eq!(
        words_column(head),
        Some(2),
        "so the words begin where an answer's prose does: {head:?}"
    );
}

#[test]
fn a_marker_no_band_holds_beside_a_word_leaves_the_answer_its_columns() {
    let head = Pulse::live(4_200).head();
    let mut rows = Rows::new();
    rows.text("the answer's own words");

    for cols in 6..=7 {
        rows.measure(Measure::new(cols, Surface::none(), None));
        rows.live(Some(&head));
        let marked: Vec<String> = rows.rows(0).iter().map(words).collect();
        rows.live(None);
        let bare: Vec<String> = rows.rows(0).iter().map(words).collect();
        assert_eq!(
            marked, bare,
            "a marker {cols} columns cannot hold beside a word of the answer takes no column of it"
        );
    }
}

#[test]
fn a_code_block_keeps_its_bar_in_the_gutter() {
    let rows = layout(
        &Entry::Assistant {
            turn: 1,
            text: SecretSet::default().redact(
                "The rule is one line:\n```rust\nfn main() {\nlet x = 1;\n}\n```\nand back to prose.",
            ),
        },
        40,
    );

    assert_eq!(
        rows.iter().map(words).collect::<Vec<String>>(),
        [
            "  The rule is one line:",
            "",
            "▏ rust",
            "▏ fn main() {",
            "▏ let x = 1;",
            "▏ }",
            "",
            "  and back to prose.",
        ],
        "the bar of a block stands in the answer's gutter and never past it"
    );
    for row in &rows[2..6] {
        assert_eq!(drawn_at(row, 0), Some('▏'), "the block's own bar: {row:?}");
        assert_eq!(drawn_at(row, 1), Some(' '), "and its blank: {row:?}");
        assert!(
            drawn_at(row, 2).is_some(),
            "so the code's words begin where the answer's do: {row:?}"
        );
    }
    assert_eq!(
        words_column(&rows[7]),
        Some(2),
        "and the prose the fence closed under begins there too: {:?}",
        rows[7]
    );
}

#[test]
fn a_thought_keeps_its_bar_in_the_gutter_and_its_own_ink() {
    let rows = layout(
        &Entry::Reasoning {
            turn: 1,
            text: SecretSet::default()
                .redact("the fence is the question: a fence still open is code"),
        },
        24,
    );

    assert!(rows.len() > 1, "a thought wider than the band is rows");
    for row in &rows {
        assert_eq!(drawn_at(row, 0), Some('│'), "the bar of a thought: {row:?}");
        assert_eq!(drawn_at(row, 1), Some(' '), "and its blank: {row:?}");
        assert_eq!(
            words_column(row),
            Some(2),
            "the thought's words stand behind the bar and not past it: {row:?}"
        );
        assert_eq!(
            inks(row),
            [Ink::Dim, Ink::Muted],
            "the bar is dim and the words it holds are muted: {row:?}"
        );
    }
}

#[test]
fn an_entry_with_nothing_to_say_is_still_one_row() {
    let secrets = SecretSet::default();
    for entry in [
        Entry::Assistant {
            turn: 1,
            text: secrets.redact(""),
        },
        Entry::Call {
            turn: 1,
            id: None,
            name: secrets.redact("bash"),
            arguments: secrets.redact(""),
        },
    ] {
        let rows: Vec<Row> = layout(&entry, 4);
        assert!(!rows.is_empty(), "{entry:?} laid out as no row at all");
    }
}

fn words(row: &Row) -> String {
    row.spans().iter().map(|(text, _)| text.as_str()).collect()
}

fn inks(row: &Row) -> Vec<Ink> {
    row.spans().iter().map(|(_, look)| look.ink()).collect()
}

#[test]
fn a_thought_and_an_answer_each_keep_their_own_gutter_and_ink() {
    let secrets = SecretSet::default();
    let thought = layout(
        &Entry::Reasoning {
            turn: 1,
            text: secrets.redact("the fence is the question: a fence still open is code"),
        },
        60,
    );
    assert!(!thought.is_empty(), "a thought is rows like anything else");
    for row in &thought {
        let spans = row.spans();
        assert_eq!(
            spans.first(),
            Some(&(String::from("│ "), Look::plain(Ink::Dim))),
            "a thought opens with its own rule: {row:?}"
        );
        assert!(
            spans[1..]
                .iter()
                .all(|(_, look)| *look == Look::plain(Ink::Muted)),
            "a thought is read in the ink reasoning is read in: {row:?}"
        );
    }

    let answer = layout(
        &Entry::Assistant {
            turn: 1,
            text: secrets.redact("the fence is the question"),
        },
        60,
    );
    for row in &answer {
        assert_eq!(
            words(row).chars().take(2).collect::<String>(),
            String::from("  "),
            "an answer is printed from its own gutter: {row:?}"
        );
        assert!(
            inks(row).iter().all(|ink| *ink == Ink::Plain),
            "and in the frame's own ink: {row:?}"
        );
    }

    assert_eq!(
        pen::cells(&thought[0].spans()[0].0),
        2,
        "a thought's gutter is the width of an answer's"
    );
}

#[test]
fn a_fenced_block_is_guttered_by_its_language_and_the_prose_resumes_after_it() {
    let rows = layout(
        &Entry::Assistant {
            turn: 1,
            text: SecretSet::default().redact(
                "The closing fence is what ends a block:\n```rust\nfn main() {\n    let  x = 1;\n}\n```\nand the prose goes on.",
            ),
        },
        60,
    );

    let drawn: Vec<String> = rows.iter().map(words).collect();
    assert_eq!(
        drawn,
        [
            "  The closing fence is what ends a block:",
            "",
            "▏ rust",
            "▏ fn main() {",
            "▏    │let  x = 1;",
            "▏ }",
            "",
            "  and the prose goes on.",
        ],
        "the tag stands over the code, the code is the code it was written as behind a guide, \
         and the answer's own prose resumes after the closing fence"
    );
    assert_eq!(
        inks(&rows[2]),
        [Ink::Dim, Ink::Accent],
        "the bar is the dim ink of a fact and the language tag the accent drawn faint: {:?}",
        rows[2]
    );
    assert_eq!(
        (drawn_at(&rows[3], 0), words_column(&rows[3])),
        (Some('▏'), Some(2)),
        "every row of the block keeps the block's bar in the answer's gutter: {:?}",
        rows[3]
    );
    assert_eq!(
        inks(&rows[7]),
        [Ink::Plain, Ink::Plain],
        "and the prose after the fence is the answer's own ink again: {:?}",
        rows[7]
    );
}

#[test]
fn a_fence_that_is_still_open_draws_the_rest_of_the_answer_as_code() {
    let rows = layout(
        &Entry::Assistant {
            turn: 1,
            text: SecretSet::default()
                .redact("Here is the rule:\n```\na fence still open\n    is a code block"),
        },
        40,
    );

    let drawn: Vec<String> = rows.iter().map(words).collect();
    assert_eq!(
        drawn,
        [
            "  Here is the rule:",
            "",
            "▏ a fence still open",
            "▏    │is a code block",
        ],
        "the tail is drawn as code rather than swallowed as prose"
    );
    for row in &rows[2..] {
        assert_eq!(
            row.spans()[0],
            (String::from("▏ "), Look::plain(Ink::Dim)),
            "every row under an open fence is a code row: {row:?}"
        );
    }
}

#[test]
fn a_live_entry_is_laid_out_alone_as_its_fragments_land() {
    let mut rows = Rows::new();
    for entry in transcript() {
        rows.push(entry);
    }
    rows.measure(Measure::new(60, Surface::none(), None));

    rows.window(Scroll::Tail, 20);
    let settled = rows.lays();
    assert!(
        settled > 0,
        "the entries on screen were laid out to draw them"
    );
    let held = rows.len();

    for fragment in [
        "a fence ", "still ", "open ", "is ", "a ", "code ", "block. ",
    ]
    .repeat(15)
    {
        rows.text(fragment);
        rows.live(None);
        rows.window(Scroll::Tail, 20);
    }

    assert_eq!(
        rows.len(),
        held + 1,
        "one live entry, however many fragments landed in it"
    );
    assert_eq!(
        rows.lays(),
        settled + 105,
        "one layout per fragment, and the settled entries never laid out again"
    );
}

#[test]
fn the_rows_module_lays_the_turn_in_flight_from_the_words_it_has_said() {
    let mut rows = Rows::new();
    rows.measure(Measure::new(60, Surface::none(), None));
    rows.reasoning("the fence is the question. ");
    rows.text("A fence still open ");
    rows.reasoning("so its tail is code. ");
    rows.text("is a code block.");
    rows.live(None);

    let laid = rows.rows(0);
    assert_eq!(
        laid.iter().map(words).collect::<Vec<String>>(),
        [
            "│ the fence is the question.",
            "  A fence still open",
            "│ so its tail is code.",
            "  is a code block.",
        ],
        "the turn in flight is laid in the order its fragments landed, each under its own gutter"
    );
    assert_eq!(
        laid.iter().map(inks).collect::<Vec<_>>(),
        [
            vec![Ink::Dim, Ink::Muted],
            vec![Ink::Plain, Ink::Plain],
            vec![Ink::Dim, Ink::Muted],
            vec![Ink::Plain, Ink::Plain],
        ],
        "a thought is read in reasoning's ink, inside its rule; an answer in the frame's"
    );
}

#[test]
fn a_streaming_answer_adds_rows_and_a_fence_still_open_is_code() {
    let mut rows = Rows::new();
    rows.measure(Measure::new(40, Surface::none(), None));
    rows.reasoning("read the fence rule first. ");
    rows.text("The rule is one line: ");
    rows.live(None);
    let drawn = rows.rows(0).to_vec();
    assert!(!drawn.is_empty(), "a fragment of prose is rows");

    rows.text("a fence still open is a code block, never prose.\n```rust\nfn main() {");
    rows.live(None);
    let more = rows.rows(0).to_vec();
    assert_eq!(
        &more[..drawn.len() - 1],
        &drawn[..drawn.len() - 1],
        "every row above the one the fragment landed on stands still"
    );
    assert!(
        more.len() > drawn.len(),
        "the answer grew rows as it streamed: {} then {}",
        drawn.len(),
        more.len()
    );
    let tail: Vec<String> = more[drawn.len()..].iter().map(words).collect();
    assert_eq!(
        tail,
        [
            "  open is a code block, never prose.",
            "",
            "▏ rust",
            "▏ fn main() {"
        ],
        "the fence and its still-streaming contents are drawn as the block they opened"
    );
    assert_eq!(
        inks(more.last().expect("a row under the fence"))[..2],
        [Ink::Dim, Ink::Accent],
        "and behind the bar a code block is set behind, not as prose"
    );
}

#[test]
fn the_marker_heads_the_newest_answers_first_row_and_no_other() {
    let mut rows = Rows::new();
    rows.measure(Measure::new(60, Surface::none(), None));
    rows.text("the first thing said. ");
    rows.reasoning("and then a thought. ");
    rows.text("and the answer being read now.");

    let head = Pulse::live(4_200).head();
    rows.live(Some(&head));
    let drawn: Vec<String> = rows.rows(0).iter().map(words).collect();

    assert_eq!(
        drawn,
        [
            "  the first thing said.",
            "│ and then a thought.",
            "⠹ 4.2s and the answer being read now.",
        ],
        "the marker is on the newest answer, and the older one is drawn as it always was"
    );
}

#[test]
fn a_turn_that_has_only_reasoned_opens_the_answers_row_under_its_thought() {
    let mut rows = Rows::new();
    rows.measure(Measure::new(60, Surface::none(), None));
    rows.reasoning("thinking it over.");

    let head = Pulse::live(4_200).head();
    rows.live(Some(&head));
    let drawn: Vec<String> = rows.rows(0).iter().map(words).collect();

    assert_eq!(drawn, ["│ thinking it over.", "⠹ 4.2s"]);
}

#[test]
fn a_settled_turn_is_drawn_from_its_facts_and_never_twice() {
    let secrets = SecretSet::default();
    let mut rows = Rows::new();
    rows.push(thought(1));
    rows.measure(Measure::new(40, Surface::none(), None));

    rows.text("a fence still open is a code block.");
    rows.live(None);
    let live = rows.window(Scroll::Tail, 20);
    assert_eq!(
        rows.len(),
        2,
        "the entry above it and the live one under it"
    );
    assert!(
        !live.pieces().is_empty(),
        "the live entry is drawn while it is live"
    );

    rows.push(Entry::Assistant {
        turn: 1,
        text: secrets.redact("a fence still open is a code block."),
    });
    let settled = rows.window(Scroll::Tail, 20);
    assert_eq!(rows.len(), 2, "the fact took the live entry's place");
    assert_eq!(
        rows_shown(&settled),
        2,
        "one row for the turn and one for the answer, and no third for a block that was never a fact"
    );

    rows.live(None);
    assert_eq!(
        rows.len(),
        2,
        "a turn the record holds as entries adds nothing to the transcript"
    );
    assert_eq!(rows_shown(&rows.window(Scroll::Tail, 20)), 2);
}

#[test]
fn a_live_entry_is_laid_out_again_when_the_marker_moves() {
    let mut rows = Rows::new();
    rows.measure(Measure::new(40, Surface::none(), None));
    rows.text("an answer arriving");
    rows.live(Some(&Pulse::live(4_200).head()));
    let laid = rows.lays();
    assert_eq!(laid, 1, "the live entry, laid out once to be drawn");

    rows.live(Some(&Pulse::live(4_200).head()));
    assert_eq!(
        rows.lays(),
        laid,
        "the same mark at the same time is the same picture"
    );

    rows.live(Some(&Pulse::live(4_280).head()));
    assert_eq!(
        rows.lays(),
        laid + 1,
        "a moved marker is one layout of the live entry"
    );

    rows.text(" and settling");
    rows.live(Some(&Pulse::live(4_280).head()));
    assert_eq!(
        rows.lays(),
        laid + 2,
        "a fragment and a moved marker are one layout each, and never two for one"
    );
}

#[test]
fn a_markers_step_lays_the_row_it_heads_and_no_other() {
    let mut rows = Rows::new();
    rows.measure(Measure::new(60, Surface::none(), None));
    rows.text("a short answer.");
    rows.live(Some(&Pulse::live(4_200).head()));

    let before: Vec<Row> = rows.rows(0).to_vec();
    let laid = rows.laid();
    rows.live(Some(&Pulse::live(4_280).head()));
    let stepped: Vec<Row> = rows.rows(0).to_vec();
    assert_eq!(
        rows.laid(),
        laid + 1,
        "a marker's step lays the row it heads"
    );
    assert_eq!(before[0].glyphs(), "⠹ 4.2s a short answer.");
    assert_eq!(stepped[0].glyphs(), "⠸ 4.2s a short answer.");
    let mut once = Rows::new();
    once.measure(Measure::new(60, Surface::none(), None));
    once.text("a short answer.");
    once.live(Some(&Pulse::live(4_280).head()));
    assert_eq!(
        once.rows(0)[0],
        stepped[0],
        "the row a step lays is the row a first pass lays, cell for cell"
    );
    assert_eq!(
        before[1..],
        stepped[1..],
        "every row under it stands as it was laid"
    );
}

#[test]
fn a_markers_step_lays_one_row_however_long_the_answer_under_it_is() {
    let mut rows = Rows::new();
    rows.measure(Measure::new(60, Surface::none(), None));
    rows.text(&"word ".repeat(4_000));
    rows.live(Some(&Pulse::live(4_200).head()));
    let deep = rows.rows(0).len();
    assert!(deep > 200, "the answer is {deep} rows deep");

    let laid = rows.laid();
    rows.live(Some(&Pulse::live(4_280).head()));
    assert_eq!(
        rows.laid(),
        laid + 1,
        "a step of the marker costs the same however long the answer under it is"
    );
}

#[test]
fn a_fragment_that_lands_lays_the_answers_rows_and_leaves_the_markers_cells() {
    let mut rows = Rows::new();
    rows.measure(Measure::new(40, Surface::none(), None));
    rows.text("an answer arriving");
    let head = Pulse::live(4_200).head();
    rows.live(Some(&head));
    let marker: Vec<(String, Look)> = rows.rows(0)[0].spans()[..3].to_vec();
    let laid = rows.laid();

    rows.text(" one word at a time, and the wrap under it moves with them");
    rows.live(Some(&head));

    let drawn: Vec<Row> = rows.rows(0).to_vec();
    assert_eq!(drawn.len(), 3, "the answer is three rows now");
    assert_eq!(
        rows.laid(),
        laid + drawn.len(),
        "its rows are laid again, the one the marker heads among them"
    );
    assert_eq!(
        drawn[0].spans()[..3],
        marker[..],
        "and the marker's own cells stand where they stood"
    );

    let laid = rows.laid();
    rows.live(Some(&Pulse::live(4_280).head()));
    assert_eq!(
        rows.laid(),
        laid + 1,
        "a marker still steps in one row over an answer it landed under"
    );
}

#[test]
fn a_marker_the_band_gives_up_steps_without_laying_the_answer_again() {
    let mut rows = Rows::new();
    rows.measure(Measure::new(6, Surface::none(), None));
    rows.text("an answer too wide for a marker beside it");
    rows.live(Some(&Pulse::live(4_200).head()));
    let first: Vec<Row> = rows.rows(0).to_vec();
    assert!(
        !first[0].glyphs().contains('⠹'),
        "the band holds no marker beside a word of the answer: {:?}",
        first[0].glyphs()
    );

    let laid = rows.laid();
    rows.live(Some(&Pulse::live(4_280).head()));
    assert_eq!(
        rows.laid(),
        laid + 1,
        "the row the marker would head is laid, and the answer under it is not"
    );
    assert_eq!(
        rows.rows(0).to_vec(),
        first,
        "and a marker the band gave up moves no cell of it"
    );
}

#[test]
fn a_marker_whose_words_widen_lays_the_rows_under_it_again() {
    let mut rows = Rows::new();
    rows.measure(Measure::new(40, Surface::none(), None));
    rows.text("aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii");
    rows.live(Some(&Pulse::live(9_900).head()));
    let narrow: Vec<Row> = rows.rows(0).to_vec();
    assert!(
        narrow[0].glyphs().contains("9.9s"),
        "the marker says the time it stands on"
    );
    let laid = rows.laid();

    rows.live(Some(&Pulse::live(10_000).head()));

    let wide: Vec<Row> = rows.rows(0).to_vec();
    assert!(
        rows.laid() > laid + 1,
        "a wider marker lays the rows under it again, because what they wrap at moved"
    );
    assert_ne!(
        narrow[0].spans()[3..],
        wide[0].spans()[3..],
        "and the answer's first words wrap at the columns the marker's own words reserve"
    );
}

#[test]
fn the_recorded_answers_marker_moves_the_row_it_heads_and_no_other_entries() {
    let mut rows = Rows::new();
    rows.measure(Measure::new(60, Surface::none(), None));
    for entry in transcript() {
        rows.push(entry);
    }
    rows.heading(Some(Pulse::live(4_200).head()));
    let newest = rows.len() - 1;
    for at in 0..rows.len() {
        rows.rows(at);
    }
    let before: Vec<Row> = rows.rows(newest).to_vec();
    assert!(
        before[0].glyphs().contains("4.2s"),
        "the marker heads the recorded answer's first row"
    );
    let laid = rows.laid();

    rows.heading(Some(Pulse::live(4_280).head()));
    for at in 0..rows.len() {
        rows.rows(at);
    }

    let stepped: Vec<Row> = rows.rows(newest).to_vec();
    assert_eq!(
        rows.laid(),
        laid + before.len(),
        "the answer the marker heads is the only entry of the band laid again"
    );
    assert_ne!(before[0], stepped[0], "and that row is what moved");
    assert_eq!(
        before[1..],
        stepped[1..],
        "every row under it stands as it was laid"
    );
}

#[test]
fn a_reflow_lays_the_live_entry_once_and_a_pass_that_moved_nothing_lays_none() {
    let mut rows = Rows::new();
    rows.measure(Measure::new(40, Surface::none(), None));
    rows.text("an answer arriving");
    let head = Pulse::live(4_200).head();
    rows.live(Some(&head));
    let laid = rows.lays();
    assert_eq!(laid, 1, "the live entry, laid out once to be drawn");

    rows.measure(Measure::new(30, Surface::none(), None));
    rows.live(Some(&head));
    rows.window(Scroll::Tail, 20);

    assert_eq!(
        rows.lays(),
        laid + 1,
        "a new width lays the live entry once, and the pass over it lays nothing"
    );

    rows.live(Some(&head));
    rows.window(Scroll::Tail, 20);

    assert_eq!(
        rows.lays(),
        laid + 1,
        "a pass that moved neither the fragments nor the mark lays nothing"
    );
}

#[test]
fn a_live_run_opens_the_answers_row_before_it_has_said_anything() {
    let mut rows = Rows::new();
    rows.measure(Measure::new(40, Surface::none(), None));
    rows.live(Some(&Pulse::live(0).head()));

    assert_eq!(rows.len(), 1, "the answer's row, opened by its marker");
    assert_eq!(rows_shown(&rows.window(Scroll::Tail, 20)), 1);
    assert_eq!(said(&painted(&rows.rows(0)[0], 40)), "⠋ 0.0s");

    rows.live(None);
    assert_eq!(rows.len(), 0);
}

fn call(which: u32) -> Entry {
    call_of(which, None)
}

fn call_of(which: u32, identity: Option<&str>) -> Entry {
    Entry::Call {
        turn: which,
        id: identity.map(|identity| SecretSet::default().redact(identity)),
        name: SecretSet::default().redact("bash"),
        arguments: SecretSet::default().redact(r#"{"command": "ls"}"#),
    }
}

fn result(which: u32, exit: Exit) -> Entry {
    result_of(which, None, exit)
}

fn result_of(which: u32, identity: Option<&str>, exit: Exit) -> Entry {
    Entry::Result {
        turn: which,
        id: identity.map(|identity| SecretSet::default().redact(identity)),
        name: SecretSet::default().redact("bash"),
        text: SecretSet::default().redact("exit 0\na.txt\n"),
        bytes: 7,
        elapsed_ms: 2,
        exit: Some(exit),
    }
}

fn long_result(which: u32) -> Entry {
    let text: String = (1..=20)
        .map(|line| format!("test rows::a_row_of_output_{line} ... ok"))
        .collect::<Vec<String>>()
        .join("\n");
    Entry::Result {
        turn: which,
        id: None,
        name: SecretSet::default().redact("bash"),
        text: SecretSet::default().redact(&text),
        bytes: 640,
        elapsed_ms: 1_800,
        exit: Some(Exit::Code(0)),
    }
}

fn drawn_inks(laid: &[Row], cols: u16) -> Vec<Ink> {
    laid.iter()
        .flat_map(|row| cell_inks(&painted(row, cols)))
        .collect()
}

fn named(which: u32, name: &str, arguments: &str) -> Entry {
    Entry::Call {
        turn: which,
        id: None,
        name: SecretSet::default().redact(name),
        arguments: SecretSet::default().redact(arguments),
    }
}

fn ground(workspace: &std::path::Path) -> Ground {
    let boundary = Boundary::from_config(&Config::default(), workspace, Enforcement::Bubblewrap)
        .expect("the boundary holds");
    Ground::new(&boundary, workspace, workspace).expect("the ground resolves")
}

fn way() -> Look {
    Look::plain(Ink::Plain).underlined(Ink::Dim)
}

#[test]
fn a_path_a_call_names_is_a_way_to_the_file_the_run_judged_it_openable_as() {
    let tree = tempfile::tempdir().expect("a temp workspace");
    std::fs::write(tree.path().join("notes.html"), "body").expect("the file");
    let entry = named(1, "open", r#"{"path":"notes.html"}"#);

    let rows = layout_on(&entry, 40, &Surface::none(), Some(&ground(tree.path())));

    assert_eq!(
        said(&painted(&rows[1], 40)),
        "  notes.html",
        "the call's arguments spell the path, so the row says the path and nothing around it"
    );
    assert!(
        painted(&rows[1], 40)[2..12]
            .iter()
            .all(|(_, look)| *look == way()),
        "the path's own cells wear the line a way wears: {:?}",
        painted(&rows[1], 40)
    );

    let resolved = std::fs::canonicalize(tree.path().join("notes.html")).expect("the file");
    let destination = format!("file://{}", resolved.display());
    let (_, links) = laid(&rows[1], Rect::new(0, 0, 40, 1));
    assert_eq!(
        links.at(0, 2),
        Some(destination.as_str()),
        "the path's first cell opens the file the judgement resolved it to"
    );
    assert_eq!(
        links.at(0, 11),
        Some(destination.as_str()),
        "and so does its last"
    );
    assert_eq!(
        links.at(0, 12),
        None,
        "and no cell past the path is a way to the file"
    );
    assert_eq!(
        links.at(0, 1),
        None,
        "nor the blank the gutter sets before it"
    );
}

#[test]
fn a_path_wider_than_the_body_is_one_destination_on_each_row() {
    let tree = tempfile::tempdir().expect("a temp workspace");
    let long = "a-note-on-the-ways-a-row-is-drawn-and-the-file-it-names.html";
    std::fs::write(tree.path().join(long), "body").expect("the file");
    let entry = named(1, "open", &format!(r#"{{"path":"{long}"}}"#));

    let rows = layout_on(&entry, 30, &Surface::none(), Some(&ground(tree.path())));

    assert!(
        rows.len() > 2,
        "a path wider than the body is broken over two rows: {:?}",
        rows.iter().map(Row::glyphs).collect::<Vec<String>>()
    );
    let resolved = std::fs::canonicalize(tree.path().join(long)).expect("the file");
    let destination = format!("file://{}", resolved.display());
    let area = Rect::new(0, 0, 30, u16::try_from(rows.len()).unwrap());
    let mut buffer = Buffer::empty(area);
    let mut links = Links::none();
    for (at, row) in rows[1..].iter().enumerate() {
        row.paint(
            Rect {
                y: u16::try_from(at).unwrap(),
                height: 1,
                ..area
            },
            &mut buffer,
            &Palette::TRUECOLOR,
            &mut links,
            &mut Sizes::none(),
        );
    }
    let destinations: Vec<&str> = (0..u16::try_from(rows.len() - 1).unwrap())
        .filter_map(|y| links.at(y, 4))
        .collect();
    assert!(
        destinations.len() > 1,
        "the path is drawn over more than one row: {destinations:?}"
    );
    assert!(
        destinations
            .iter()
            .all(|said| *said == destination.as_str()),
        "and each row it was broken over carries the whole destination, never half of it: \
         {destinations:?}"
    );
}

#[test]
fn every_cell_of_the_path_a_call_names_is_a_way_to_the_file() {
    let tree = tempfile::tempdir().expect("a temp workspace");
    std::fs::write(tree.path().join("notes.html"), "body").expect("the file");
    let entry = named(1, "open", r#"{"note": "notes.html", "path": "notes.html"}"#);

    let rows = layout_on(&entry, 50, &Surface::none(), Some(&ground(tree.path())));

    assert_eq!(
        said(&painted(&rows[1], 50)),
        "  notes.html",
        "the arguments spell the path once, so the row says it once"
    );

    let resolved = std::fs::canonicalize(tree.path().join("notes.html")).expect("the file");
    let destination = format!("file://{}", resolved.display());
    let (_, links) = laid(&rows[1], Rect::new(0, 0, 50, 1));
    for column in 2..12 {
        assert_eq!(
            links.at(0, column),
            Some(destination.as_str()),
            "column {column} is the path the run read, and every cell of it is a way there"
        );
    }
    assert_eq!(
        links.at(0, 12),
        None,
        "and no cell past the path is a way anywhere"
    );
}

#[test]
fn a_path_a_call_names_that_the_run_would_have_refused_is_drawn_as_it_is_today() {
    let tree = tempfile::tempdir().expect("a temp workspace");
    let entry = named(1, "open", r#"{"path":"outside.html"}"#);

    let judged = layout_on(&entry, 40, &Surface::none(), Some(&ground(tree.path())));
    let plain = layout(&entry, 40);

    assert_eq!(
        said(&painted(&judged[1], 40)),
        "  outside.html",
        "the refused call still says the path it named, and nothing around it"
    );
    assert_eq!(
        painted(&judged[1], 40),
        painted(&plain[1], 40),
        "a file the run would have refused is drawn exactly as the frame draws it with no \
         judgement at all: the words and no line under them"
    );
    let (_, links) = laid(&judged[1], Rect::new(0, 0, 40, 1));
    assert!(
        links.is_empty(),
        "and no cell of the row is a way to anywhere"
    );
}

#[test]
fn a_bash_call_is_drawn_as_the_command_line_a_reader_would_type() {
    let rows = layout(&named(1, "bash", r#"{"command": "rg -n 'fence' src"}"#), 60);
    let drawn: Vec<String> = rows.iter().map(words).collect();

    assert_eq!(
        drawn,
        ["… bash running", "  rg -n 'fence' src"],
        "the call names the tool in the gutter and the command line a reader would type under it: \
         {drawn:?}"
    );
    assert!(
        drawn.iter().all(|row| unpunctuated(row)),
        "no cell of a call's row is punctuation the model wrote around its arguments: {drawn:?}"
    );
}

#[test]
fn a_skill_call_is_drawn_as_the_skills_own_name() {
    let rows = layout(&named(1, "skill", r#"{"name": "rust-review"}"#), 60);
    let drawn: Vec<String> = rows.iter().map(words).collect();

    assert_eq!(
        drawn,
        ["… skill running", "  rust-review"],
        "the row says the skill a reader would ask for by that name: {drawn:?}"
    );
}

#[test]
fn an_argument_no_tool_spells_is_drawn_as_words_and_never_as_punctuation() {
    let rows = layout(
        &named(1, "grep", r#"{"glob": "*.rs", "pattern": "fn main"}"#),
        60,
    );

    assert_eq!(
        words(&rows[1]),
        "  glob *.rs pattern fn main",
        "a call this product has no tool for says each key and the value under it"
    );
    assert!(
        unpunctuated(&words(&rows[1])),
        "and it says them as words: no brace, bracket, quote, comma or colon survives: {:?}",
        words(&rows[1])
    );
}

fn unpunctuated(row: &str) -> bool {
    !row.chars()
        .any(|glyph| matches!(glyph, '{' | '}' | '[' | ']' | '"' | ':' | ','))
}

#[test]
fn a_call_wears_the_mark_its_verdict_settled_on_and_no_mark_before_one() {
    let mut rows = Rows::new();
    rows.measure(Measure::new(80, Surface::none(), None));
    rows.push(call(1));
    let opened = painted(&rows.rows(0)[0], 80);
    assert_eq!(
        said(&opened),
        "  │ … bash running",
        "a call still out wears its ellipsis"
    );
    assert_eq!(
        cell_inks(&opened)[0],
        Ink::Dim,
        "and the ink of a row that claims nothing"
    );

    rows.push(result(1, Exit::Code(1)));
    let settled = painted(&rows.rows(0)[0], 80);
    assert_eq!(
        said(&settled),
        "  │ ✗ bash failure exit 1 2 ms",
        "a call a result settled on wears the cross of its verdict and the name it was called by"
    );
    assert_eq!(cell_inks(&settled)[4], Ink::Err);
}

#[test]
fn a_result_the_cache_reads_on_its_own_still_says_the_tool_it_came_from() {
    let mut rows = Rows::new();
    rows.measure(Measure::new(80, Surface::none(), None));
    rows.push(result(1, Exit::Code(0)));

    assert_eq!(
        said(&painted(&rows.rows(0)[0], 80)),
        "  │ ✓ bash success 2 ms",
        "a result with no call above it names the tool, because no call row holds that name for it"
    );
}

#[test]
fn a_result_row_says_the_verdict_the_duration_and_the_size_and_repeats_no_name_the_call_row_holds()
{
    let mut rows = Rows::new();
    rows.measure(Measure::new(80, Surface::none(), None));
    rows.push(call(1));
    let opened = rows.rows(0).len();

    rows.push(result(1, Exit::Code(0)));
    let settled: Vec<String> = rows.rows(0).iter().map(words).collect();
    assert_eq!(
        settled[..opened],
        ["    ✓ bash success 2 ms", "      ls"],
        "the call keeps the row it opened and the words it was called with: {settled:?}"
    );
    assert_eq!(
        settled.last().unwrap(),
        "      exit 0 · 7 bytes",
        "and under them the result says the verdict, how long the call took and how much it \
         wrote: {settled:?}"
    );
    assert!(
        settled[opened..].iter().all(|row| !row.contains("bash")),
        "repeating no name the call row already held: {settled:?}"
    );
}

#[test]
fn a_result_with_no_verdict_says_the_numbers_and_guesses_no_exit() {
    let entry = Entry::Result {
        turn: 1,
        id: None,
        name: SecretSet::default().redact("bash"),
        text: SecretSet::default().redact("a.txt"),
        bytes: 7,
        elapsed_ms: 2,
        exit: None,
    };

    let rows = layout(&entry, 40);

    assert_eq!(
        said(&painted(&rows[0], 40)),
        "✓ bash completed 2 ms",
        "a result with no verdict claims none: the name stands in a body's gutter and the numbers \
         it holds, with no `exit 0` guessed from the text beside them"
    );
}

fn painted(row: &Row, cols: u16) -> Vec<(char, Look)> {
    let palette = Palette::TRUECOLOR;
    let area = Rect::new(0, 0, cols, 1);
    let mut buffer = Buffer::empty(area);
    row.paint(
        area,
        &mut buffer,
        &palette,
        &mut Links::none(),
        &mut Sizes::none(),
    );
    (0..row.width())
        .map(|x| {
            let cell = buffer
                .cell((x, 0))
                .unwrap_or_else(|| panic!("column {x} is outside the row"));
            let look = Look::of(cell, &palette)
                .unwrap_or_else(|| panic!("column {x} is drawn in a style that is not a look"));
            let glyph = cell.symbol().chars().next().expect("one glyph a cell");
            (glyph, look)
        })
        .collect()
}

fn spelled(row: &Row, cols: u16) -> String {
    let band = Rect::new(0, 0, cols, 4);
    let mut buffer = Buffer::empty(band);
    let mut sizes = Sizes::none();
    row.paint(
        Rect { height: 1, ..band },
        &mut buffer,
        &Palette::TRUECOLOR,
        &mut Links::none(),
        &mut sizes,
    );
    sizes.settle(band, &mut buffer, &Palette::TRUECOLOR);
    (0..row.width())
        .map(|x| {
            buffer
                .cell((x, 0))
                .expect("a cell of the row")
                .symbol()
                .to_owned()
        })
        .collect::<String>()
        .trim_end()
        .to_owned()
}

#[test]
fn a_live_answer_that_opens_on_a_large_heading_keeps_the_cells_the_sizing_leaves() {
    let mut rows = Rows::new();
    rows.measure(Measure::new(
        40,
        Surface::of(None, CellSize::new(10, 20), false, true),
        None,
    ));
    rows.text("# A gate rule");
    rows.live(Some(&Pulse::live(4_200).head()));

    let heading = rows.rows(0)[1].clone();
    assert_eq!(
        spelled(&heading, 40),
        "  A   g a t e   r u l e",
        "the heading under the marker's row is drawn at twice the cell: {heading:?}"
    );
    assert_eq!(
        heading.glyphs().trim_start(),
        "A gate rule",
        "and the row is copied as its words: {heading:?}"
    );
}

fn said(row: &[(char, Look)]) -> String {
    row.iter()
        .map(|(glyph, _)| *glyph)
        .collect::<String>()
        .trim_end()
        .to_owned()
}

fn cell_inks(row: &[(char, Look)]) -> Vec<Ink> {
    row.iter().map(|(_, look)| look.ink()).collect()
}

#[test]
fn a_result_longer_than_its_fold_draws_its_head_and_the_count_of_rows_kept_back() {
    let rows = layout(&long_result(1), 80);
    let drawn: Vec<String> = rows.iter().map(words).collect();

    assert_eq!(
        rows.len(),
        5,
        "a folded result is the four rows of its head and the row counting what is kept back: {drawn:?}"
    );
    assert_eq!(
        drawn[0], "✓ bash success 1.8 s",
        "the head opens with the verdict row, which a fold never takes away"
    );
    assert_eq!(
        drawn[1..4],
        [
            "  test rows::a_row_of_output_1 ... ok",
            "  test rows::a_row_of_output_2 ... ok",
            "  test rows::a_row_of_output_3 ... ok",
        ],
        "and the rows of output under it are the first rows the command wrote: {drawn:?}"
    );
    assert_eq!(
        drawn[4], "⋯ 18 rows kept back",
        "the count includes every withheld output and detail row: {drawn:?}"
    );
    assert!(!drawn.iter().any(|row| row.contains("bytes")));
    assert_eq!(
        inks(&rows[4])[0],
        Ink::Dim,
        "the marker the count opens with is the gutter's own faded mark"
    );
    assert!(
        inks(&rows[4])[2..].iter().all(|ink| *ink == Ink::Dim),
        "and its words are the dim ink of the fold's own note, a rung under the output it counts"
    );
}

#[test]
fn a_result_its_whole_fold_holds_is_drawn_whole() {
    let entry = Entry::Result {
        turn: 1,
        id: None,
        name: SecretSet::default().redact("bash"),
        text: SecretSet::default().redact("a.txt\nb.txt\nc.txt\nd.txt"),
        bytes: 40,
        elapsed_ms: 3,
        exit: Some(Exit::Code(0)),
    };
    let rows = layout(&entry, 80);
    let drawn: Vec<String> = rows.iter().map(words).collect();

    assert_eq!(
        rows.len(),
        6,
        "four rows of a command's output are a body a fold would not hold back, so every one of \
         them is drawn: {drawn:?}"
    );
    assert!(
        !drawn.iter().any(|row| row.contains("kept back")),
        "a result with no fold says nothing about rows kept back: {drawn:?}"
    );
}

fn kept_back_row(laid: &[Row]) -> Option<String> {
    laid.iter().map(words).find(|row| row.contains("kept back"))
}

#[test]
fn a_results_fold_inside_an_opened_work_survives_a_reflow_and_a_new_entry_beside_it() {
    let mut rows = Rows::new();
    rows.measure(Measure::new(80, Surface::none(), None));
    rows.push(long_result(1));
    rows.push(answer(1));
    rows.push(stop_entry(Stop::Final));
    assert!(
        rows.toggle_newest(),
        "the work the result stands in is one the reader opens"
    );
    assert!(
        rows.open(0),
        "and the result inside that work keeps a fold of its own to open"
    );
    let opened: Vec<String> = rows.rows(0).iter().map(words).collect();
    assert_eq!(
        opened.len(),
        22,
        "the fold is open, so the whole result is drawn: {opened:?}"
    );
    let laid = rows.lays();

    rows.measure(Measure::new(30, Surface::none(), None));
    let reflowed: Vec<String> = rows.rows(0).iter().map(words).collect();
    assert_eq!(
        rows.lays(),
        laid + 1,
        "the narrower band re-lays the opened work once"
    );
    assert!(
        reflowed.len() > 21,
        "the same output takes more rows at thirty columns than at eighty: {}",
        reflowed.len()
    );
    assert_eq!(
        kept_back_row(rows.rows(0)),
        None,
        "a resize is no reader's hand on the fold, so it stays open and counts nothing back"
    );

    rows.push(thought(2));
    let beside: Vec<String> = rows.rows(3).iter().map(words).collect();
    assert_eq!(
        beside,
        vec![String::from("│ turn 2")],
        "the entry beside it is the one row it is drawn as, and no row of the fold's: {beside:?}"
    );
    assert_eq!(
        rows.lays(),
        laid + 2,
        "a new entry beside an opened work lays itself out and leaves the work alone"
    );
    assert!(
        rows.rows(0)
            .iter()
            .map(words)
            .any(|row| row.contains("a_row_of_output_20")),
        "the result the reader opened is still open with a new entry beside it"
    );
}

#[test]
fn a_session_read_back_is_folded_again() {
    let directory = tempfile::tempdir().expect("a session directory");
    let lines: Vec<String> = [thought(1), long_result(1)]
        .iter()
        .map(|entry| serde_json::to_string(entry).expect("an entry is one line of a transcript"))
        .collect();
    std::fs::write(
        transcript_path(directory.path()),
        format!("{}\n", lines.join("\n")),
    )
    .expect("a transcript a run wrote");
    let read = read_transcript(directory.path()).expect("the transcript reads back");
    assert_eq!(
        read.entries().len(),
        2,
        "the session holds the turn and the result the run recorded"
    );

    let mut rows = Rows::new();
    rows.measure(Measure::new(80, Surface::none(), None));
    for entry in read.entries().iter().cloned() {
        rows.push(entry);
    }
    let drawn: Vec<String> = rows.rows(1).iter().map(words).collect();

    assert_eq!(
        kept_back_row(rows.rows(1)).as_deref(),
        Some("    ⋯ 18 rows kept back"),
        "a result read back from a session is a fold again, because a fold is a fact of the \
         layout and never a state the session carries: {drawn:?}"
    );
    assert!(
        !drawn.iter().any(|row| row.contains("a_row_of_output_12")),
        "and the rows it keeps back are kept back on the way in: {drawn:?}"
    );
}

fn timed(which: u32, elapsed_ms: u64) -> Entry {
    Entry::Result {
        turn: which,
        id: None,
        name: SecretSet::default().redact("bash"),
        text: SecretSet::default().redact("exit 0\na.txt\n"),
        bytes: 7,
        elapsed_ms,
        exit: Some(Exit::Code(0)),
    }
}

fn reading(which: u32, path: &str) -> Entry {
    named(which, "open", &format!(r#"{{"path": "{path}"}}"#))
}

fn asked_for(which: u32) -> Entry {
    Entry::Prompt {
        text: SecretSet::default().redact(format!("prompt {which}")),
    }
}

fn one_row_of(work: Vec<Entry>) -> Row {
    let mut rows = Rows::new();
    rows.measure(Measure::new(80, Surface::none(), None));
    rows.push(asked_for(1));
    for entry in work {
        rows.push(entry);
    }
    rows.push(answer(1));
    rows.push(stop_entry(Stop::Final));
    let laid: Vec<String> = rows.rows(1).iter().map(words).collect();
    assert_eq!(
        laid.len(),
        1,
        "a settled work is drawn as exactly one row: {laid:?}"
    );
    rows.rows(1)[0].clone()
}

#[test]
fn a_settled_exchanges_work_is_one_row() {
    let mut rows = Rows::new();
    rows.measure(Measure::new(80, Surface::none(), None));
    rows.push(Entry::Prompt {
        text: SecretSet::default().redact("triage the retry test"),
    });
    rows.push(Entry::Turn { turn: 1 });
    rows.push(thought(1));
    rows.push(call(1));
    rows.push(timed(1, 4_100));
    rows.push(answer(1));
    rows.push(stop_entry(Stop::Final));

    let work: Vec<String> = rows.rows(1).iter().map(words).collect();
    assert_eq!(
        work,
        ["⋯ 1 call · 4.1 s"],
        "a settled exchange's work is one row standing where its work stood, between the prompt \
         and the answer: {work:?}"
    );
    assert_eq!(
        inks(&rows.rows(1)[0]),
        vec![Ink::Dim, Ink::Plain, Ink::Dim],
        "the mark and the words are in the dim ink of a fact"
    );
    for at in [2, 3] {
        assert!(
            rows.rows(at).is_empty(),
            "the thought and the call under the work are drawn as its one row and no row of \
             their own: {:?}",
            rows.rows(at).iter().map(words).collect::<Vec<String>>()
        );
    }
    assert_eq!(
        rows.len(),
        6,
        "the prompt, the three entries of the work the result settled into, the answer and the \
         stop the run ended on"
    );
}

#[test]
fn a_settled_works_row_counts_its_calls_its_files_and_its_time() {
    let mut work = Vec::new();
    for which in 0..12 {
        work.push(reading(1, ["a.rs", "b.rs", "c.rs"][which % 3]));
        work.push(timed(1, if which == 0 { 4_100 } else { 0 }));
    }

    assert_eq!(
        words(&one_row_of(work)),
        "⋯ 12 calls · 3 files · 4.1 s",
        "the work's row says how many calls it made, how many files it read and how long they \
         took, in that order"
    );
}

#[test]
fn a_work_holding_no_file_says_no_file() {
    let work = vec![call(1), timed(1, 4_100)];

    assert_eq!(
        words(&one_row_of(work)),
        "⋯ 1 call · 4.1 s",
        "a work that read no file says nothing of a file it does not hold"
    );
}

#[test]
fn a_count_of_one_is_a_singular() {
    let work = vec![reading(1, "notes.md"), timed(1, 4_100)];

    assert_eq!(
        words(&one_row_of(work)),
        "⋯ 1 call · 1 file · 4.1 s",
        "one call and one file are said in the singular, the way a count of one is read"
    );
}

#[test]
fn two_calls_naming_one_path_count_one_file() {
    let work = vec![
        reading(1, "notes.html"),
        timed(1, 0),
        reading(1, "notes.html"),
        timed(1, 2_000),
    ];

    assert_eq!(
        words(&one_row_of(work)),
        "⋯ 2 calls · 1 file · 2.0 s",
        "the same path named twice is one file, however many calls read it"
    );
}

#[test]
fn a_work_under_a_second_says_the_milliseconds_it_took() {
    let work = vec![call(1), timed(1, 23)];

    assert_eq!(
        words(&one_row_of(work)),
        "⋯ 1 call · 23 ms",
        "a work under a second says the milliseconds it took, and never a tenth of a second that \
         reads as none"
    );
}

#[test]
fn a_work_taking_longer_than_a_minute_says_its_minutes_and_seconds() {
    let work = vec![call(1), timed(1, 90_000)];

    assert_eq!(
        words(&one_row_of(work)),
        "⋯ 1 call · 1m 30s",
        "a work that took longer than a minute says its minutes and the seconds under them"
    );
}

#[test]
fn a_url_an_open_names_is_no_file() {
    let work = vec![
        named(1, "open", r#"{"url": "https://example.test/notes.html"}"#),
        timed(1, 4_100),
    ];

    assert_eq!(
        words(&one_row_of(work)),
        "⋯ 1 call · 4.1 s",
        "an open call that names a url read no file of this workspace, so the work counts none"
    );
}

#[test]
fn a_work_whose_results_carry_no_time_says_no_time() {
    let work = vec![call(1), timed(1, 0)];

    assert_eq!(
        words(&one_row_of(work)),
        "⋯ 1 call",
        "a work whose results carry no time says no time rather than a time of nothing"
    );
}

#[test]
fn the_run_in_flight_draws_its_calls_and_results_and_no_work_row() {
    let mut rows = Rows::new();
    rows.measure(Measure::new(80, Surface::none(), None));
    rows.push(Entry::Prompt {
        text: SecretSet::default().redact("triage the retry test"),
    });
    rows.push(Entry::Turn { turn: 1 });
    rows.push(thought(1));
    rows.push(call(1));
    rows.push(timed(1, 4_100));

    rows.text("the retry test is the one to read");
    rows.live(None);

    let laid: Vec<String> = rows
        .window(Scroll::Back(0), 40)
        .pieces()
        .iter()
        .flat_map(|piece| (0..piece.rows).map(move |row| (piece.entry, row)))
        .map(|(entry, row)| words(&rows.rows(entry)[usize::from(row)]))
        .collect();
    assert!(
        laid.iter().any(|row| row == "│ turn 1"),
        "the run in flight draws its thought as it lands: {laid:?}"
    );
    assert!(
        laid.iter().any(|row| row.starts_with("    ✓ bash success")),
        "and the call the result settled: {laid:?}"
    );
    assert!(
        laid.iter().any(|row| row == "      exit 0 · 7 bytes"),
        "and the result under it: {laid:?}"
    );
    assert!(
        !laid
            .iter()
            .any(|row| row.starts_with("⋯") && row.contains("call")),
        "and no work row of its own, because the work is not settled: {laid:?}"
    );
}

#[test]
fn a_run_of_two_turns_leaves_one_row_for_the_work_of_both() {
    let mut rows = Rows::new();
    rows.measure(Measure::new(80, Surface::none(), None));
    for entry in [
        asked_for(1),
        Entry::Turn { turn: 1 },
        call(1),
        timed(1, 4_000),
        answer(1),
        Entry::Turn { turn: 2 },
        call(2),
        timed(2, 100),
        answer(2),
        stop_entry(Stop::Final),
    ] {
        rows.push(entry);
    }
    let work: Vec<String> = rows.rows(1).iter().map(words).collect();
    assert_eq!(
        work,
        ["⋯ 2 calls · 4.1 s"],
        "the work of a run that took two turns is one row, counted across both of them: {work:?}"
    );
    for at in [2, 3, 4] {
        assert!(
            rows.rows(at).is_empty(),
            "the answer the run went past and the second turn's call are drawn in that one row: \
             {:?}",
            rows.rows(at).iter().map(words).collect::<Vec<String>>()
        );
    }
    let answer: Vec<String> = rows.rows(6).iter().map(words).collect();
    assert!(
        answer
            .iter()
            .any(|row| row.trim_start().starts_with("The closing fence")),
        "and the answer the run ended on stands under it as the answer it is: {answer:?}"
    );
}

#[test]
fn the_run_in_flight_draws_its_work_whole_as_each_turn_ends() {
    let mut rows = Rows::new();
    rows.measure(Measure::new(80, Surface::none(), None));
    for entry in [
        asked_for(1),
        Entry::Turn { turn: 1 },
        call(1),
        timed(1, 4_000),
        answer(1),
        Entry::Turn { turn: 2 },
        call(2),
        timed(2, 100),
    ] {
        rows.push(entry);
    }

    let laid: Vec<(usize, String)> = rows
        .window(Scroll::Back(0), 40)
        .pieces()
        .iter()
        .flat_map(|piece| (0..piece.rows).map(move |row| (piece.entry, row)))
        .map(|(entry, row)| (entry, words(&rows.rows(entry)[usize::from(row)])))
        .collect();
    let said: Vec<&String> = laid.iter().map(|(_, row)| row).collect();
    assert!(
        said.iter().any(|row| row.starts_with("    ✓ bash success")),
        "the run in flight draws the call of the turn that ended: {said:?}"
    );
    assert!(
        said.iter().any(|row| row
            .trim_start()
            .starts_with("The closing fence is what ends")),
        "and the answer that turn wrote stands under it: {said:?}"
    );
    assert!(
        !said.iter().any(|row| row.starts_with("⋯")),
        "and no work row stands anywhere, because the exchange is the one being watched: {said:?}"
    );
}

#[test]
fn a_work_settles_under_the_answer_that_ended_it() {
    let mut rows = Rows::new();
    rows.measure(Measure::new(80, Surface::none(), None));
    for entry in [
        asked_for(1),
        Entry::Turn { turn: 1 },
        call(1),
        timed(1, 4_100),
        answer(1),
    ] {
        rows.push(entry);
    }
    assert!(
        !rows.rows(2).is_empty(),
        "the exchange is the one being watched, so its work is drawn whole under the answer"
    );

    rows.push(stop_entry(Stop::Final));
    let work: Vec<String> = rows.rows(1).iter().map(words).collect();
    assert_eq!(
        work,
        ["⋯ 1 call · 4.1 s"],
        "the stop that lands under the answer settles the work above it, and the rows the work was \
         drawn as are laid again as its one row: {work:?}"
    );
}

#[test]
fn one_key_opens_the_newest_settled_works_row_and_the_same_key_closes_it_again() {
    let mut rows = Rows::new();
    rows.measure(Measure::new(80, Surface::none(), None));
    rows.push(Entry::Prompt {
        text: SecretSet::default().redact("triage the retry test"),
    });
    rows.push(call(1));
    rows.push(timed(1, 4_100));
    rows.push(answer(1));
    rows.push(stop_entry(Stop::Final));

    let closed: Vec<String> = rows.rows(1).iter().map(words).collect();
    assert_eq!(
        closed,
        ["⋯ 1 call · 4.1 s"],
        "the settled work begins as its one row: {closed:?}"
    );
    let laid = rows.lays();

    assert!(
        rows.toggle_newest(),
        "the newest settled work has a row for the key to open"
    );
    assert_eq!(
        rows.lays(),
        laid,
        "opening a work lays nothing by itself: the next paint lays the one entry it opened"
    );

    let opened: Vec<String> = rows.rows(1).iter().map(words).collect();
    assert_eq!(
        opened[0], "    ✓ bash success 4.1 s",
        "the opened work is drawn under the prompt as its entries are: {opened:?}"
    );
    assert_eq!(
        rows.lays(),
        laid + 1,
        "the entry was laid out once, at the fold it was opened to"
    );

    assert!(
        rows.toggle_newest(),
        "the same key closes it again, because the work it opened is the one it closes"
    );
    let reclosed: Vec<String> = rows.rows(1).iter().map(words).collect();
    assert_eq!(
        reclosed, closed,
        "and the rows it closes to are the row the reader was handed before: {reclosed:?}"
    );

    let mut short = Rows::new();
    short.measure(Measure::new(80, Surface::none(), None));
    short.push(result(1, Exit::Code(0)));
    assert!(
        !short.toggle_newest(),
        "a work the record has not written past is no row, so the key answers nothing"
    );
}

#[test]
fn a_work_a_reader_opened_stays_open_through_a_resize_and_the_entry_after_it() {
    let mut rows = Rows::new();
    rows.measure(Measure::new(80, Surface::none(), None));
    rows.push(Entry::Prompt {
        text: SecretSet::default().redact("triage the retry test"),
    });
    rows.push(named(
        1,
        "bash",
        r#"{"command": "cargo test -p sindri --test rows -- --nocapture"}"#,
    ));
    rows.push(long_result(1));
    rows.push(answer(1));
    rows.push(stop_entry(Stop::Final));
    assert!(
        rows.toggle_newest(),
        "the settled work is one the key opens"
    );
    let opened = rows.rows(1).len();
    let laid = rows.lays();

    rows.measure(Measure::new(30, Surface::none(), None));
    let reflowed = rows.rows(1).len();
    assert_eq!(
        rows.lays(),
        laid + 1,
        "the narrower band re-lays the opened work once"
    );
    assert!(
        reflowed > opened,
        "the same work takes more rows at thirty columns than at eighty: {reflowed} against \
         {opened}"
    );

    rows.push(thought(2));
    let beside: Vec<String> = rows.rows(4).iter().map(words).collect();
    assert_eq!(
        beside,
        vec![String::from("│ turn 2")],
        "the entry beside it is the one row it is drawn as: {beside:?}"
    );
    let still: Vec<String> = rows.rows(1).iter().map(words).collect();
    assert_eq!(
        still[0], "    ✓ bash success 1.8 s",
        "the work the reader opened is still open with an entry landing after it: {still:?}"
    );

    rows.push(Entry::Prompt {
        text: SecretSet::default().redact("and the prompt after it"),
    });
    let after: Vec<String> = rows.rows(1).iter().map(words).collect();
    assert_eq!(
        after[0], "    ✓ bash success 1.8 s",
        "and the prompt after it is a word of the exchange below, so the work stands as open as \
         the reader left it: {after:?}"
    );
}

fn the_long_call() -> Rows {
    let mut rows = Rows::new();
    rows.measure(Measure::new(80, Surface::none(), None));
    rows.push(asked_for(1));
    rows.push(call(1));
    rows.push(long_result(1));
    rows.push(answer(1));
    rows.push(stop_entry(Stop::Final));
    rows
}

#[test]
fn a_row_the_mark_is_grown_over_opens_the_work_it_stands_on() {
    let mut rows = the_long_call();
    assert_eq!(
        rows.rows(1).len(),
        1,
        "the settled work is one row before the reader opens it"
    );
    assert!(
        rows.open(1),
        "a row the mark is grown over opens the work it stands on"
    );

    let opened: Vec<String> = rows.rows(1).iter().map(words).collect();
    assert_eq!(
        opened[..2],
        ["    ✓ bash success 1.8 s", "      ls"],
        "the opened work keeps the call's head and the words it was called with: {opened:?}"
    );
    assert_eq!(
        kept_back_row(rows.rows(1)).as_deref(),
        Some("    ⋯ 17 rows kept back"),
        "and the result inside it keeps its own fold, its count and the head it is handed by: \
         {opened:?}"
    );
    assert_eq!(
        opened.len(),
        7,
        "two rows of the call, four of the head and the count: {opened:?}"
    );
}

#[test]
fn a_result_inside_an_opened_work_keeps_its_own_fold() {
    let mut rows = the_long_call();
    assert!(rows.open(1), "the work the result stands in is open");
    assert!(
        rows.open(1),
        "a row inside the opened work opens the fold of the result it stands on"
    );

    let whole: Vec<String> = rows.rows(1).iter().map(words).collect();
    assert_eq!(
        kept_back_row(rows.rows(1)),
        None,
        "so the result is drawn whole and counts nothing back: {whole:?}"
    );
    assert!(
        whole.iter().any(|row| row.contains("a_row_of_output_12")),
        "and every row the fold kept back is drawn: {whole:?}"
    );
}

#[test]
fn an_exchange_that_did_no_work_draws_no_work_row() {
    let mut rows = Rows::new();
    rows.measure(Measure::new(80, Surface::none(), None));
    rows.push(Entry::Prompt {
        text: SecretSet::default().redact("triage the retry test"),
    });
    rows.push(Entry::Turn { turn: 1 });
    rows.push(thought(1));
    rows.push(answer(1));
    rows.push(stop_entry(Stop::Final));

    assert!(
        rows.rows(1).is_empty(),
        "the turn of an exchange that did no work draws no row, and no work row stands in its \
         place: {:?}",
        rows.rows(1).iter().map(words).collect::<Vec<String>>()
    );
    assert_eq!(
        rows.rows(2).iter().map(words).collect::<Vec<String>>(),
        ["│ turn 1"],
        "its thought stands where it did, because a work that made no call and read no file has no \
         number to count"
    );
    assert!(
        !rows.open(1),
        "a work that counts nothing is drawn as no row, so a row grown over it opens nothing"
    );
    assert!(
        !rows.toggle_newest(),
        "and the newest work's fold is no key's either"
    );
}

#[test]
fn a_reasoning_entry_a_run_wrote_is_drawn_from_the_transcript_it_left() {
    let root = tempfile::tempdir().expect("a temp ground");
    let workspace = root.path().join("workspace");
    let state = root.path().join("state");
    for path in [&workspace, &state] {
        std::fs::create_dir_all(path).expect("a directory");
    }
    let boundary = Boundary::from_config(&Config::default(), &workspace, Enforcement::Unsafe)
        .expect("the boundary holds");
    let limits = Limits::default();
    let secrets = SecretSet::default();
    let session = Session::create(&state).expect("a session");
    let mut record = Record::open(
        &session,
        Facts {
            wire: Wire::Anthropic,
            model: "claude-sonnet-4",
            route: None,
            thinking: false,
            boundary: &boundary,
            limits: &limits,
            workspace: &workspace,
            secrets: &secrets,
        },
    )
    .expect("a record");
    record
        .wrote(&Turn {
            reasoning: Some(String::from(
                "the fence is the question: a fence still open is code",
            )),
            ..Turn::answered(1, "read the fence rule", "a fence still open is code")
        })
        .expect("a turn is written whole");
    drop(record);

    let read = read_transcript(session.directory()).expect("the transcript reads back");
    assert!(
        matches!(read.entries().get(1), Some(Entry::Reasoning { .. })),
        "the run's own record holds the thought its turn was thought through: {:?}",
        read.entries()
    );

    let mut rows = Rows::new();
    rows.measure(Measure::new(80, Surface::none(), None));
    for entry in read.entries().iter().cloned() {
        rows.push(entry);
    }

    let thought = &rows.rows(1)[0];
    assert_eq!(
        drawn_at(thought, 0),
        Some('│'),
        "the bar of a thought is drawn from the entry the record holds: {thought:?}"
    );
    assert!(
        words(thought).contains("the fence is the question"),
        "and the words behind it are the reasoning the run wrote: {thought:?}"
    );
    assert_eq!(
        inks(thought),
        [Ink::Dim, Ink::Muted],
        "in the ink reasoning is read in: {thought:?}"
    );

    let answer = &rows.rows(2)[0];
    assert!(
        said(&painted(answer, 80)).starts_with("  a fence still open is code"),
        "an answer read back is drawn after the thought of its own turn: {answer:?}"
    );
}

#[test]
fn the_record_writes_a_turn_for_every_turn_while_the_band_draws_no_row_for_one() {
    let root = tempfile::tempdir().expect("a temp ground");
    let workspace = root.path().join("workspace");
    let state = root.path().join("state");
    for path in [&workspace, &state] {
        std::fs::create_dir_all(path).expect("a directory");
    }
    let boundary = Boundary::from_config(&Config::default(), &workspace, Enforcement::Unsafe)
        .expect("the boundary holds");
    let limits = Limits::default();
    let secrets = SecretSet::default();
    let session = Session::create(&state).expect("a session");
    let mut record = Record::open(
        &session,
        Facts {
            wire: Wire::Anthropic,
            model: "claude-sonnet-4",
            route: None,
            thinking: false,
            boundary: &boundary,
            limits: &limits,
            workspace: &workspace,
            secrets: &secrets,
        },
    )
    .expect("a record");
    for (turn, prompt, answer) in [
        (1, "read the fence rule", "a fence still open is code"),
        (2, "and the retry rule", "a retry is the run's own word"),
    ] {
        record
            .wrote(&Turn::answered(turn, prompt, answer))
            .expect("a turn is written whole");
    }
    drop(record);

    let read = read_transcript(session.directory()).expect("the transcript reads back");
    let turns: Vec<u32> = read
        .entries()
        .iter()
        .filter_map(|entry| match entry {
            Entry::Turn { turn } => Some(*turn),
            _ => None,
        })
        .collect();
    assert_eq!(
        turns,
        vec![1, 2],
        "the record writes a turn for every turn the run took: {:?}",
        read.entries()
    );

    let mut rows = Rows::new();
    rows.measure(Measure::new(80, Surface::none(), None));
    for entry in read.entries().iter().cloned() {
        rows.push(entry);
    }
    for (at, entry) in read.entries().iter().enumerate() {
        if matches!(entry, Entry::Turn { .. }) {
            assert_eq!(
                rows.rows(at).len(),
                0,
                "and the band draws no row of any of them: {:?}",
                rows.rows(at)
            );
        }
    }
}

#[test]
fn a_transcript_whose_calls_hold_no_identity_pairs_each_result_with_the_call_before_it() {
    let directory = tempfile::tempdir().expect("a session directory");
    let written = [
        thought(1),
        call(1),
        call(1),
        result(1, Exit::Code(0)),
        result(1, Exit::Code(1)),
    ];
    let lines: Vec<String> = written
        .iter()
        .map(|entry| serde_json::to_string(entry).expect("an entry is one line of a transcript"))
        .collect();
    assert!(
        !lines.iter().any(|line| line.contains("\"id\"")),
        "an entry holding no identity is the line this product wrote before identities existed: \
         {lines:?}"
    );
    std::fs::write(
        transcript_path(directory.path()),
        format!("{}\n", lines.join("\n")),
    )
    .expect("a transcript a run wrote");
    let read = read_transcript(directory.path()).expect("the transcript reads back");

    let mut rows = Rows::new();
    rows.measure(Measure::new(80, Surface::none(), None));
    for entry in read.entries().iter().cloned() {
        rows.push(entry);
    }

    assert_eq!(rows.len(), 3, "the turn and the two calls it holds");
    assert_eq!(
        said(&painted(&rows.rows(1)[1], 80)),
        "  │ ✓ bash success 2 ms",
        "with no identity to pair by, the first result settles the first call"
    );
    assert_eq!(
        said(&painted(&rows.rows(2)[0], 80)),
        "  │ ✗ bash failure exit 1 2 ms",
        "and the second settles the second, so a session from either side of this change draws"
    );
}

#[test]
fn a_transcript_of_two_calls_pairs_them_by_identity_when_it_is_read_back() {
    let directory = tempfile::tempdir().expect("a session directory");
    let written = [
        thought(1),
        call_of(1, Some("toolu_1")),
        call_of(1, Some("toolu_2")),
        result_of(1, Some("toolu_2"), Exit::Code(1)),
        result_of(1, Some("toolu_1"), Exit::Code(0)),
    ];
    let lines: Vec<String> = written
        .iter()
        .map(|entry| serde_json::to_string(entry).expect("an entry is one line of a transcript"))
        .collect();
    std::fs::write(
        transcript_path(directory.path()),
        format!("{}\n", lines.join("\n")),
    )
    .expect("a transcript a run wrote");
    let read = read_transcript(directory.path()).expect("the transcript reads back");

    let mut rows = Rows::new();
    rows.measure(Measure::new(80, Surface::none(), None));
    for entry in read.entries().iter().cloned() {
        rows.push(entry);
    }

    assert_eq!(rows.len(), 3, "the turn and the two calls it holds");
    assert_eq!(
        said(&painted(&rows.rows(1)[1], 80)),
        "  │ ✓ bash success 2 ms",
        "the identity survives the file it is written to, so the call it names settles here"
    );
    assert_eq!(
        said(&painted(&rows.rows(2)[0], 80)),
        "  │ ✗ bash failure exit 1 2 ms",
        "and the call below it takes its own result, read back in the other order"
    );
}

#[test]
fn an_answer_is_never_folded_however_long_it_runs() {
    let text: String = (1..=40)
        .map(|line| format!("line {line} of the answer.\n"))
        .collect();
    let entry = Entry::Assistant {
        turn: 1,
        text: SecretSet::default().redact(&text),
    };
    let rows = layout(&entry, 80);
    let drawn: Vec<String> = rows.iter().map(words).collect();

    assert!(
        rows.len() > 7,
        "the answer runs longer than any fold, so it has the rows a fold would cut: {drawn:?}"
    );
    assert!(
        drawn
            .iter()
            .any(|row| row.contains("line 40 of the answer")),
        "an answer is the reader's own, so it is drawn whole however long it runs and its last \
         words are drawn: {drawn:?}"
    );
    assert!(
        !drawn.iter().any(|row| row.contains("kept back")),
        "no row of an answer counts what is kept back, because none of it is: {drawn:?}"
    );
}

#[test]
fn a_call_that_exited_zero_settles_on_a_check_and_its_code() {
    let rows = layout(&result(1, Exit::Code(0)), 40);
    let header = painted(&rows[0], 40);

    assert_eq!(said(&header), "✓ bash success 2 ms");
    assert_eq!(cell_inks(&header)[0], Ink::Ok, "the check is the good ink");
    assert!(
        cell_inks(&header)[2..14]
            .iter()
            .all(|ink| *ink == Ink::Plain),
        "and the facts beside it are faded: {:?}",
        cell_inks(&header)
    );
}

#[test]
fn a_call_that_failed_settles_on_a_cross_and_the_code_it_exited_with() {
    let rows = layout(&result(1, Exit::Code(1)), 40);
    let header = painted(&rows[0], 40);

    assert_eq!(said(&header), "✗ bash failure exit 1 2 ms");
    assert_eq!(cell_inks(&header)[0], Ink::Err, "the cross is the bad ink");
}

#[test]
fn a_result_body_is_drawn_in_the_quieter_ink_that_says_it_is_not_the_answer() {
    let rows = layout(&result(1, Exit::Code(0)), 40);
    let body = drawn_inks(&rows[1..rows.len() - 1], 40);

    assert!(
        !body.is_empty(),
        "a result with output has a body under its verdict row to draw"
    );
    assert!(
        body.iter().all(|ink| *ink == Ink::Muted),
        "every cell of a result's body is drawn in the quieter ink, so a run of output reads \
         as one quiet run and never as prose the model wrote: {body:?}"
    );

    let answer = layout(&answer(1), 80);
    let words = drawn_inks(&answer, 80);

    assert!(
        !words.is_empty(),
        "the answer a run wrote is rows of its own to draw"
    );
    assert!(
        words.iter().all(|ink| *ink == Ink::Plain),
        "an answer's words keep the plain ink they wear today: {words:?}"
    );
}

#[test]
fn a_call_the_timeout_killed_says_timed_out() {
    let header = painted(&layout(&result(1, Exit::TimedOut), 40)[0], 40);

    assert_eq!(said(&header), "✗ bash failure timed out 2 ms");
    assert_eq!(cell_inks(&header)[0], Ink::Err);
}

#[test]
fn a_cancelled_call_is_a_row_and_not_a_dangling_spinner() {
    let mut rows = Rows::new();
    rows.measure(Measure::new(80, Surface::none(), None));
    rows.push(call(1));
    let in_flight = said(&painted(&rows.rows(0)[0], 80));
    assert_eq!(
        in_flight, "  │ … bash running",
        "a call that has not settled names itself and claims nothing"
    );

    rows.push(result(1, Exit::Cancelled));
    let settled = painted(&rows.rows(0)[0], 80);

    assert_eq!(
        said(&settled),
        "  │ ✗ bash failure cancelled 2 ms",
        "the row the cancelled call settles on says what stopped it"
    );
    assert_eq!(cell_inks(&settled)[4], Ink::Err);
}

#[test]
fn a_wave_of_calls_settles_its_rows_in_the_order_the_run_wrote_them() {
    let mut rows = Rows::new();
    rows.measure(Measure::new(80, Surface::none(), None));
    rows.push(call(1));
    rows.push(call(1));
    rows.push(result(1, Exit::Code(0)));
    rows.push(result(1, Exit::Code(1)));

    assert_eq!(rows.len(), 2, "two calls, two rows");
    assert_eq!(
        said(&painted(&rows.rows(0)[0], 80)),
        "  │ ✓ bash success 2 ms",
        "the first result settles the first call"
    );
    assert_eq!(
        said(&painted(&rows.rows(1)[0], 80)),
        "  │ ✗ bash failure exit 1 2 ms",
        "and the second settles the second"
    );
}

#[test]
fn a_result_settles_the_call_it_names_when_a_turn_holds_two_calls_to_one_tool() {
    let mut rows = Rows::new();
    rows.measure(Measure::new(80, Surface::none(), None));
    rows.push(call_of(1, Some("toolu_1")));
    rows.push(call_of(1, Some("toolu_2")));
    rows.push(result_of(1, Some("toolu_2"), Exit::Code(1)));
    rows.push(result_of(1, Some("toolu_1"), Exit::Code(0)));

    assert_eq!(rows.len(), 2, "two calls, two rows");
    assert_eq!(
        said(&painted(&rows.rows(0)[0], 80)),
        "  │ ✓ bash success 2 ms",
        "the first call wears the verdict of the result that named it, though that result landed \
         after the one that named the call below it"
    );
    assert_eq!(
        said(&painted(&rows.rows(1)[0], 80)),
        "  │ ✗ bash failure exit 1 2 ms",
        "and the second call wears the verdict of its own result, so neither is guessed"
    );
}

#[test]
fn a_result_naming_a_call_the_cache_never_held_opens_a_row_of_its_own() {
    let mut rows = Rows::new();
    rows.measure(Measure::new(80, Surface::none(), None));
    rows.push(call_of(1, Some("toolu_1")));
    rows.push(result_of(1, Some("toolu_2"), Exit::Code(0)));

    assert_eq!(
        rows.len(),
        2,
        "a result settles no call it does not name, so it opens a row under the one it leaves \
         waiting"
    );
    assert_eq!(
        said(&painted(&rows.rows(0)[0], 80)),
        "  │ … bash running",
        "the call the result never named still waits for its own"
    );
    assert_eq!(
        said(&painted(&rows.rows(1)[0], 80)),
        "  │ ✓ bash success 2 ms",
        "and the result stands alone, saying its verdict and the name the call row it never \
         settled would have held"
    );
}

#[test]
fn a_settled_call_keeps_its_words_above_the_row_that_says_how_it_ended() {
    let mut rows = Rows::new();
    rows.measure(Measure::new(80, Surface::none(), None));
    rows.push(call(1));
    rows.push(result(1, Exit::Code(0)));

    assert_eq!(
        rows.len(),
        1,
        "the result settled the call instead of joining it"
    );
    let drawn: Vec<String> = rows.rows(0).iter().map(words).collect();
    assert_eq!(
        drawn[..2],
        ["    ✓ bash success 2 ms", "      ls"],
        "the entry is the call's rows first, so what was run stands above what came of it: \
         {drawn:?}"
    );
    assert_eq!(
        drawn.last().unwrap(),
        "      exit 0 · 7 bytes",
        "and the result's row under them: {drawn:?}"
    );
}

#[test]
fn settling_a_call_lays_that_entry_again_and_leaves_the_history_alone() {
    let mut rows = Rows::new();
    for which in 1..=3 {
        rows.push(thought(which));
        rows.push(answer(which));
    }
    rows.push(call(4));
    rows.measure(Measure::new(80, Surface::none(), None));
    rows.window(Scroll::Tail, 40);
    let laid = rows.lays();

    let settled_at = rows.len() - 1;
    let history: Vec<Vec<Row>> = (0..settled_at).map(|at| rows.rows(at).to_vec()).collect();
    assert_eq!(
        rows.lays(),
        laid,
        "reading back what was laid out costs nothing"
    );

    rows.push(result(4, Exit::Code(1)));
    rows.window(Scroll::Tail, 40);

    assert_eq!(
        rows.lays(),
        laid + 1,
        "one entry was laid again when the call settled"
    );
    for (at, was) in history.into_iter().enumerate() {
        assert_eq!(
            rows.rows(at),
            was.as_slice(),
            "entry {at} was laid out again, and nothing above the call changed"
        );
    }
    assert_eq!(
        said(&painted(&rows.rows(settled_at)[1], 80)),
        "  │ ✗ bash failure exit 1 2 ms",
        "and the entry the call opened is the one that was laid again"
    );
    assert_eq!(
        rows.lays(),
        laid + 1,
        "reading the settled row back costs nothing either"
    );
}

#[test]
fn a_fence_that_opens_a_live_answer_claims_no_ground_of_its_own() {
    let mut rows = Rows::new();
    rows.measure(Measure::new(40, Surface::none(), None));
    rows.text("```rust\nfn main() {\n");
    rows.live(Some(&Pulse::live(4_200).head()));

    let laid = rows.rows(0);
    assert!(
        laid.len() > 1,
        "the fence opens the answer and runs past the marker's own row"
    );

    let palette = Palette::TRUECOLOR;
    let well = palette.band(Band::Well);
    for (at, row) in laid.iter().enumerate() {
        let area = Rect::new(0, u16::try_from(at).expect("a frame this short"), 40, 1);
        let mut buffer = Buffer::empty(area);
        pen::claim(&mut buffer, area, palette.ground(Some(Band::Well)));
        row.paint(
            area,
            &mut buffer,
            &palette,
            &mut Links::none(),
            &mut Sizes::none(),
        );
        let kept: Vec<Color> = (0..40).map(|x| buffer[(x, area.y)].bg).collect();
        assert_eq!(
            kept,
            [well; 40],
            "row {at} of {:?} left the well under it as it found it",
            said(&painted(row, 40))
        );
    }
}

#[test]
fn a_fence_that_opens_a_live_answer_keeps_its_picture_under_the_marker() {
    let text = "```mermaid\nflowchart TD\n  A-->B\n```\nand the prose under it\n";
    let surface = Surface::of(None, CellSize::new(10, 20), true, false);
    let mut rows = Rows::new();
    rows.measure(Measure::new(40, surface.clone(), None));
    rows.text(text);
    let head = Pulse::live(4_200).head();
    rows.live(Some(&head));

    let laid = rows.rows(0);
    let picture = laid[0]
        .picture()
        .expect("the fence opens the marked answer, so its picture stands under the marker")
        .rows();
    assert!(
        laid.len() >= usize::from(picture),
        "the rows laid are the picture's block: {picture} rows of picture, {} of answer",
        laid.len()
    );
    let marked = painted(&laid[0], 40);
    assert!(
        said(&marked).starts_with(&head.text()),
        "the marker stands in the row's own gutter, from the first column: {}",
        said(&marked)
    );

    let mut settled = Rows::new();
    settled.measure(Measure::new(40, surface, None));
    settled.push(Entry::Assistant {
        turn: 1,
        text: SecretSet::default().redact(text),
    });
    let drawn = settled.rows(0);
    assert_eq!(
        drawn.iter().position(|row| row.picture().is_some()),
        laid.iter().position(|row| row.picture().is_some()),
        "the answer settled draws its picture on the same row the marker's answer does"
    );
}

#[test]
fn a_row_set_under_a_marker_moves_the_run_it_names_with_its_words() {
    let mut rows = Rows::new();
    rows.measure(Measure::new(40, Surface::none(), None));
    rows.text("[the spec](https://example.test/spec) and the prose under it\n");
    let head = Pulse::live(4_200).head();
    rows.live(Some(&head));

    let marked = rows.rows(0)[0].clone();
    let (buffer, links) = laid(&marked, Rect::new(0, 0, 40, 1));
    let drawn: String = (0..marked.width())
        .map(|x| {
            buffer
                .cell((x, 0))
                .expect("a cell of the row")
                .symbol()
                .to_owned()
        })
        .collect();
    let at = drawn
        .find("the spec")
        .expect("the answer's own words stand in the row");
    let column = u16::try_from(drawn[..at].chars().count()).expect("a column of a row this wide");

    assert_eq!(
        links.at(0, column),
        Some("https://example.test/spec"),
        "the way begins on the first cell of `the spec`: {drawn:?}"
    );
    assert_eq!(
        links.at(0, column + 7),
        Some("https://example.test/spec"),
        "and covers its last"
    );
    assert_eq!(
        links.at(0, column + 8),
        None,
        "no cell past it is a way there"
    );
    for column in 0..=head.width() {
        assert_eq!(
            links.at(0, column),
            None,
            "column {column} is the marker's own, and names nothing"
        );
    }
}

fn laid(row: &Row, area: Rect) -> (Buffer, Links) {
    let mut buffer = Buffer::empty(area);
    let mut links = Links::none();
    row.paint(
        area,
        &mut buffer,
        &Palette::TRUECOLOR,
        &mut links,
        &mut Sizes::none(),
    );
    (buffer, links)
}

#[test]
fn a_run_of_a_row_that_names_a_destination_is_drawn_word_for_word_and_names_it_beside_the_frame() {
    let row = Row::of([
        (String::from("  "), Ink::Plain),
        (String::from("the spec"), Ink::Muted),
    ])
    .linking(2, 8, "https://example.test/spec");

    let (_, links) = laid(&row, Rect::new(0, 3, 40, 1));

    assert_eq!(
        links.at(3, 2),
        Some("https://example.test/spec"),
        "the run's first cell names the destination the frame writes before it"
    );
    assert_eq!(
        links.at(3, 9),
        Some("https://example.test/spec"),
        "and so does its last"
    );
    assert_eq!(links.at(3, 10), None, "no cell past the run is a way there");
    assert_eq!(
        links.at(3, 1),
        None,
        "and the gutter before it is no way to anywhere"
    );
    assert_eq!(
        row.width(),
        10,
        "the destination spent no column: the row is as wide as its words"
    );
}

#[test]
fn a_row_that_names_no_destination_is_drawn_as_it_was_and_lays_nothing() {
    let row = Row::of([
        (String::from("  "), Ink::Plain),
        (String::from("keys"), Ink::Plain),
    ]);

    let (buffer, links) = laid(&row, Rect::new(0, 0, 40, 1));

    assert!(
        links.is_empty(),
        "a row that names nothing leaves the frame no way to anywhere"
    );
    assert_eq!(
        buffer.cell((2, 0)).expect("a cell of the row").symbol(),
        "k",
        "and its words are drawn where they always were"
    );
}

#[test]
fn a_run_wider_than_the_row_it_was_laid_on_names_nothing_on_the_row_below() {
    let area = Rect::new(0, 0, 20, 2);
    let mut buffer = Buffer::empty(area);
    let mut links = Links::none();
    let row = Row::of([(String::from("the spec"), Ink::Plain)]).linking(0, 80, "https://x.test/s");
    row.paint(
        Rect { height: 1, ..area },
        &mut buffer,
        &Palette::TRUECOLOR,
        &mut links,
        &mut Sizes::none(),
    );
    Row::of([(String::from("and its gate"), Ink::Plain)]).paint(
        Rect {
            y: 1,
            height: 1,
            ..area
        },
        &mut buffer,
        &Palette::TRUECOLOR,
        &mut links,
        &mut Sizes::none(),
    );

    assert_eq!(links.at(0, 18), Some("https://x.test/s"));
    assert_eq!(
        links.at(0, 19),
        Some("https://x.test/s"),
        "the run covers every cell of the row it was laid on, to its last"
    );
    assert_eq!(
        links.at(1, 0),
        None,
        "and no cell of the row below it is a way anywhere: a run is one row's, and a wrap is \
         the next row's own name"
    );
}

#[test]
fn a_row_set_under_a_gutter_moves_the_run_it_names_with_its_words() {
    let row = Row::of([(String::from("the spec"), Ink::Plain)])
        .linking(0, 8, "https://example.test/spec")
        .guttered("│ ", Ink::Dim);

    let (buffer, links) = laid(&row, Rect::new(0, 0, 40, 1));

    assert_eq!(
        buffer.cell((0, 0)).expect("a cell of the row").symbol(),
        "│",
        "the gutter is the row's first column"
    );
    assert_eq!(
        links.at(0, 2),
        Some("https://example.test/spec"),
        "and the way begins where the words it covers do"
    );
    assert_eq!(
        links.at(0, 1),
        None,
        "never on the gutter the words were moved past"
    );
}

#[test]
fn a_row_set_under_a_gutter_keeps_the_picture_it_stands_on() {
    let surface = Surface::of(None, CellSize::new(10, 20), true, false);
    let rows = markdown::rows_on("```mermaid\nflowchart TD\n  A-->B\n```", 40, 0, &surface);
    let at = rows
        .iter()
        .position(|row| row.picture().is_some())
        .expect("the fence is a diagram on a terminal that places pictures");
    let guttered = rows[at].clone().guttered("│ ", Ink::Dim);

    assert_eq!(
        guttered.picture(),
        rows[at].picture(),
        "a picture is placed from the band's own edge, so a gutter moves the words and not it"
    );
    assert_eq!(
        guttered.width(),
        rows[at].width() + 2,
        "and the gutter takes its two columns of the row: {guttered:?}"
    );
    assert_eq!(
        drawn_at(&guttered, 0),
        Some('│'),
        "the gutter is the row's first column: {guttered:?}"
    );
    assert_eq!(
        words_column(&guttered),
        words_column(&rows[at]).map(|column| column + 2),
        "and the words stand past it: {guttered:?}"
    );
}

fn thumb_of(turns: u32, band: u16, scroll: Scroll) -> Option<sindri::rows::Thumb> {
    let mut rows = Rows::new();
    rows.measure(Measure::new(120, Surface::none(), None));
    for which in 1..=turns {
        rows.push(thought(which));
    }
    assert_eq!(
        rows.height(),
        turns,
        "one thought is one row of a 120-column band"
    );
    rows.thumb(scroll, band)
}

#[test]
fn the_thumb_spans_the_bands_share_of_a_transcript_taller_than_it() {
    let half = thumb_of(80, 40, Scroll::Back(40))
        .expect("a band of forty rows of an eighty-row transcript has a thumb");
    assert_eq!(
        half.span(),
        20,
        "a band of forty rows is half of an eighty-row transcript, so its thumb covers half the band"
    );

    let skinny = thumb_of(80, 36, Scroll::Back(44))
        .expect("a band of thirty-six rows of one has a thumb too");
    assert_eq!(
        skinny.span(),
        16,
        "thirty-six rows of eighty is sixteen and a fifth of the band, which a reader is shown as sixteen: a thumb rounds to the nearest cell and never to none"
    );
}

#[test]
fn the_thumb_follows_the_window_row_for_row() {
    let head =
        thumb_of(80, 36, Scroll::Back(44)).expect("a head that is off the tail draws a thumb");
    assert_eq!(
        head.rows(),
        0..16,
        "at the head of the transcript the thumb opens the band, covering the sixteen rows of it that are the whole transcript's share"
    );

    let middle = thumb_of(80, 36, Scroll::Back(22)).expect("half way back still draws a thumb");
    assert_eq!(
        middle.rows(),
        10..26,
        "half way from the head to the tail stands the thumb half way down the twenty rows the band leaves it"
    );

    let foot = thumb_of(80, 36, Scroll::Back(1)).expect("one row off the tail still draws a thumb");
    assert_eq!(
        foot.rows(),
        20..36,
        "one row off the tail stands the thumb at the foot of the band, so the reader sees it leave"
    );
}

#[test]
fn a_band_with_nothing_to_point_at_draws_no_thumb() {
    assert_eq!(
        thumb_of(80, 36, Scroll::Tail),
        None,
        "a reader following the run at the tail is shown the newest rows, and a thumb there would sit on them"
    );
    assert_eq!(
        thumb_of(36, 36, Scroll::Back(1)),
        None,
        "a transcript no taller than its band is all in view, so there is nothing to point at"
    );
    assert_eq!(
        thumb_of(80, 1, Scroll::Back(1)),
        None,
        "a band of one row has no share of the whole to point at"
    );

    for cols in 2..=3 {
        let mut narrow = Rows::new();
        narrow.measure(Measure::new(cols, Surface::none(), None));
        for which in 1..=80 {
            narrow.push(thought(which));
        }
        assert_eq!(
            narrow.thumb(Scroll::Back(44), 36),
            None,
            "a band of {cols} columns holds no gutter beside a wide glyph, which is set in the \
             first column instead: it loses the thumb before it loses a word"
        );
    }

    let mut roomy = Rows::new();
    roomy.measure(Measure::new(4, Surface::none(), None));
    for which in 1..=80 {
        roomy.push(thought(which));
    }
    assert!(
        roomy.thumb(Scroll::Back(44), 36).is_some(),
        "and four columns hold the gutter beside a word, so the thumb is drawn there"
    );
}

fn stop_entry(stop: Stop) -> Entry {
    Entry::Stop { stop }
}

fn failed() -> Stop {
    Stop::Error(SecretSet::default().redact("the wire refused\nafter the second try"))
}

#[test]
fn every_stop_but_a_final_one_is_drawn_under_its_own_word_on_the_mark_it_wears() {
    let stops = [
        (Stop::MaxTurns, "✗ max_turns", Ink::Err),
        (Stop::Budget(Budget::Clock), "✗ budget", Ink::Err),
        (Stop::Budget(Budget::Output), "✗ budget", Ink::Err),
        (Stop::Cancelled, "✗ cancelled", Ink::Err),
        (failed(), "✗ error", Ink::Err),
    ];
    for (stop, said, ink) in stops {
        let rows = layout(&stop_entry(stop.clone()), 40);

        assert_eq!(
            words(&rows[0]),
            said,
            "{} is drawn under its own word, so a reader reads why it stopped",
            stop.word()
        );
        assert_eq!(
            inks(&rows[0]),
            vec![ink, Ink::Plain, Ink::Dim],
            "the mark in the ink its verdict wears, and the word a rung quieter than it"
        );
    }
}

#[test]
fn an_error_stop_carries_one_line_of_its_words_and_no_other_stop_carries_a_line() {
    let rows = layout(&stop_entry(failed()), 60);
    let drawn: Vec<String> = rows.iter().map(words).collect();

    assert_eq!(
        drawn,
        vec![String::from("✗ error"), String::from("  the wire refused")],
        "an error carries one line of the run's own words about it, and the rest stays unsaid: \
         {drawn:?}"
    );

    for stop in [Stop::MaxTurns, Stop::Budget(Budget::Clock), Stop::Cancelled] {
        let rows = layout(&stop_entry(stop.clone()), 60);
        assert_eq!(
            rows.len(),
            1,
            "{} says its word and carries no line under it: {rows:?}",
            stop.word()
        );
    }
}

#[test]
fn a_stop_gives_up_its_mark_before_a_word_in_a_band_too_narrow_for_both() {
    let entry = stop_entry(failed());

    for row in layout(&entry, 2) {
        assert!(
            row.width() <= 2,
            "a row of two columns holds no word past them: {row:?}"
        );
        assert!(
            !row.glyphs().contains('✗'),
            "the mark is given up rather than the words: {row:?}"
        );
    }

    let narrow = layout(&entry, 3);
    let head = &narrow[0];
    assert_eq!(
        drawn_at(head, 0),
        Some('✗'),
        "three columns hold the mark: {head:?}"
    );
    assert_eq!(
        words_column(head),
        Some(2),
        "and the word the cross was given up for follows it: {head:?}"
    );
}

#[test]
fn a_stop_read_back_from_a_transcript_is_drawn_with_the_reason_a_reader_missed() {
    let directory = tempfile::tempdir().expect("a session directory");
    std::fs::write(
        transcript_path(directory.path()),
        "{\"kind\":\"stop\",\"stop\":{\"error\":\"the wire went away\"}}\n",
    )
    .expect("a transcript a run wrote");
    let read = read_transcript(directory.path()).expect("the transcript reads back");

    let drawn: Vec<String> = read
        .entries()
        .iter()
        .flat_map(|entry| layout(entry, 80))
        .map(|row| words(&row))
        .collect();

    assert_eq!(
        drawn,
        vec![
            String::from("✗ error"),
            String::from("  the wire went away")
        ],
        "a stop written by one run reads back as the reason the run stopped: {drawn:?}"
    );
}

#[test]
fn a_run_that_starts_after_a_stop_says_its_words_below_that_stops_entry() {
    let mut view = View::of(
        Capabilities::default(),
        sindri::facts::Facts::default()
            .asked()
            .pulsed(Some(Pulse::live(0))),
        Transcript::none(),
    );
    view.text("the first run's answer");
    view.handed(
        view.facts()
            .clone()
            .pulsed(Some(Pulse::settled(&Stop::Cancelled, 1_200))),
    );
    view.stopped(&Stop::Cancelled);
    let ended = frame::painted(&mut view, 60, 12).words();

    assert_eq!(
        ended[0], "✗ cancelled 1.2s the first run's answer",
        "the marker settles on the answer the run ended on: {ended:?}"
    );
    assert_eq!(
        ended[1], "✗ cancelled",
        "and the stop it ended on is the transcript's last entry till a run follows it: {ended:?}"
    );

    view.settle();
    view.handed(view.facts().clone().pulsed(Some(Pulse::live(0))));
    view.text("the second run is working");
    let after = frame::painted(&mut view, 60, 12).words();
    let at = |said: &str| {
        after
            .iter()
            .position(|row| row == said)
            .unwrap_or_else(|| panic!("the frame draws {said:?}: {after:?}"))
    };

    assert!(
        at("✗ cancelled") < at("⠋ 0.0s the second run is working"),
        "the run after it says its words under that stop rather than above it: {after:?}"
    );
}

#[test]
fn a_refusal_is_the_entry_a_stop_is_beside_the_draft_that_asked_for_it() {
    let mut view = View::of(
        Capabilities::default(),
        facts::Facts::boot(
            Boot {
                wire: None,
                model: None,
                workspace: PathBuf::from("/work/agent"),
                branch: None,
                enforcement: Enforcement::Bubblewrap,
            },
            None,
        ),
        Transcript::none(),
    );
    view.paste("why does the retry test fail");
    let Outcome::Send(prompt) = view.key(Key::Enter) else {
        panic!("a draft typed and sent is the prompt a run is asked with");
    };
    view.run_refused("cannot create the sandbox: bubblewrap is on PATH", prompt);
    let drawn = frame::painted(&mut view, 60, 12).words();
    let whole = drawn.join("\n");
    let at = |said: &str| {
        drawn
            .iter()
            .position(|row| row == said)
            .unwrap_or_else(|| panic!("the frame draws {said:?}: {whole}"))
    };
    let entry = at("✗ error");
    let draft = drawn
        .iter()
        .position(|row| row.contains("›why does the retry test fail"))
        .unwrap_or_else(|| panic!("the frame draws the draft that asked for it: {whole}"));

    assert_eq!(
        drawn[entry + 1],
        "  cannot create the sandbox: bubblewrap is on PATH",
        "a refusal is drawn as the entry a stop is, in the refusal's own words: {whole}"
    );
    assert!(
        entry < draft,
        "and the draft that asked for it is the reader's again, below it in the composer: {whole}"
    );
    assert!(
        !drawn.iter().any(|row| row.trim() == "sindri"),
        "the splash screen a launch opened on gives way to the row the refusal leaves: {whole}"
    );
}

#[test]
fn a_stop_and_the_marker_beside_it_say_the_same_word_and_never_two() {
    let stop = failed();
    let mut view = View::of(
        Capabilities::default(),
        sindri::facts::Facts::default().asked(),
        Transcript::none(),
    );
    view.text("the answer the run got before it stopped");
    view.handed(
        view.facts()
            .clone()
            .pulsed(Some(Pulse::settled(&stop, 1_240))),
    );
    view.stopped(&stop);
    let drawn = frame::painted(&mut view, 40, 12).words();
    let whole = drawn.join("\n");
    let at = |said: &str| {
        drawn
            .iter()
            .position(|row| row == said)
            .unwrap_or_else(|| panic!("the frame draws {said:?}: {whole}"))
    };
    let marker = at("✗ error 1.2s the answer the run got");
    let entry = at("✗ error");

    assert_eq!(
        drawn[marker + 1],
        "  before it stopped",
        "the marker settles on the run's answer: {whole}"
    );
    assert_eq!(
        drawn[entry + 1],
        "  the wire refused",
        "and the run's own words about it stand under the stop: {whole}"
    );
    assert!(
        marker < entry,
        "the stop the run ended on is the transcript's last entry, under its answer: {whole}"
    );
    for word in ["final", "max_turns", "budget", "cancelled"] {
        assert!(
            !whole.contains(word),
            "a frame that stopped on an error says no other reason than error: {whole}"
        );
    }
}

#[test]
fn a_streaming_answer_draws_the_definitions_that_ended_and_never_one_still_arriving() {
    let mut rows = Rows::new();
    rows.measure(Measure::new(40, Surface::none(), None));
    rows.text("a note[^1] here\n\n[^1]: the words it stands for");
    rows.live(None);

    let arriving: Vec<String> = rows.rows(0).iter().map(words).collect();
    assert_eq!(
        arriving,
        ["  a note¹ here"],
        "a definition still arriving is gathered and drawn nowhere, so its answer has no foot yet"
    );

    rows.text("\n\nand the answer goes on");
    rows.live(None);
    let ended: Vec<String> = rows.rows(0).iter().map(words).collect();
    assert_eq!(
        ended,
        [
            String::from("  a note¹ here"),
            String::new(),
            String::from("  and the answer goes on"),
            String::new(),
            format!("  {}", pen::filled(pen::columns(38), '─')),
            String::from("  ¹ the words it stands for"),
        ],
        "the definition ended, so an answer still arriving draws it at the foot behind one hairline"
    );

    rows.push(Entry::Assistant {
        turn: 1,
        text: SecretSet::default()
            .redact("a note[^1] here\n\n[^1]: the words it stands for\n\nand the answer goes on"),
    });
    assert_eq!(
        rows.rows(0).iter().map(words).collect::<Vec<String>>(),
        ended,
        "the answer read from the record draws the foot the stream was already drawing"
    );
}

fn prompt_of_lines(lines: usize) -> Entry {
    let text: Vec<String> = (1..=lines).map(|line| format!("line {line}")).collect();
    Entry::Prompt {
        text: SecretSet::default().redact(text.join("\n")),
    }
}

#[test]
fn a_prompt_longer_than_its_head_draws_its_head_and_the_count_of_rows_kept_back() {
    let rows = layout(&prompt_of_lines(12), 80);
    let drawn: Vec<String> = rows.iter().map(words).collect();

    assert_eq!(
        drawn,
        [
            String::new(),
            String::from("┃›line 1"),
            String::from("┃ line 2"),
            String::from("┃ line 3"),
            String::from("┃ line 4"),
            String::from("⋯ 8 rows kept back"),
        ],
        "a prompt past the head a fold keeps is drawn as that head and one row counting the rows \
         kept back, and never a last row to stand beside them: {drawn:?}"
    );
    assert_eq!(
        rows[5].stands_on(),
        Some(Band::Prompt),
        "the row counting a prompt's rows kept back stands on the prompt's own band, so a press on \
         it is read as a row of that prompt: {:?}",
        rows[5]
    );
    assert_eq!(
        inks(&rows[5]),
        vec![Ink::Dim, Ink::Plain, Ink::Dim],
        "and its marker and its words wear the dim ink a folded result's count wears, so a reader \
         meets one kind of fold"
    );
}

#[test]
fn a_prompt_short_enough_to_draw_whole_is_offered_no_count() {
    for lines in [1, 3, 6] {
        let rows = layout(&prompt_of_lines(lines), 80);
        let drawn: Vec<String> = rows.iter().map(words).collect();

        assert_eq!(
            drawn.len(),
            lines + 1,
            "a prompt of {lines} lines is the blank row above it and one row a line, and no row \\
             standing in for words the reader cannot see: {drawn:?}"
        );
        assert!(
            !drawn.iter().any(|row| row.contains("kept back")),
            "a prompt the band draws whole is offered no count of rows kept back, because it keeps \\
             none: {drawn:?}"
        );
        assert!(
            drawn[lines].ends_with(&format!("line {lines}")),
            "so the last line the reader typed is the last row the prompt draws: {drawn:?}"
        );
    }
}

#[test]
fn one_key_opens_a_folded_prompt_where_no_work_is_drawn_as_a_row_and_closes_it_again() {
    let mut rows = Rows::new();
    rows.measure(Measure::new(80, Surface::none(), None));
    rows.push(prompt_of_lines(12));
    rows.push(answer(1));
    rows.push(stop_entry(Stop::Final));

    let folded: Vec<String> = rows.rows(0).iter().map(words).collect();
    assert_eq!(
        folded.len(),
        5,
        "the prompt that opens the conversation draws the head of its fold with no blank row above \
         it: {folded:?}"
    );
    assert!(
        folded[4].contains("kept back"),
        "and the count of the rows it keeps back closes it: {folded:?}"
    );

    assert!(
        rows.toggle_newest(),
        "the newest fold the reader may move is this prompt's, where no work of the exchange is \
         drawn as a row for the key to take instead"
    );
    let opened: Vec<String> = rows.rows(0).iter().map(words).collect();
    assert_eq!(
        opened.len(),
        12,
        "the opened prompt draws every line the reader typed: {opened:?}"
    );
    assert!(
        !opened.iter().any(|row| row.contains("kept back")),
        "and none of it is counted back, because none of it is kept back: {opened:?}"
    );

    assert!(
        rows.toggle_newest(),
        "and the same key folds it again, because a prompt's fold is the reader's too"
    );
    let reclosed: Vec<String> = rows.rows(0).iter().map(words).collect();
    assert_eq!(
        reclosed, folded,
        "so the rows it gives back to the conversation are the ones the reader was handed before: \
         {reclosed:?}"
    );

    assert!(
        rows.open(0),
        "a row the mark is grown over opens the fold the prompt keeps, the way it opens a result's"
    );
    assert_eq!(
        rows.rows(0).len(),
        12,
        "so every line the reader typed is drawn again: {:?}",
        rows.rows(0).iter().map(words).collect::<Vec<String>>()
    );
}

#[test]
fn a_prompt_a_reader_folded_survives_a_reflow_and_counts_the_rows_the_narrower_band_keeps_back() {
    let said = "the retry test is the one to read, and the line it disagrees with is the one";
    let prompt = Entry::Prompt {
        text: SecretSet::default().redact([said; 12].join("\n")),
    };
    let mut rows = Rows::new();
    rows.measure(Measure::new(80, Surface::none(), None));
    rows.push(prompt_of_lines(2));
    rows.push(answer(1));
    rows.push(prompt);
    rows.push(answer(2));
    rows.push(stop_entry(Stop::Final));

    assert_eq!(
        kept_back_row(rows.rows(2)).as_deref(),
        Some("⋯ 8 rows kept back"),
        "at eighty columns a line of the prompt is one row, and the fold counts the eight under the \
         head it draws — the blank row above it no word of the prompt's"
    );
    assert!(
        rows.toggle_newest(),
        "the fold the reader moves is this prompt's"
    );
    let opened: Vec<String> = rows.rows(2).iter().map(words).collect();
    assert_eq!(
        opened.len(),
        13,
        "opened, the prompt draws the blank row above it and every line of its own: {opened:?}"
    );

    rows.measure(Measure::new(40, Surface::none(), None));
    let reflowed = rows.rows(2).len();
    assert!(
        reflowed > opened.len(),
        "the same words take more rows at forty columns than at eighty: {reflowed} against {}",
        opened.len()
    );
    assert_eq!(
        kept_back_row(rows.rows(2)),
        None,
        "a resize is no reader's hand on the fold, so the prompt stays open and counts nothing back"
    );

    assert!(
        rows.toggle_newest(),
        "and the key folds it again at the width it is drawn at"
    );
    let folded: Vec<String> = rows.rows(2).iter().map(words).collect();
    assert_eq!(
        kept_back_row(rows.rows(2)),
        Some(format!("⋯ {} rows kept back", reflowed - 5)),
        "so the count is the rows the narrower band keeps back, and never the count the wider one \
         told: {folded:?}"
    );
    assert_eq!(
        folded.len(),
        6,
        "the blank row above it, the head of four rows and the count under them are all it draws, \
         and no last row beside them: {folded:?}"
    );

    rows.measure(Measure::new(120, Surface::none(), None));
    let wider: Vec<String> = rows.rows(2).iter().map(words).collect();
    assert_eq!(
        kept_back_row(rows.rows(2)).as_deref(),
        Some("⋯ 8 rows kept back"),
        "and a prompt the reader left folded is still folded at the next width, counting the eight \
         rows that width keeps back: {wider:?}"
    );
    assert_eq!(
        wider.len(),
        6,
        "so the head it draws is four rows of its own words at every width, under the blank row \
         that separates it from the exchange above: {wider:?}"
    );
}
