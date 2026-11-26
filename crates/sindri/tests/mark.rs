mod caps;
mod frame;

use sindri::composer::{Key, Outcome};
use sindri::facts::Facts;
use sindri::ink::Ink;
use sindri::mark::Cell;
use sindri::pointer::{Notch, Pointer};
use sindri::view::{Transcript, View};
use sindri_core::provider::Deltas;
use sindri_core::records::Entry;
use sindri_core::redact::SecretSet;

fn view(text: &str) -> View {
    View::of(
        caps::pointing(),
        Facts::default().asked(),
        Transcript::of([Entry::Assistant {
            turn: 1,
            text: SecretSet::default().redact(text),
        }]),
    )
}

#[test]
fn mouse_drag_highlights_partial_text_and_copies_once_on_release() {
    let mut view = view("abcdefghij");
    let shown = frame::painted(&mut view, 80, 24);
    let y = (0..20)
        .find(|y| shown.row(*y).contains("abcdefghij"))
        .unwrap();
    let x = shown.column_of(y, 'a').unwrap();
    let start = Cell::new(y, x + 2);
    let end = Cell::new(y, x + 5);
    assert_eq!(
        view.pointer(shown.told(), Pointer::Press(start)),
        Outcome::Edited
    );
    assert_eq!(
        view.pointer(shown.told(), Pointer::Drag(end)),
        Outcome::Edited
    );
    let marked = frame::painted(&mut view, 80, 24);
    for column in x + 2..=x + 5 {
        assert_eq!(marked.ink(column, y), Some(Ink::Mark));
    }
    assert_eq!(
        view.pointer(marked.told(), Pointer::Release(end)),
        Outcome::Copy("cdef".into())
    );
    assert_eq!(
        view.pointer(marked.told(), Pointer::Release(end)),
        Outcome::Unchanged
    );
}

#[test]
fn plain_click_and_keyboard_shortcuts_never_copy() {
    let mut view = view("last answer");
    let shown = frame::painted(&mut view, 80, 24);
    let at = Cell::new(0, 4);
    view.pointer(shown.told(), Pointer::Press(at));
    assert!(!matches!(
        view.pointer(shown.told(), Pointer::Release(at)),
        Outcome::Copy(_)
    ));
    for key in [
        Key::Copy,
        Key::ShiftUp,
        Key::ShiftDown,
        Key::ShiftLeft,
        Key::ShiftRight,
    ] {
        assert_eq!(view.key(key), Outcome::Unchanged);
    }
}

#[test]
fn release_without_motion_reports_still_selects_and_reverse_drag_matches() {
    for reverse in [false, true] {
        let mut view = view("abcdefghij");
        let shown = frame::painted(&mut view, 80, 24);
        let y = (0..20)
            .find(|y| shown.row(*y).contains("abcdefghij"))
            .unwrap();
        let x = shown.column_of(y, 'a').unwrap();
        let (start, end) = if reverse {
            (Cell::new(y, x + 5), Cell::new(y, x + 2))
        } else {
            (Cell::new(y, x + 2), Cell::new(y, x + 5))
        };
        view.pointer(shown.told(), Pointer::Press(start));
        assert_eq!(
            view.pointer(shown.told(), Pointer::Release(end)),
            Outcome::Copy("cdef".into())
        );
    }
}

#[test]
fn resize_scroll_and_stream_cancel_pending_selection() {
    for change in 0..3 {
        let mut view = view("abcdefghij");
        let shown = frame::painted(&mut view, 80, 24);
        view.pointer(shown.told(), Pointer::Press(Cell::new(0, 2)));
        view.pointer(shown.told(), Pointer::Drag(Cell::new(0, 5)));
        match change {
            0 => {
                frame::painted(&mut view, 70, 24);
            }
            1 => {
                view.pointer(shown.told(), Pointer::Wheel(Notch::Up));
            }
            _ => view.text("changed"),
        }
        assert_eq!(
            view.pointer(shown.told(), Pointer::Release(Cell::new(0, 8))),
            Outcome::Unchanged
        );
        assert!(view.mark().is_none());
    }
}

#[test]
fn release_outside_band_is_clamped_and_cannot_copy_hidden_fence_source() {
    let mut view = view(&format!("```text\n  {}\n\n  end\n```", "x".repeat(200)));
    let shown = frame::painted(&mut view, 80, 24);
    let y = (0..20).find(|y| shown.row(*y).contains("xxxx")).unwrap();
    let x = shown.column_of(y, 'x').unwrap();
    view.pointer(shown.told(), Pointer::Press(Cell::new(y, x)));
    let Outcome::Copy(text) = view.pointer(shown.told(), Pointer::Release(Cell::new(y, u16::MAX)))
    else {
        panic!("nonempty selection")
    };
    assert!(text.len() < 80);
    assert!(!text.contains("end"));
}

#[test]
fn multiline_fence_selection_preserves_indentation_and_blank_lines() {
    let mut view = view("```text\n  first\n\n    last\n```");
    let shown = frame::painted(&mut view, 80, 24);
    let first = (0..20).find(|y| shown.row(*y).contains("first")).unwrap();
    let last = (0..20).find(|y| shown.row(*y).contains("last")).unwrap();
    view.pointer(shown.told(), Pointer::Press(Cell::new(first, 0)));
    assert_eq!(
        view.pointer(shown.told(), Pointer::Release(Cell::new(last, 70))),
        Outcome::Copy("  first\n\n    last".into())
    );
}

#[test]
fn dragging_a_folded_prompt_copies_only_visible_rows_and_does_not_open_it() {
    let text = (1..=20)
        .map(|n| format!("prompt line {n}"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut view = View::of(
        caps::pointing(),
        Facts::default().asked(),
        Transcript::of([Entry::Prompt {
            text: SecretSet::default().redact(&text),
        }]),
    );
    let shown = frame::painted(&mut view, 80, 24);
    let first = (0..20)
        .find(|y| shown.row(*y).contains("prompt line 1"))
        .unwrap();
    let count = (0..20)
        .find(|y| shown.row(*y).contains("kept back"))
        .unwrap();
    view.pointer(shown.told(), Pointer::Press(Cell::new(first, 3)));
    view.pointer(shown.told(), Pointer::Drag(Cell::new(count, 70)));
    let Outcome::Copy(text) = view.pointer(shown.told(), Pointer::Release(Cell::new(count, 70)))
    else {
        panic!("visible prompt")
    };
    assert!(!text.contains("prompt line 20"));
    assert!(
        frame::painted(&mut view, 80, 24)
            .row(count)
            .contains("kept back")
    );
}
