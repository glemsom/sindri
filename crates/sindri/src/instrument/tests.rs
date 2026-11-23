use ratatui::layout::Rect;
use sindri_core::records::Usage;

use super::*;
use crate::ledger::{self, Ctx, FLOOR, Facts, Shape};
use crate::rows::margins;

#[test]
fn a_share_is_the_pixels_of_the_blocks_width_it_names() {
    assert_eq!(
        Share::of(48).pixels(120),
        58,
        "48% of a block 120 pixels wide covers 58 pixels"
    );
    assert_eq!(
        Share::of(0).pixels(120),
        0,
        "no share of a block is no pixel of it"
    );
    assert_eq!(
        Share::of(100).pixels(120),
        120,
        "the whole of it is every pixel"
    );
    assert_eq!(
        Share::of(255).pixels(120),
        120,
        "a share is a hundredth of the width, so no share of it is wider than the block"
    );
}

#[test]
fn a_share_names_the_hundredth_of_the_block_it_stands_for() {
    assert_eq!(
        Share::of(48).hundredth(),
        48,
        "a share names the hundredth of the block it covers, so 48% is the forty-eighth of them"
    );
    assert_eq!(
        Share::of(0).hundredth(),
        0,
        "no share of a block covers any hundredth of it"
    );
    assert_eq!(
        Share::of(255).hundredth(),
        100,
        "and no share of it names a hundredth past the hundredth"
    );
}

fn measured(size: u64, window: u64) -> Facts {
    Facts {
        ctx: Some(Ctx {
            size,
            window: Some(window),
        }),
        ..Facts::default()
    }
}

#[test]
fn the_gauge_is_the_pixels_of_the_window_the_run_has_filled() {
    let gauge =
        Gauge::of(&measured(48_200, 100_000)).expect("a run with a window named is a gauge");

    assert_eq!(
        gauge.share.pixels(120),
        58,
        "a window 48% full of a block 120 pixels wide covers 58 pixels"
    );
}

#[test]
fn a_run_at_the_meters_own_turn_has_turned() {
    let turned = |size: u64| {
        Gauge::of(&measured(size, 100_000))
            .expect("a run with a window named")
            .turned
    };

    assert!(
        !turned(89_000),
        "a window 89% full is drawn in the ink the meter's glyphs are calm in"
    );
    assert!(
        turned(90_000),
        "90% is where the frame's own meter glyphs turn to the accent"
    );
    assert!(
        turned(120_000),
        "and a run past the whole of its window is no calmer than one at its ceiling"
    );
}

#[test]
fn a_run_that_has_no_window_named_is_no_gauge() {
    assert_eq!(
        Gauge::of(&Facts::default()),
        None,
        "no measured window is no share of one to draw"
    );
    assert_eq!(
        Gauge::of(&Facts {
            ctx: Some(Ctx {
                size: 48_200,
                window: None,
            }),
            ..Facts::default()
        }),
        None,
        "a size with no window named is a size and no share of one"
    );
}

fn spent(input: Option<u64>, output: Option<u64>) -> Usage {
    Usage {
        input,
        output,
        ..Usage::default()
    }
}

#[test]
fn the_two_figures_of_a_spend_are_drawn_against_the_total_the_foot_states() {
    let usage = Usage {
        total: Some(51_300),
        ..spent(Some(48_200), Some(3_100))
    };
    let spend = Spend::of(&usage).expect("a spend of two figures");

    assert_eq!(
        spend.input.map(|share| share.pixels(120)),
        Some(113),
        "the input's 48.2k of a run that spent 51.3k is 94 hundredths of the block, and 113 of \
         its 120 pixels"
    );
    assert_eq!(
        spend.output.map(|share| share.pixels(120)),
        Some(7),
        "and the output's 3.1k is the 6 hundredths beside it, which is 7 pixels of the same block"
    );
    assert_eq!(
        [spend.input, spend.output].map(|share| share.map_or(0, |share| share.pixels(120))),
        [113, 7],
        "the two bars are the whole of the block between them, so the shape the foot draws is the \
         run's own split of what it spent"
    );
}

#[test]
fn a_figure_of_zero_against_a_larger_one_is_no_pixels() {
    let nothing_in = Spend::of(&spent(Some(0), Some(3_100))).expect("a spend of two figures");

    assert_eq!(
        nothing_in.input.map(|share| share.pixels(120)),
        Some(0),
        "a figure of zero spent is no pixel of the block drawn for it"
    );
    assert_eq!(
        nothing_in.output.map(|share| share.pixels(120)),
        Some(120),
        "and the figure it is drawn against is still the whole of it"
    );

    let nothing_out = Spend::of(&spent(Some(48_200), Some(0))).expect("a spend of two figures");

    assert_eq!(
        nothing_out.input.map(|share| share.pixels(120)),
        Some(120),
        "the bar that holds the larger figure is the whole of the block either way"
    );
    assert_eq!(
        nothing_out.output.map(|share| share.pixels(120)),
        Some(0),
        "and the zero is no pixel whichever of the two figures it is"
    );
}

#[test]
fn two_stated_zeros_are_two_empty_bars() {
    let spend = Spend::of(&spent(Some(0), Some(0))).expect("two stated figures are a spend");

    assert_eq!(
        [spend.input, spend.output].map(|share| share.map(|share| share.pixels(120))),
        [Some(0), Some(0)],
        "nothing spent is nothing drawn, so two zeros are two empty bars and never two full ones"
    );
}

#[test]
fn a_spend_of_one_figure_states_one_bar() {
    let spend = Spend::of(&spent(None, Some(3_100))).expect("one stated figure is a spend");

    assert_eq!(
        (spend.input, spend.output.map(|share| share.pixels(120))),
        (None, Some(120)),
        "a figure with no other to be drawn against is the whole of its bar, and the figure \
         the usage never stated is no bar at all"
    );
}

#[test]
fn a_spend_of_no_figure_at_all_is_no_bar() {
    assert_eq!(
        Spend::of(&Usage::default()),
        None,
        "a usage that states neither figure is no spend to draw"
    );
}

fn band(width: u16, height: u16) -> Rect {
    Rect {
        x: 0,
        y: 0,
        width,
        height,
    }
}

fn room(height: u16) -> Rect {
    band(FLOOR - margins(), height)
}

fn footed() -> Facts {
    Facts {
        wire: Some(String::from("anthropic")),
        model: Some(String::from("claude-sonnet-4-5")),
        spend: Usage {
            input: Some(48_200),
            output: Some(3_100),
            total: Some(51_300),
            ..Usage::default()
        },
        ..Facts::default()
    }
}

fn railways(facts: &Facts, height: u16) -> Vec<ledger::Row> {
    ledger::rows(facts, Shape::Ledger, band(FLOOR, height), None)
}

#[test]
fn the_bezel_stands_between_the_last_fact_and_the_foot() {
    let laid = railways(&footed(), 6);

    assert_eq!(
        bezel(&laid, room(6)),
        Some(Rect {
            x: 0,
            y: 2,
            width: FLOOR - margins(),
            height: 1,
        }),
        "the row of air between the last fact and the input's own row is the rail's frame"
    );
}

#[test]
fn the_bezel_of_a_rail_that_states_no_spend_runs_to_the_bands_foot() {
    let facts = Facts {
        wire: Some(String::from("anthropic")),
        model: Some(String::from("claude-sonnet-4-5")),
        ..Facts::default()
    };

    assert_eq!(
        bezel(&railways(&facts, 5), room(5)),
        Some(Rect {
            x: 0,
            y: 2,
            width: FLOOR - margins(),
            height: 3,
        }),
        "a rail whose foot states no figure keeps its frame down to the band's own last row"
    );
}

#[test]
fn the_bezel_stands_below_the_row_that_counts_the_facts_the_rail_kept_back() {
    let facts = Facts {
        branch: Some(String::from(
            "spec/a-branch-name-that-no-column-of-this-rail-can-hold",
        )),
        spend: Usage {
            input: Some(48_200),
            output: Some(3_100),
            total: Some(51_300),
            ..Usage::default()
        },
        ..Facts::default()
    };
    let laid = railways(&facts, 5);
    let counted = laid
        .iter()
        .find(|row| {
            row.parts().iter().any(
                |(_, part)| matches!(part, ledger::Part::Run(words, _) if words.starts_with('+')),
            )
        })
        .expect("the rail counts the fact it kept back");
    let framed = bezel(&laid, room(5)).expect("the air the rail leaves above its foot");

    assert!(
        framed.y > counted.y(),
        "the bezel opens on row {} and the rail counts the facts it kept back on row {}: a frame \
         stands in air and never over a word",
        framed.y,
        counted.y()
    );
    for row in laid
        .iter()
        .filter(|row| row.y() >= framed.y && row.y() < framed.bottom())
    {
        assert!(
            row.parts().is_empty(),
            "row {} of the bezel states a word, and a frame stands in air",
            row.y()
        );
    }
}

#[test]
fn the_bezel_stands_nowhere_in_a_rail_whose_rows_fill_its_band() {
    let facts = Facts {
        wire: Some(String::from("anthropic")),
        model: Some(String::from("claude-sonnet-4-5")),
        ..Facts::default()
    };

    assert_eq!(
        bezel(&railways(&facts, 2), room(2)),
        None,
        "a rail that fills its band leaves no row under its last fact to frame"
    );
    assert_eq!(
        bezel(&railways(&Facts::default(), 4), room(4)),
        None,
        "and a rail that states no fact at all has no row to begin a frame under"
    );
}
