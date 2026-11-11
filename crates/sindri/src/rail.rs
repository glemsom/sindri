//! The ledger's adapter: the band it is handed, and the rows the ledger lays on it drawn there.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::ledger::{self, Facts, Rail};
use crate::palette::{Band, Palette};
use crate::pen;

pub fn paint(facts: &Facts, rail: Rail, area: Rect, buffer: &mut Buffer, palette: &Palette) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    pen::claim(buffer, area, palette.ground(Some(Band::Rail)));
    ledger::draw(&ledger::rows(facts, rail.into(), area), buffer, palette);
}

#[cfg(test)]
mod tests {
    //! What the adapter draws of the rows the ledger lays: the buffer it paints into read back, a
    //! row of glyphs and the ink of each cell.

    use std::path::PathBuf;

    use sindri_core::policy::Enforcement;

    use super::*;
    use crate::ink::Ink;
    use crate::ledger::{COLUMNS, Ctx, FIELD, FULL_CELL, GAP, METER, tokens};
    use crate::pen;
    use crate::readback::{column_of, inks};

    /// The columns `text` draws in, which is the pen's question and not a count of its glyphs.
    fn columns(text: &str) -> usize {
        usize::from(pen::cells(text))
    }

    const PALETTE: Palette = Palette::TRUECOLOR;

    fn band(width: u16, height: u16) -> Rect {
        Rect {
            x: 0,
            y: 0,
            width,
            height,
        }
    }

    fn glyphs(buffer: &Buffer, y: u16) -> String {
        (buffer.area.x..buffer.area.right())
            .map(|x| {
                buffer
                    .cell((x, y))
                    .unwrap_or_else(|| panic!("row {y}, column {x} is outside the band"))
                    .symbol()
                    .to_owned()
            })
            .collect()
    }

    /// The dated Anthropic name a real run is served, which is wider than a value column.
    const SERVED: &str = "claude-sonnet-4-5-20250929";

    /// What the rail draws of [`SERVED`]: its head, and the mark saying the date stamp went.
    const CUT: &str = "claude-sonnet-4-5…";

    const BRANCH: &str = "spec/1-capability";

    /// The row a ctx fact of `value` is drawn on: its label in the ten-column field, and the value
    /// against the rail's last column.
    fn ctx_line(value: &str) -> String {
        let at = pen::hanging(band(COLUMNS, 1), pen::cells(value)).expect("a rail that holds it");
        format!(
            "{}{}{value}",
            pen::pad("ctx", FIELD),
            pen::blank(at.saturating_sub(FIELD))
        )
    }

    fn frame() -> Facts {
        Facts {
            wire: Some(String::from("anthropic")),
            model: Some(String::from(SERVED)),
            think: Some(String::from("high")),
            turns: Some((3, 250)),
            ctx: Some(Ctx {
                size: 48_200,
                window: None,
            }),
            cache: Some(88),
            workspace: Some(PathBuf::from(
                "/home/glemsom/Documents/git/sindri/.worktrees/46-rail",
            )),
            branch: Some(String::from("spec/1-capability")),
            loaded: vec![String::from("bash"), String::from("records")],
            sandbox: Some(Enforcement::Bubblewrap),
            session: Some(String::from("9f2c7a41-6a1e-4b3d-8f5c-0d2e7a4b9c61")),
            temp: Some(PathBuf::from(
                "/home/glemsom/.local/state/sindri/sessions/9f2c7a41-6a1e-4b3d-8f5c-0d2e7a4b9c61/tmp",
            )),
            ..Facts::default()
        }
    }

    /// Every fact the rail draws, its label and its value, stated here and not read off the module's
    /// own tables, so a test cannot pass by agreeing with the code it is testing.
    const CONTRACT: [(&str, &str); 12] = [
        ("wire", "anthropic"),
        ("model", CUT),
        ("think", "high"),
        ("turns", "3 / 250"),
        ("ctx", "48.2k"),
        ("cache", "88%"),
        ("workspace", "…/.worktrees/46-rail"),
        ("branch", "spec/1-capability"),
        ("skills", "2"),
        ("sandbox", "bwrap"),
        ("session", "9f2c7a41"),
        ("temp", "…/9f2c7a41/tmp"),
    ];

    #[test]
    fn a_runs_size_is_spelled_in_tokens_and_never_counted_out() {
        for (size, value) in [
            (0, "0"),
            (948, "948"),
            (999, "999"),
            (1_000, "1k"),
            (48_200, "48.2k"),
            (200_000, "200k"),
            (999_949, "999.9k"),
            (999_950, "1M"),
            (1_020_000, "1.02M"),
            (1_100_000, "1.1M"),
        ] {
            let facts = Facts {
                ctx: Some(Ctx { size, window: None }),
                ..Facts::default()
            };
            let mut buffer = Buffer::empty(band(COLUMNS, 1));

            paint(
                &facts,
                Rail::Ledger,
                band(COLUMNS, 1),
                &mut buffer,
                &PALETTE,
            );

            assert_eq!(
                glyphs(&buffer, 0),
                ctx_line(value),
                "{size} tokens is drawn as {value} and never as the count itself"
            );
        }
    }

    #[test]
    fn a_run_with_a_window_draws_the_window_and_the_meter_of_the_share_it_fills() {
        for (size, window, value) in [
            (0, 200_000, "░░░░░░ 0 / 200k"),
            (5_000, 200_000, "░░░░░░ 5k / 200k"),
            (48_200, 0, "48.2k"),
            (48_200, 200_000, "█░░░░░ 48.2k / 200k"),
            (100_000, 1_020_000, "█░░░░░ 100k / 1.02M"),
            (200_000, 200_000, "██████ 200k / 200k"),
            (250_000, 200_000, "██████ 250k / 200k"),
        ] {
            let facts = Facts {
                ctx: Some(Ctx {
                    size,
                    window: Some(window),
                }),
                ..Facts::default()
            };
            let mut buffer = Buffer::empty(band(COLUMNS, 1));

            paint(
                &facts,
                Rail::Ledger,
                band(COLUMNS, 1),
                &mut buffer,
                &PALETTE,
            );

            assert_eq!(
                glyphs(&buffer, 0),
                ctx_line(value),
                "{size} tokens against a window of {window} is drawn as {value}: a share under \
                 half a cell of the meter lights none of it, a run past its window has no larger \
                 share to claim, and a window of no tokens is no window at all"
            );
        }
    }

    #[test]
    fn a_nearly_full_window_is_the_one_share_the_rail_raises_its_voice_about() {
        for (size, meter, ink) in [
            (178_000, "█████░", Ink::Plain),
            (180_000, "█████░", Ink::Accent),
            (200_000, "██████", Ink::Accent),
            (240_000, "██████", Ink::Accent),
        ] {
            let facts = Facts {
                ctx: Some(Ctx {
                    size,
                    window: Some(200_000),
                }),
                ..Facts::default()
            };
            let mut ledger = Buffer::empty(band(COLUMNS, 1));

            paint(
                &facts,
                Rail::Ledger,
                band(COLUMNS, 1),
                &mut ledger,
                &PALETTE,
            );

            let filled = meter.chars().filter(|cell| *cell == FULL_CELL).count();
            assert_eq!(
                glyphs(&ledger, 0),
                ctx_line(&format!("{meter} {} / 200k", tokens(size))),
                "{size} tokens of two hundred thousand fills {filled} of the meter's six cells"
            );
            // the meter stands where a value of "178k / 200k" leaves it, one column from the edge
            let at = usize::from(COLUMNS) - METER - 1 - usize::from(pen::cells("178k / 200k"));
            for cell in 0..METER {
                let column = u16::try_from(at + cell).unwrap_or(u16::MAX);
                let drawn = Ink::of(
                    ledger.cell((column, 0)).expect("a cell of the meter"),
                    &PALETTE,
                );
                assert_eq!(
                    drawn,
                    Some(match cell < filled {
                        true => ink,
                        false => Ink::Dim,
                    }),
                    "cell {cell} of the meter {size} tokens fills is drawn in {drawn:?}: a share \
                     under ninety percent is read in `{}` and one at it in the accent, so a run \
                     one turn from a full window is the one the eye lands on",
                    ink.name()
                );
            }

            let mut strip = Buffer::empty(band(40, 1));

            paint(&facts, Rail::Strip, band(40, 1), &mut strip, &PALETTE);

            assert_eq!(
                Ink::of(strip.cell((4, 0)).expect("a cell of the meter"), &PALETTE),
                Some(ink),
                "the strip draws the same meter and reads it in the same ink the ledger does: \
                 one threshold, in one place"
            );
        }
    }

    #[test]
    fn a_frame_handed_no_size_draws_no_ctx_row_and_counts_nothing_for_it() {
        let mut unknown = frame();
        unknown.ctx = None;
        let mut roomy = Buffer::empty(band(COLUMNS, 17));

        paint(
            &unknown,
            Rail::Ledger,
            band(COLUMNS, 17),
            &mut roomy,
            &PALETTE,
        );

        let rows: Vec<String> = (0..17).map(|y| glyphs(&roomy, y)).collect();
        assert_eq!(
            rows[..7],
            [
                "wire                   anthropic",
                "model         claude-sonnet-4-5…",
                "think                       high",
                "────────────────────────────────",
                "turns                    3 / 250",
                "cache                        88%",
                "────────────────────────────────",
            ],
            "a frame before the first reply has no size, so it draws no ctx row and the \
             block closes up under the turns"
        );
        assert!(
            !rows.iter().any(|row| row.contains("more")),
            "and the rail gave nothing up, so it counts nothing: no row may say so"
        );

        let band = band(COLUMNS, 8);
        let mut tight = Buffer::empty(band);

        paint(&frame(), Rail::Ledger, band, &mut tight, &PALETTE);
        let with = glyphs(&tight, 7);

        paint(&unknown, Rail::Ledger, band, &mut tight, &PALETTE);
        let without = glyphs(&tight, 7);

        assert_eq!(
            (with.trim_end(), without.trim_end()),
            ("+7 more", "+6 more"),
            "the count is of the facts the frame knew: the size joining the frame is one \
             more fact given up at this height and no fact the rail draws, so its absence \
             leaves the count one shorter and never one longer"
        );
    }

    #[test]
    fn the_ledger_draws_whether_the_model_thinks_below_the_model_it_belongs_to() {
        let facts = Facts {
            wire: Some(String::from("anthropic")),
            model: Some(String::from("claude-sonnet")),
            think: Some(String::from("high")),
            ..Facts::default()
        };
        let mut buffer = Buffer::empty(band(COLUMNS, 3));

        paint(
            &facts,
            Rail::Ledger,
            band(COLUMNS, 3),
            &mut buffer,
            &PALETTE,
        );

        let rows: Vec<String> = (0..3).map(|y| glyphs(&buffer, y)).collect();
        assert_eq!(
            rows,
            [
                "wire                   anthropic",
                "model              claude-sonnet",
                "think                       high",
            ],
            "the model block holds the fact, one row below the model it is about"
        );
    }

    #[test]
    fn the_ledger_reads_its_blocks_in_the_contracts_order_with_a_hairline_between_two() {
        let mut buffer = Buffer::empty(band(COLUMNS, 17));

        paint(
            &frame(),
            Rail::Ledger,
            band(COLUMNS, 17),
            &mut buffer,
            &PALETTE,
        );

        let rows: Vec<String> = (0..17).map(|y| glyphs(&buffer, y)).collect();
        assert_eq!(
            rows,
            [
                "wire                   anthropic",
                "model         claude-sonnet-4-5…",
                "think                       high",
                "────────────────────────────────",
                "turns                    3 / 250",
                "ctx                        48.2k",
                "cache                        88%",
                "────────────────────────────────",
                "workspace   …/.worktrees/46-rail",
                "branch         spec/1-capability",
                "────────────────────────────────",
                "skills                         2",
                "────────────────────────────────",
                "sandbox                    bwrap",
                "────────────────────────────────",
                "session                 9f2c7a41",
                "temp              …/9f2c7a41/tmp",
            ],
            "six blocks, a hairline between two of them, nothing given up at this width"
        );
        assert_eq!(
            inks(&buffer, 3, &PALETTE),
            [(Ink::Dim, 32)],
            "the hairline runs the rail's width"
        );
        assert_eq!(
            inks(&buffer, 4, &PALETTE),
            [(Ink::Dim, 10), (Ink::Plain, 22)],
            "the label field is `dim`, the ground stands between it and the value, and the \
             value is `plain`"
        );
    }

    #[test]
    fn a_frame_handed_no_wire_draws_no_wire_row() {
        let facts = Facts {
            wire: None,
            model: Some(String::from("claude-sonnet")),
            ..Facts::default()
        };
        let mut buffer = Buffer::empty(band(COLUMNS, 2));

        paint(
            &facts,
            Rail::Ledger,
            band(COLUMNS, 2),
            &mut buffer,
            &PALETTE,
        );

        assert_eq!(
            (glyphs(&buffer, 0).trim_end(), glyphs(&buffer, 1).trim_end()),
            ("model              claude-sonnet", ""),
            "a rail handed no wire has no wire row, and nothing went, so no row counts it"
        );
    }

    #[test]
    fn a_ledger_row_draws_the_wire_by_its_word_and_nothing_beside_it() {
        for (word, row) in [
            ("anthropic", "wire                   anthropic"),
            ("openai", "wire                      openai"),
            ("copilot", "wire                     copilot"),
        ] {
            let facts = Facts {
                wire: Some(String::from(word)),
                ..Facts::default()
            };
            let mut buffer = Buffer::empty(band(COLUMNS, 1));

            paint(
                &facts,
                Rail::Ledger,
                band(COLUMNS, 1),
                &mut buffer,
                &PALETTE,
            );

            assert_eq!(
                glyphs(&buffer, 0),
                row,
                "the label in its field and the word against the rail's last column, and nothing else"
            );
        }
    }

    #[test]
    fn a_fact_with_no_cut_rule_of_its_own_is_dropped_when_no_column_is_left_for_it() {
        let facts = Facts {
            branch: Some(String::from("spec/a-branch-name-that-cannot-fit")),
            ..Facts::default()
        };
        let mut buffer = Buffer::empty(band(COLUMNS, 2));

        paint(
            &facts,
            Rail::Ledger,
            band(COLUMNS, 2),
            &mut buffer,
            &PALETTE,
        );

        assert_eq!(
            glyphs(&buffer, 0),
            "+1 more                         ",
            "a branch wider than a thirty-two column rail leaves no room for its value, so \
             the fact goes and the row says so"
        );
        assert!(
            !glyphs(&buffer, 0).contains("spec/a"),
            "and never the start of it in the fact's place"
        );
    }

    #[test]
    fn a_served_name_longer_than_the_column_is_drawn_by_its_head_and_its_mark() {
        let facts = Facts {
            model: Some(String::from("claude-sonnet-4-5-20250929")),
            ..Facts::default()
        };
        let mut ledger = Buffer::empty(band(COLUMNS, 1));

        paint(
            &facts,
            Rail::Ledger,
            band(COLUMNS, 1),
            &mut ledger,
            &PALETTE,
        );

        assert_eq!(
            glyphs(&ledger, 0),
            "model         claude-sonnet-4-5…",
            "a dated name takes the value column whole, and the mark says the date stamp went"
        );

        let mut strip = Buffer::empty(band(40, 1));

        paint(&facts, Rail::Strip, band(40, 1), &mut strip, &PALETTE);

        assert_eq!(
            glyphs(&strip, 0).trim_end(),
            "model claude-sonnet-4-5…",
            "and the strip's copy of the same name is cut the same way"
        );
    }

    #[test]
    fn a_served_name_with_no_segment_to_keep_is_dropped_and_counted() {
        let facts = Facts {
            model: Some(String::from("aservednamethatnobreaksanywhere")),
            ..Facts::default()
        };
        let mut ledger = Buffer::empty(band(COLUMNS, 2));

        paint(
            &facts,
            Rail::Ledger,
            band(COLUMNS, 2),
            &mut ledger,
            &PALETTE,
        );

        assert_eq!(
            glyphs(&ledger, 0),
            "+1 more                         ",
            "a name cut to nothing says nothing, so the fact goes and the row counts it"
        );
        assert!(
            !glyphs(&ledger, 0).contains("aserved"),
            "and never the start of it in the fact's place"
        );

        let mut strip = Buffer::empty(band(40, 1));

        paint(&facts, Rail::Strip, band(40, 1), &mut strip, &PALETTE);

        assert_eq!(
            glyphs(&strip, 0).trim_end(),
            "+1 more",
            "and the strip gives up the same fact, and says so in the same words"
        );
    }

    #[test]
    fn a_band_too_short_for_every_block_gives_facts_up_in_the_sacrifice_order() {
        let mut buffer = Buffer::empty(band(COLUMNS, 8));

        paint(
            &frame(),
            Rail::Ledger,
            band(COLUMNS, 8),
            &mut buffer,
            &PALETTE,
        );

        let rows: Vec<String> = (0..8).map(|y| glyphs(&buffer, y)).collect();
        assert_eq!(
            rows,
            [
                "wire                   anthropic",
                "model         claude-sonnet-4-5…",
                "think                       high",
                "────────────────────────────────",
                "turns                    3 / 250",
                "────────────────────────────────",
                "sandbox                    bwrap",
                "+7 more                         ",
            ],
            "temp, cache, session, branch, skills, ctx and workspace went, in that \
             order, and the foot of the rail says seven"
        );
    }

    #[test]
    fn a_band_that_can_only_hold_the_model_block_draws_the_model_without_its_think_row() {
        let mut buffer = Buffer::empty(band(COLUMNS, 4));

        paint(
            &frame(),
            Rail::Ledger,
            band(COLUMNS, 4),
            &mut buffer,
            &PALETTE,
        );

        let rows: Vec<String> = (0..4).map(|y| glyphs(&buffer, y)).collect();
        assert_eq!(
            rows,
            [
                "model         claude-sonnet-4-5…",
                "────────────────────────────────",
                "sandbox                    bwrap",
                "+10 more                        ",
            ],
            "thinking is given up before the model it sits under, so the model keeps its row"
        );
    }

    #[test]
    fn a_strip_carries_the_share_of_a_named_window_and_gives_up_the_size_beside_it() {
        let facts = Facts {
            ctx: Some(Ctx {
                size: 48_200,
                window: Some(200_000),
            }),
            ..Facts::default()
        };
        let mut ledger = Buffer::empty(band(COLUMNS, 1));

        paint(
            &facts,
            Rail::Ledger,
            band(COLUMNS, 1),
            &mut ledger,
            &PALETTE,
        );

        assert_eq!(
            glyphs(&ledger, 0),
            "ctx          █░░░░░ 48.2k / 200k",
            "the ledger's ctx row says the size against the window it is a share of, and draws \
             that share as a meter beside them"
        );

        let mut strip = Buffer::empty(band(40, 1));

        paint(&facts, Rail::Strip, band(40, 1), &mut strip, &PALETTE);

        assert_eq!(
            glyphs(&strip, 0).trim_end(),
            "ctx █░░░░░",
            "the strip's one row has no room for the size or the window it is a share of, so \
             it keeps the meter that says how much of the window is left"
        );
    }

    #[test]
    fn forty_columns_hold_the_fact_a_reader_watches_and_the_model_they_are() {
        let mut buffer = Buffer::empty(band(40, 1));

        paint(&frame(), Rail::Strip, band(40, 1), &mut buffer, &PALETTE);

        assert_eq!(
            glyphs(&buffer, 0),
            "ctx 48.2k · model claude-sonnet-4-5…    ",
            "forty columns hold the run's size and the model, because the four facts of the \
             strip's list are all it carries and the count does not fit beside the two"
        );
    }

    #[test]
    fn ninety_nine_columns_hold_the_four_facts_and_count_the_ones_they_left_out() {
        let mut facts = frame();
        facts.ctx = Some(Ctx {
            size: 48_200,
            window: Some(200_000),
        });
        let mut buffer = Buffer::empty(band(99, 1));

        paint(&facts, Rail::Strip, band(99, 1), &mut buffer, &PALETTE);

        assert_eq!(
            glyphs(&buffer, 0).trim_end(),
            format!("ctx █░░░░░ · model {CUT} · sandbox bwrap · branch {BRANCH} · +8 more"),
            "ninety-nine columns hold the whole list and the count of the eight facts the \
             strip does not carry at all"
        );
    }

    #[test]
    fn the_strip_gives_its_facts_up_from_the_tail_of_its_own_list() {
        let mut counted = Buffer::empty(band(47, 1));

        paint(&frame(), Rail::Strip, band(47, 1), &mut counted, &PALETTE);

        assert_eq!(
            glyphs(&counted, 0).trim_end(),
            "ctx 48.2k · model claude-sonnet-4-5… · +10 more",
            "the branch and the sandbox posture went, and the count that fits says so"
        );

        let mut facts_win = Buffer::empty(band(60, 1));

        paint(&frame(), Rail::Strip, band(60, 1), &mut facts_win, &PALETTE);

        assert_eq!(
            glyphs(&facts_win, 0).trim_end(),
            "ctx 48.2k · model claude-sonnet-4-5… · sandbox bwrap",
            "three facts beat the count: the room the count would take is the room the \
             sandbox posture is drawn in"
        );
    }

    #[test]
    fn a_strip_with_one_fact_draws_it_and_counts_the_fact_it_does_not_carry() {
        let one = Facts {
            sandbox: Some(Enforcement::Bubblewrap),
            ..Facts::default()
        };
        let mut buffer = Buffer::empty(band(40, 1));

        paint(&one, Rail::Strip, band(40, 1), &mut buffer, &PALETTE);

        assert_eq!(
            glyphs(&buffer, 0).trim_end(),
            "sandbox bwrap",
            "the one fact the frame knows is drawn, and no count stands where nothing went"
        );

        let elsewhere = Facts {
            wire: Some(String::from("anthropic")),
            ..Facts::default()
        };
        let mut buffer = Buffer::empty(band(40, 1));

        paint(&elsewhere, Rail::Strip, band(40, 1), &mut buffer, &PALETTE);

        assert_eq!(
            glyphs(&buffer, 0).trim_end(),
            "+1 more",
            "a fact the strip does not carry is counted rather than drawn, because the \
             strip's four are the whole of what its row reads"
        );
    }

    #[test]
    fn no_fact_is_ever_drawn_partially_at_any_width_between_twenty_and_two_hundred() {
        for width in 20..=200u16 {
            let rail = Rail::of(width);
            let area = match rail {
                Rail::Ledger => band(COLUMNS, 40),
                Rail::Strip => band(width, 1),
            };
            let mut buffer = Buffer::empty(area);
            paint(&frame(), rail, area, &mut buffer, &PALETTE);
            let rows: Vec<String> = (0..area.height).map(|y| glyphs(&buffer, y)).collect();

            let mut went = 0;
            for (label, value) in CONTRACT {
                match rows.iter().find(|row| row.contains(label)) {
                    Some(row) => assert!(
                        row.contains(value),
                        "at {width} columns the {label} row reads {row:?}, which is not \
                         the whole {value:?}"
                    ),
                    None => went += 1,
                }
            }

            let more = format!("+{went} more");
            let screen = rows.join("\n");
            if went == 0 {
                assert!(
                    !screen.contains("more"),
                    "at {width} columns nothing went, so no row may count what did"
                );
                continue;
            }
            if screen.contains(&more) {
                continue;
            }
            assert_eq!(
                rail,
                Rail::Strip,
                "at {width} columns the ledger dropped the {more} row it had room to draw"
            );
            let drawn = columns(screen.trim_end());
            assert!(
                drawn + columns(GAP) + columns(&more) > usize::from(width),
                "at {width} columns the strip dropped the {more} count it had room to draw \
                 beside the {drawn} columns it kept"
            );
        }
    }

    #[test]
    fn a_strip_row_is_the_four_facts_of_its_list_beside_each_other_with_one_gap() {
        // 220 columns hold the four facts and the count beside them, so the order is read
        // without the room deciding any of it.
        let mut buffer = Buffer::empty(band(220, 1));

        paint(&frame(), Rail::Strip, band(220, 1), &mut buffer, &PALETTE);

        assert_eq!(
            glyphs(&buffer, 0).trim_end(),
            format!("ctx 48.2k · model {CUT} · sandbox bwrap · branch {BRANCH} · +8 more"),
            "one row, the strip's own four facts in its own order, a gap between two of \
             them, and the count of the eight it does not carry even with the room to"
        );

        let mut narrower = Buffer::empty(band(88, 1));

        paint(&frame(), Rail::Strip, band(88, 1), &mut narrower, &PALETTE);

        assert_eq!(
            glyphs(&narrower, 0).trim_end(),
            format!("ctx 48.2k · model {CUT} · sandbox bwrap · branch {BRANCH}"),
            "eighty-eight columns hold the four facts and no count: the count's own \
             eleven columns are the ones this row does not have"
        );
    }

    #[test]
    fn a_ledger_row_sets_its_label_dim_and_its_value_plain_on_one_hanging_edge() {
        let facts = Facts {
            model: Some(String::from("claude-sonnet")),
            think: Some(String::from("high")),
            ..Facts::default()
        };
        let mut buffer = Buffer::empty(band(COLUMNS, 2));

        paint(
            &facts,
            Rail::Ledger,
            band(COLUMNS, 2),
            &mut buffer,
            &PALETTE,
        );

        assert_eq!(
            glyphs(&buffer, 0),
            "model              claude-sonnet",
            "the label in its ten-column field and the value against the rail's last column"
        );
        assert_eq!(
            glyphs(&buffer, 1),
            "think                       high",
            "and a value of another width ends in that same column: one hanging edge, and \
             no run of leaders out to it"
        );
        assert_eq!(
            inks(&buffer, 0, &PALETTE),
            [(Ink::Dim, 10), (Ink::Plain, 22)],
            "the field is `dim` and the value is the fact, so it is `plain`"
        );
    }

    #[test]
    fn no_width_of_rail_draws_a_run_of_leaders() {
        for width in 20..=200u16 {
            let rail = Rail::of(width);
            let area = match rail {
                Rail::Ledger => band(COLUMNS, 40),
                Rail::Strip => band(width, 1),
            };
            let mut buffer = Buffer::empty(area);

            paint(&frame(), rail, area, &mut buffer, &PALETTE);

            let screen: Vec<String> = (0..area.height).map(|y| glyphs(&buffer, y)).collect();
            assert!(
                !screen
                    .iter()
                    .any(|row| row.contains("..") || row.contains(". ")),
                "at {width} columns a row reads {screen:?}: the space between a label and its \
                 value is blank, and no leader run stands in it"
            );
        }
    }

    #[test]
    fn a_window_is_drawn_as_a_meter_beside_the_size_it_is_a_share_of() {
        let facts = Facts {
            turns: Some((3, 250)),
            ctx: Some(Ctx {
                size: 48_200,
                window: Some(200_000),
            }),
            cache: Some(88),
            ..Facts::default()
        };
        let mut buffer = Buffer::empty(band(COLUMNS, 3));

        paint(
            &facts,
            Rail::Ledger,
            band(COLUMNS, 3),
            &mut buffer,
            &PALETTE,
        );

        let rows: Vec<String> = (0..3).map(|y| glyphs(&buffer, y)).collect();
        assert_eq!(
            rows,
            [
                "turns                   3 /  250",
                "ctx          █░░░░░ 48.2k / 200k",
                "cache                        88%",
            ],
            "a quarter of the window is one cell of the meter and the size is the number beside \
             it, and the hit rate is no meter at all"
        );
        assert_eq!(
            column_of(&rows[0], '/'),
            column_of(&rows[1], '/'),
            "the two halves of a pair line up: every slash in the block lands in one column"
        );
    }
}
