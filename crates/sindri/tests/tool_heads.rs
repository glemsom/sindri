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
    let rows = (0..20).map(|y| shown.row(y)).collect::<Vec<_>>();
    let first = rows.iter().position(|row| row.contains("界")).unwrap();
    assert!(rows[first].contains("  x"));
    assert!(rows[first + 1].trim().is_empty());
    assert!(rows[first + 2].contains("      y"));
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
    assert_eq!(view.key(Key::Copy), Outcome::Copy(command.to_owned()));
    let shown = frame::painted(&mut view, 80, 24);
    let row = (0..20)
        .find(|y| shown.row(*y).contains("界") && !shown.row(*y).contains("printf"))
        .unwrap();
    view.drag(Cell::new(row, 2), Cell::new(row, 25));
    frame::painted(&mut view, 80, 24);
    assert_eq!(view.key(Key::Copy), Outcome::Copy(output.to_owned()));
}
