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
        let shown = frame::painted(&mut view, 80, 24);
        let words = shown.text("tools");
        assert_eq!(words.matches('✓').count(), 1, "{words}");
        assert_eq!(words.matches("success").count(), 1, "{words}");
        let head = (0..24)
            .map(|y| shown.row(y))
            .find(|row| row.contains("bash"))
            .unwrap();
        assert!(!head.contains("bytes"), "{head}");
        assert!(words.contains("987 bytes"), "{words}");
    }
}

#[test]
fn live_failure_absent_exit_and_unmatched_work_keep_required_words_at_narrow_widths() {
    for width in [120, 80, 40, 12] {
        for (entries, expected) in [
            (vec![call()], "running"),
            (
                vec![call(), result(Some(Exit::Code(7)), "denied")],
                "failure",
            ),
            (vec![call(), result(None, "")], "completed"),
            (vec![result(None, "orphan")], "completed"),
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
            assert!(words.contains("bash"), "{words}");
            assert!(words.contains(expected), "{words}");
            if expected == "failure" {
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
            let shown = frame::painted(&mut view, width, height);
            let head = (0..height)
                .find(|y| shown.row(*y).contains("bash"))
                .unwrap();
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
            let shown = frame::painted(&mut view, width, height);
            let row = (0..height).find(|y| shown.row(*y).contains("one")).unwrap();
            view.drag(Cell::new(row, 2), Cell::new(row, width - 3));
            frame::painted(&mut view, width, height);
            assert_eq!(view.key(Key::Fold), Outcome::Edited);
            let shown = frame::painted(&mut view, width, height);
            let row = (0..height).find(|y| shown.row(*y).contains("one")).unwrap();
            view.drag(Cell::new(row, 2), Cell::new(row, width - 3));
            frame::painted(&mut view, width, height);
            assert_eq!(view.key(Key::Copy), Outcome::Unchanged);
            let shown = frame::painted(&mut view, 80, 24);
            assert!((0..24).any(|y| shown.row(y).contains("ten")));
            assert!(!(0..24).any(|y| shown.row(y).contains("kept back")));
        }
    }
}

#[test]
fn compact_short_success_keeps_all_evidence_and_reader_controls_in_every_capability_and_size() {
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
            let shown = frame::painted(&mut view, width, height);
            let words = (0..height)
                .map(|y| shown.row(y).trim().to_owned())
                .collect::<String>();
            assert!(words.contains("bash"));
            assert!(words.contains("success"));
            if width > 12 {
                assert!(words.contains("987 bytes"));
            }
            assert!(!words.contains("kept back"));
            let row = (0..height).find(|y| shown.row(*y).contains("界")).unwrap();
            view.drag(Cell::new(row, 0), Cell::new(row, width - 1));
            frame::painted(&mut view, width, height);
            assert_eq!(view.key(Key::Copy), Outcome::Unchanged);
            assert_eq!(view.key(Key::Fold), Outcome::Edited);
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
fn reader_opened_compact_result_keeps_source_through_exchange_scroll_resize_and_capability_changes()
{
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
    let shown = frame::painted(&mut view, 120, 40);
    let row = (0..40).find(|y| shown.row(*y).contains("界")).unwrap();
    view.drag(Cell::new(row, 0), Cell::new(row, 119));
    frame::painted(&mut view, 120, 40);
    assert_eq!(view.key(Key::Fold), Outcome::Edited);
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
        .find(|y| shown.row(*y).contains("rows kept back"))
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
            let shown = frame::painted(&mut view, width, height);
            let palette = capabilities.palette();
            let band = shown
                .told()
                .layout()
                .expect("a painted frame lays the bands it draws")
                .transcript;
            let rule = band.x + 2;
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
                .find(|y| shown.row(*y).contains("bash"))
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
            8,
            "each call draws its rule on its own four rows and on no row beside it: {rules:?}"
        );
        assert!(
            rules.iter().all(|column| *column == band.x + 2),
            "two calls never draw their rule at different columns: {rules:?}"
        );
    }
}
