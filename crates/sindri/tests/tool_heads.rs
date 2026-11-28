mod caps;
mod frame;

use sindri::facts::Facts;
use sindri::palette::Palette;
use sindri::view::{Transcript, View};
use sindri_core::records::Entry;
use sindri_core::redact::SecretSet;
use sindri_core::run::Exit;

fn call() -> Entry {
    Entry::Call {
        turn: 1,
        id: Some(SecretSet::default().redact("work")),
        name: SecretSet::default().redact("bash"),
        arguments: SecretSet::default().redact(r#"{"command":"printf hello"}"#),
    }
}

fn result(exit: Option<Exit>, text: &str) -> Entry {
    Entry::Result {
        turn: 1,
        id: Some(SecretSet::default().redact("work")),
        name: SecretSet::default().redact("bash"),
        text: SecretSet::default().redact(text),
        bytes: 987,
        elapsed_ms: 12345,
        exit,
    }
}

#[test]
fn paired_work_has_one_outcome_and_bytes_below_the_invocation_on_every_capability_path() {
    for capabilities in [
        caps::pictured(),
        caps::greys(),
        caps::uncoloured(),
        caps::inked(Palette::TRUECOLOR),
        caps::inked(Palette::ANSI),
        caps::inked(Palette::SLOTS),
    ] {
        let mut view = View::of(
            capabilities,
            Facts::default().asked().running(),
            Transcript::of(vec![call(), result(Some(Exit::Code(0)), "hello")]),
        );
        view.key(sindri::composer::Key::Fold);
        let shown = frame::painted(&mut view, 80, 24);
        let words = shown.text("tools");
        assert_eq!(words.matches('✓').count(), 1, "{words}");
        assert_eq!(words.matches("success").count(), 0, "{words}");
        let head = (0..24)
            .map(|y| shown.row(y))
            .find(|row| row.contains("✓"))
            .unwrap();
        assert!(!head.contains("bytes"), "{head}");
        assert!(words.contains("987 bytes"), "{words}");
    }
}

#[test]
fn live_failure_absent_exit_and_unmatched_work_keep_required_words_at_narrow_widths() {
    for width in [120, 80, 40] {
        for (entries, expected) in [
            (vec![call()], "running"),
            (
                vec![call(), result(Some(Exit::Code(7)), "denied")],
                "exit 7",
            ),
            (vec![call(), result(None, "")], "printf"),
            (vec![result(None, "orphan")], "orphan"),
        ] {
            let mut view = View::of(
                caps::inked(Palette::SLOTS),
                Facts::default().asked().running(),
                Transcript::of(entries),
            );
            let shown = frame::painted(&mut view, width, 24);
            let words = (0..20)
                .map(|y| shown.row(y).trim().to_owned())
                .collect::<String>();
            if width >= 40 && expected == "completed" {
                assert!(words.contains("bash"), "{words}");
            }
            assert!(words.contains(expected), "{words}");
            if expected == "exit 7" {
                assert!(words.contains("exit 7"), "{words}");
            }
            if expected == "completed" {
                assert!(!words.contains("exit"), "{words}");
            }
            if width <= 40 {
                assert!(!words.contains("12.3"), "{words}");
            }
        }
    }
}

#[test]
fn result_rows_preserve_unicode_blank_lines_and_column_relationships() {
    let mut view = View::of(
        caps::pictured(),
        Facts::default().asked().running(),
        Transcript::of(vec![call(), result(Some(Exit::Code(0)), "界  x\n\n    y")]),
    );
    view.key(sindri::composer::Key::Fold);
    let shown = frame::painted(&mut view, 80, 24);
    let band = shown
        .told()
        .layout()
        .expect("a painted frame lays the bands it draws")
        .transcript;
    let rows = (0..20).map(|y| shown.row(y)).collect::<Vec<_>>();
    let first = rows.iter().position(|row| row.contains("界")).unwrap();
    let words = band.x + 4;
    let row = u16::try_from(first).expect("a row of the frame");
    assert_eq!(shown.column_of(row, 'x'), Some(words + 4));
    assert!(
        rows[first + 1]
            .trim_matches(|c: char| c.is_whitespace() || c == '│')
            .is_empty()
    );
    assert_eq!(shown.column_of(row + 2, 'y'), Some(words + 4));
}

#[test]
fn copying_invocation_and_result_returns_original_source_without_head_decoration() {
    use sindri::composer::{Key, Outcome};
    use sindri::mark::Cell;
    let command = "printf '界'\n\n  printf '  x'";
    let output = "界  x\n\n    y";
    let mut invocation = call();
    if let Entry::Call { arguments, .. } = &mut invocation {
        *arguments =
            SecretSet::default().redact(serde_json::json!({"command": command}).to_string());
    }
    let mut view = View::of(
        caps::inked(Palette::SLOTS),
        Facts::default().asked().running(),
        Transcript::of(vec![invocation, result(Some(Exit::Code(0)), output)]),
    );
    view.key(sindri::composer::Key::Fold);
    let shown = frame::painted(&mut view, 80, 24);
    let row = (0..20).find(|y| shown.row(*y).contains("printf")).unwrap();
    view.drag(Cell::new(row, 2), Cell::new(row, 25));
    frame::painted(&mut view, 80, 24);
    assert_eq!(view.key(Key::Copy), Outcome::Unchanged);
    let shown = frame::painted(&mut view, 80, 24);
    let row = (0..20)
        .find(|y| shown.row(*y).contains("界") && !shown.row(*y).contains("printf"))
        .unwrap();
    view.drag(Cell::new(row, 2), Cell::new(row, 25));
    frame::painted(&mut view, 80, 24);
    assert_eq!(view.key(Key::Copy), Outcome::Unchanged);
}

#[test]
fn invocation_result_and_fold_stand_behind_one_rule_at_one_column_on_every_frame_path() {
    for capabilities in [
        caps::pictured(),
        caps::greys(),
        caps::uncoloured(),
        caps::inked(Palette::TRUECOLOR),
        caps::inked(Palette::ANSI),
        caps::inked(Palette::SLOTS),
    ] {
        for (width, height) in [(120, 40), (80, 24), (40, 24), (12, 24)] {
            let mut view = View::of(
                capabilities.clone(),
                Facts::default().asked().running(),
                Transcript::of(vec![
                    call(),
                    result(
                        Some(Exit::Code(0)),
                        "one\ntwo\nthree\nfour\nfive\nsix\nseven\neight",
                    ),
                ]),
            );
            view.key(sindri::composer::Key::Fold);
            let shown = frame::painted(&mut view, width, height);
            let head = (0..height).find(|y| shown.row(*y).contains("✓")).unwrap();
            let fold = (0..height).find(|y| shown.row(*y).contains("⋯")).unwrap();
            if width > 12 {
                let rule = shown
                    .column_of(head, '│')
                    .expect("the one rule a call draws");
                for y in head..=fold {
                    let row = shown.row(y);
                    assert_eq!(shown.glyph(rule, y), '│', "{row}");
                    assert_eq!(row.matches('│').count(), 1, "{row}");
                }
                assert!((head..=fold).all(|y| !shown.row(y).trim().is_empty()));
            } else {
                assert!(!shown.row(head).contains('│'));
            }
        }
    }
}

#[test]
fn opening_result_work_preserves_source_and_fold_choice_after_resize() {
    use sindri::composer::{Key, Outcome};
    use sindri::mark::Cell;
    for capabilities in [
        caps::pictured(),
        caps::greys(),
        caps::uncoloured(),
        caps::inked(Palette::TRUECOLOR),
        caps::inked(Palette::ANSI),
        caps::inked(Palette::SLOTS),
    ] {
        for (width, height) in [(120, 40), (80, 24), (40, 24), (12, 24)] {
            let output = "one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\nten";
            let mut view = View::of(
                capabilities.clone(),
                Facts::default().asked().running(),
                Transcript::of(vec![call(), result(Some(Exit::Code(0)), output)]),
            );
            view.key(sindri::composer::Key::Fold);
            let shown = frame::painted(&mut view, width, height);
            let row = (0..height).find(|y| shown.row(*y).contains("one")).unwrap();
            view.drag(Cell::new(row, 2), Cell::new(row, width - 3));
            frame::painted(&mut view, width, height);
            let shown = frame::painted(&mut view, width, height);
            let row = (0..height).find(|y| shown.row(*y).contains("one")).unwrap();
            view.drag(Cell::new(row, 2), Cell::new(row, width - 3));
            frame::painted(&mut view, width, height);
            assert_eq!(view.key(Key::Copy), Outcome::Unchanged);
            let shown = frame::painted(&mut view, 80, 24);
            assert!((0..24).any(|y| shown.row(y).contains("one")));
            assert!((0..24).any(|y| shown.row(y).contains("kept back")));
        }
    }
}

#[test]
fn opened_short_call_keeps_all_evidence_and_reader_controls_in_every_capability_and_size() {
    use sindri::composer::{Key, Outcome};
    use sindri::mark::Cell;
    for capabilities in [
        caps::pictured(),
        caps::greys(),
        caps::uncoloured(),
        caps::inked(Palette::TRUECOLOR),
        caps::inked(Palette::ANSI),
        caps::inked(Palette::SLOTS),
    ] {
        for (width, height) in [(120, 40), (80, 24), (40, 24), (12, 24)] {
            let mut view = View::of(
                capabilities.clone(),
                Facts::default().asked().running(),
                Transcript::of(vec![call(), result(Some(Exit::Code(0)), "界\n\n  x")]),
            );
            view.key(sindri::composer::Key::Fold);
            let shown = frame::painted(&mut view, width, height);
            let words = (0..height)
                .map(|y| shown.row(y).trim().to_owned())
                .collect::<String>();
            assert!(words.contains("✓"));
            assert!(!words.contains("success"));
            if width > 12 {
                assert!(words.contains("987 bytes"));
            }
            assert!(!words.contains("kept back"));
            let row = (0..height).find(|y| shown.row(*y).contains("界")).unwrap();
            view.drag(Cell::new(row, 0), Cell::new(row, width - 1));
            frame::painted(&mut view, width, height);
            assert_eq!(view.key(Key::Copy), Outcome::Unchanged);
            let open = frame::painted(&mut view, 80, 24);
            assert!(!(0..24).any(|y| open.row(y).contains("kept back")));
            let row = (0..24).find(|y| open.row(*y).contains("界")).unwrap();
            view.drag(Cell::new(row, 2), Cell::new(row, 20));
            frame::painted(&mut view, 80, 24);
            assert_eq!(view.key(Key::Fold), Outcome::Edited);
        }
    }
}

#[test]
fn reader_opened_call_keeps_source_through_exchange_scroll_resize_and_capability_changes() {
    use sindri::composer::{Key, Outcome};
    use sindri::mark::Cell;
    let output = "界\n\n  x";
    let mut entries = vec![Entry::Prompt {
        text: SecretSet::default().redact("inspect work"),
    }];
    entries.extend([call(), result(Some(Exit::Code(0)), output)]);
    entries.push(Entry::Assistant {
        turn: 1,
        text: SecretSet::default().redact("The answer stays on the well."),
    });
    entries.push(Entry::Stop {
        stop: sindri_core::run::Stop::Final,
    });
    let mut view = View::of(
        caps::pictured().faced(false),
        Facts::default().asked(),
        Transcript::of(entries),
    );
    frame::painted(&mut view, 120, 40);
    assert_eq!(view.key(Key::Fold), Outcome::Edited);
    frame::painted(&mut view, 120, 40);
    view.key(Key::Fold);
    let shown = frame::painted(&mut view, 120, 40);
    let row = (0..40).find(|y| shown.row(*y).contains("界")).unwrap();
    view.drag(Cell::new(row, 0), Cell::new(row, 119));
    frame::painted(&mut view, 120, 40);
    assert_eq!(view.key(Key::Copy), Outcome::Unchanged);
    for capabilities in [
        caps::pictured(),
        caps::greys(),
        caps::uncoloured(),
        caps::inked(Palette::TRUECOLOR),
        caps::inked(Palette::ANSI),
        caps::inked(Palette::SLOTS),
    ] {
        view.drawn_on(capabilities.faced(false));
        for (width, height) in [(120, 40), (80, 24), (40, 24), (12, 24)] {
            frame::painted(&mut view, width, height);
            view.key(Key::PageUp);
            frame::painted(&mut view, width, height);
            view.key(Key::PageDown);
            let shown = frame::painted(&mut view, width, height);
            let row = (0..height).find(|y| shown.row(*y).contains("界")).unwrap();
            view.drag(Cell::new(row, 0), Cell::new(row, width - 1));
            frame::painted(&mut view, width, height);
            assert_eq!(view.key(Key::Copy), Outcome::Unchanged);
            assert!(!(0..height).any(|y| shown.row(y).contains("kept back")));
            if view.capabilities().palette() == Palette::SLOTS {
                assert!(shown.told().placements().is_empty());
            }
        }
    }
}

#[test]
fn generic_invocation_copy_preserves_recorded_json_arguments() {
    use sindri::composer::{Key, Outcome};
    use sindri::mark::Cell;
    let original = "{\"nested\": [\"  evidence  \", {\"flag\": true}]}";
    let mut invocation = call();
    if let Entry::Call {
        name, arguments, ..
    } = &mut invocation
    {
        *name = SecretSet::default().redact("generic");
        *arguments = SecretSet::default().redact(original);
    }
    let mut view = View::of(
        caps::pictured(),
        Facts::default().asked().running(),
        Transcript::of(vec![invocation]),
    );
    let shown = frame::painted(&mut view, 80, 24);
    let row = (0..24)
        .find(|y| shown.row(*y).contains("evidence"))
        .unwrap();
    view.drag(Cell::new(row, 2), Cell::new(row, 60));
    frame::painted(&mut view, 80, 24);
    assert_eq!(view.key(Key::Copy), Outcome::Unchanged);
}

#[test]
fn closed_result_fold_hides_byte_detail_until_opened() {
    use sindri::composer::{Key, Outcome};
    use sindri::mark::Cell;
    let output = (0..12)
        .map(|n| format!("evidence {n}\n"))
        .collect::<String>();
    let mut view = View::of(
        caps::pictured(),
        Facts::default().asked().running(),
        Transcript::of(vec![call(), result(Some(Exit::Code(0)), &output)]),
    );
    let shown = frame::painted(&mut view, 80, 24);
    assert!(!(0..24).any(|y| shown.row(y).contains("bytes")));
    let row = (0..24)
        .find(|y| shown.row(*y).contains('\u{22ef}'))
        .unwrap();
    view.drag(Cell::new(row, 2), Cell::new(row, 60));
    frame::painted(&mut view, 80, 24);
    assert_eq!(view.key(Key::Fold), Outcome::Edited);
    let shown = frame::painted(&mut view, 80, 40);
    assert!((0..40).any(|y| shown.row(y).contains("987 bytes")));
}

fn work(n: u32, command: &str, output: &str) -> Vec<Entry> {
    let secrets = SecretSet::default();
    let id = Some(secrets.redact(format!("work-{n}")));
    vec![
        Entry::Call {
            turn: n,
            id: id.clone(),
            name: secrets.redact("bash"),
            arguments: secrets.redact(serde_json::json!({"command": command}).to_string()),
        },
        Entry::Result {
            turn: n,
            id,
            name: secrets.redact("bash"),
            text: secrets.redact(output),
            bytes: 42,
            elapsed_ms: 1_200,
            exit: Some(Exit::Code(0)),
        },
    ]
}

#[test]
fn a_call_lays_its_command_and_output_behind_one_rule_at_the_gutters_own_column() {
    use sindri::palette::Band;
    let long = "a line of output wider than any band the frame sets ".repeat(4);
    for capabilities in [
        caps::inked(Palette::TRUECOLOR),
        caps::inked(Palette::ANSI),
        caps::inked(Palette::SLOTS),
    ] {
        for (width, height) in [(120, 40), (80, 24), (40, 24)] {
            let mut entries = work(1, "printf hello", "the one line it printed");
            entries.extend(work(2, "rg -n 'rule' crates/sindri/src", &long));
            let mut view = View::of(
                capabilities.clone(),
                Facts::default().asked().running(),
                Transcript::of(entries),
            );
            view.key(sindri::composer::Key::Fold);
            let shown = frame::painted(&mut view, width, height);
            let palette = capabilities.palette();
            let band = shown
                .told()
                .layout()
                .expect("a painted frame lays the bands it draws")
                .transcript;
            let rule = band.x + 2;
            let head = (0..height)
                .find(|y| shown.row(*y).contains("printf hello"))
                .unwrap();
            view.drag(
                sindri::mark::Cell::new(head, 2),
                sindri::mark::Cell::new(head, 25),
            );
            frame::painted(&mut view, width, height);
            view.key(sindri::composer::Key::Fold);
            let shown = frame::painted(&mut view, width, height);
            let (command_row, command) = shown.drawn_at("printf hello");
            assert_eq!(command, rule + 2, "{}", shown.row(command_row));
            let (output_row, output) = shown.drawn_at("the one line it printed");
            assert_eq!(output, rule + 2, "{}", shown.row(output_row));
            let (wide_row, wide) = shown.drawn_at("a line of output wider");
            assert_eq!(wide, rule + 2, "{}", shown.row(wide_row));
            assert_eq!(
                shown.glyph(band.right() - 1, wide_row),
                '…',
                "a row the band cuts keeps the transcript's cut mark: {}",
                shown.row(wide_row)
            );
            let exact = "z".repeat(usize::from(band.width).saturating_sub(5));
            let mut full = work(1, "printf hello", "the one line it printed");
            full.extend(work(2, "rg -n 'rule' crates/sindri/src", &exact));
            let mut whole = View::of(
                capabilities.clone(),
                Facts::default().asked().running(),
                Transcript::of(full),
            );
            whole.key(sindri::composer::Key::Fold);
            let shown = frame::painted(&mut whole, width, height);
            let (exact_row, exact_column) = shown.drawn_at(&exact);
            assert_eq!(exact_column, rule + 2, "{}", shown.row(exact_row));
            assert!(
                !shown.row(exact_row).contains('…'),
                "a row of output holds every column the band has: {}",
                shown.row(exact_row)
            );
            assert_eq!(shown.glyph(band.right() - 2, exact_row), 'z');
            let head = (band.y..band.bottom())
                .find(|y| shown.row(*y).contains("✓"))
                .expect("the call's first row");
            let detail = (band.y..band.bottom())
                .find(|y| shown.row(*y).contains("bytes"))
                .expect("the call's last row");
            for y in head..=detail {
                let row = shown.row(y);
                assert_eq!(
                    row.matches('│').count(),
                    1,
                    "a call draws one rule and no second edge: row {y} {row:?}"
                );
                assert_eq!(shown.glyph(rule, y), '│', "row {y} {row:?}");
                for x in band.x..band.right() {
                    assert_eq!(
                        shown.cell(x, y).bg,
                        palette.band(Band::Well),
                        "row {y}, column {x} of a call claims no ground of its own: {row:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn two_calls_of_different_lengths_draw_their_rule_at_one_column() {
    for (width, height) in [(120, 40), (80, 24)] {
        let mut entries = vec![Entry::Prompt {
            text: SecretSet::default().redact("two calls, one margin"),
        }];
        entries.push(Entry::Assistant {
            turn: 1,
            text: SecretSet::default().redact("The first call prints and the second reads."),
        });
        entries.extend(work(1, "printf hello", "one"));
        entries.extend(work(
            2,
            "rg -n 'rule' src",
            "a line of output wider than the other",
        ));
        entries.push(Entry::Assistant {
            turn: 2,
            text: SecretSet::default().redact("Both stand behind one rule."),
        });
        let mut view = View::of(
            caps::inked(Palette::TRUECOLOR),
            Facts::default().asked(),
            Transcript::of(entries),
        );
        let shown = frame::painted(&mut view, width, height);
        let band = shown
            .told()
            .layout()
            .expect("a painted frame lays the bands it draws")
            .transcript;
        let rules: Vec<u16> = (band.y..band.bottom())
            .filter(|y| shown.row(*y).contains('│'))
            .map(|y| {
                let row = shown.row(y);
                assert_eq!(row.matches('│').count(), 1, "row {y} {row:?}");
                shown
                    .column_of(y, '│')
                    .expect("the column the rule is drawn at")
            })
            .collect();
        assert_eq!(
            rules.len(),
            2,
            "each closed call draws its rule on its own row and no row beside it: {rules:?}"
        );
        assert!(
            rules.iter().all(|column| *column == band.x + 2),
            "two calls never draw their rule at different columns: {rules:?}"
        );
    }
}

#[test]
fn settled_call_keeps_only_its_log_row_until_opened() {
    use sindri::composer::{Key, Outcome};
    let mut view = View::of(
        caps::inked(Palette::SLOTS),
        Facts::default().asked().running(),
        Transcript::of(vec![call(), result(Some(Exit::Code(0)), "hello")]),
    );
    let shown = frame::painted(&mut view, 80, 24);
    let words = shown.text("tools");
    assert!(words.contains("printf hello"), "{words}");
    assert!(words.contains("\u{22ef}"), "{words}");
    assert!(!words.contains("rows kept back"), "{words}");
    assert!(!words.contains("success"), "{words}");
    assert!(!words.contains("987 bytes"), "{words}");
    assert_eq!(view.key(Key::Fold), Outcome::Edited);
    let opened = frame::painted(&mut view, 80, 24).text("tools");
    assert!(opened.contains("987 bytes"), "{opened}");
}

#[test]
fn failure_keeps_the_last_recorded_nonempty_diagnostic_and_no_byte_detail() {
    for output in ["first\n  final diagnostic\n\n", ""] {
        let mut view = View::of(
            caps::inked(Palette::SLOTS),
            Facts::default().asked().running(),
            Transcript::of(vec![call(), result(Some(Exit::Code(7)), output)]),
        );
        let shown = frame::painted(&mut view, 80, 24);
        let words = shown.text("tools");
        assert!(words.contains("exit 7"), "{words}");
        assert!(!words.contains("first"), "{words}");
        assert!(!words.contains("987 bytes"), "{words}");
        assert_eq!(words.contains("final diagnostic"), !output.is_empty());
    }
}

#[test]
fn closed_head_prioritizes_the_command_and_kept_count_at_six_measures() {
    for width in [120, 100, 80, 60, 50, 40] {
        let mut entries = work(
            1,
            "printf a-very-long-recognisable-command-tail",
            "one\ntwo",
        );
        let mut view = View::of(
            caps::inked(Palette::SLOTS),
            Facts::default().asked().running(),
            Transcript::of(std::mem::take(&mut entries)),
        );
        let shown = frame::painted(&mut view, width, 24);
        let row = (0..24)
            .map(|y| shown.row(y))
            .find(|row| row.contains('\u{22ef}'))
            .unwrap();
        assert!(row.contains("printf"), "{row}");
        assert!(row.contains("✓"), "{row}");
        if width <= 60 {
            assert!(!row.contains("bash"), "{row}");
            assert!(!row.contains("1.2 s"), "{row}");
        }
        if width <= 50 {
            assert!(row.contains('…'), "{row}");
        }
    }
}

#[test]
fn settled_exchange_counts_recorded_calls_failures_and_time_without_reflowing_its_tally() {
    for width in [24, 28, 40, 80, 120] {
        for failed in [false, true] {
            let mut entries = vec![Entry::Prompt {
                text: SecretSet::default().redact("inspect"),
            }];
            entries.extend(work(1, "printf hello", "one\ntwo\nthree"));
            let mut second = work(2, "printf goodbye", "diagnostic");
            if failed && let Entry::Result { exit, .. } = &mut second[1] {
                *exit = Some(Exit::Code(7));
            }
            entries.extend(second);
            entries.push(Entry::Stop {
                stop: sindri_core::run::Stop::Final,
            });
            let mut view = View::of(
                caps::uncoloured(),
                Facts::default().asked(),
                Transcript::of(entries),
            );
            let shown = frame::painted(&mut view, width, 24);
            let rows: Vec<_> = (0..20).map(|y| shown.row(y)).collect();
            let tally = rows.iter().find(|row| row.contains("2 calls")).unwrap();
            assert_eq!(rows.iter().filter(|row| row.contains("calls")).count(), 1);
            assert_eq!(tally.contains("1 failed"), failed, "{tally}");
            assert!(!tally.contains("0 failed"), "{tally}");
            if width >= 40 {
                assert!(tally.contains("2.4 s"), "{tally}");
            } else if failed {
                assert!(!tally.contains("2.4 s"), "{tally}");
            }
        }
    }
}

#[test]
fn closed_heads_keep_numeric_disclosure_and_fit_even_when_fixed_facts_do_not() {
    use sindri::picture::Surface;
    use sindri::rows::{Measure, Rows};
    for name in ["bash", "a_tool_name_that_cannot_fit"] {
        for command in ["abcdef", "界界界界"] {
            for width in [4, 8, 12, 20, 24, 40] {
                let mut rows = Rows::new();
                rows.measure(Measure::new(width, Surface::none(), None));
                rows.push(Entry::Call {
                    turn: 1,
                    id: Some(SecretSet::default().redact("work")),
                    name: SecretSet::default().redact(name),
                    arguments: SecretSet::default().redact(format!(r#"{{"command":"{command}"}}"#)),
                });
                rows.push(result(Some(Exit::Code(0)), "hello"));
                let head = &rows.rows(0)[0];
                assert!(head.width() <= width, "{width}: {head:?}");
                let closed = head.glyphs();
                let count = closed.rsplit(' ').next().unwrap_or_default();
                let digits: String = count.chars().filter(char::is_ascii_digit).collect();
                assert!(
                    !digits.is_empty() && count.ends_with(&digits),
                    "{width}: {closed}"
                );
                if name == "bash" && width == 40 {
                    assert!(closed.ends_with("⋯3"), "{closed}");
                }
            }
        }
    }
}
