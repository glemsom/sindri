//! The extra headers row's fold: the table the row holds, drawn as one
//! `Name: value` line per header with `+ add a header` under the last of them,
//! and the keys that make it a table a keyboard can build — `a` adds a line, `d`
//! removes the header under the cursor, `Ctrl-U` empties it, `Enter` hands that
//! line its own keys, `Esc` shuts the fold and keeps the draft. The save is the
//! path the row already had: the fold is a way into a value, never a second one.
//!
//! The line under the cursor is the field's own cursor, so the mark the fold
//! draws and the cell the terminal's cursor stands on are one place, and a line
//! a reader is in is a line a check can be run on — the sentence one line earns
//! is a sentence about `Field::line_text` at `Field::line`. What the fold holds
//! is whose the keys are. `Choosing` is the fold's: a list a reader walks, adds
//! to and removes from. `Writing` is the line's, because a header's value may
//! hold the `a` and the `d` the fold takes, and a fold that kept the keys would
//! eat two letters out of every value. `Enter` is the rung between them, which
//! is the rung `Enter` already climbs on the model and provider rows.
//!
//! What the fold draws is the file's text, unresolved: a `{session}` in a value
//! is shown as written and never run, because a screen is not where a value
//! becomes a request.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use sindri_core::config::Key;

use super::{Outcome, Settings};

/// Where the headers row's fold stands: shut, or open with the keys either the
/// fold's or the line's under the cursor. Two variants and not a pair of flags,
/// for the reason `Edit` is one field: a fold writing while it is shut is a
/// state nothing can build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fold {
    /// The row is the field it has always been: every key is the value's.
    Shut,
    /// Open, with the keys the fold's: `↑↓` walk the lines, `a` and `d` add and
    /// remove one, `Ctrl-U` empties one, and `Enter` hands one its own keys.
    Choosing,
    /// Open, with the keys the line's under the cursor: it is being typed, so
    /// `a` and `d` are two characters of it like any other.
    Writing,
}

impl Fold {
    /// Whether the table stands open, either way.
    pub fn folded(self) -> bool {
        self != Fold::Shut
    }

    /// Whether the keys are the line's rather than the fold's: what the hint
    /// line says and what the two letters read before they are taken.
    pub fn writing(self) -> bool {
        self == Fold::Writing
    }
}

/// How many headers a table holds, in the row's own words: a line something
/// stands on is a header, and a blank line is the place one is typed — which is
/// why a save drops it and this does not count it.
pub(crate) fn counted(text: &str) -> String {
    let held = text
        .split('\n')
        .filter(|line| !line.trim().is_empty())
        .count();
    match held {
        0 => "no headers".to_string(),
        1 => "1 header".to_string(),
        held => format!("{held} headers"),
    }
}

/// A key while the table stands open, and what it did. `None` is a key the fold
/// leaves to the modal: a `Tab` or a page walks the rows and shuts the fold on
/// the way out, `Ctrl-Q` and `Ctrl-C` are the frame's, and while a line is being
/// written every key that is the line's own is the field's.
pub(super) fn key(settings: &mut Settings, key: KeyEvent) -> Option<Outcome> {
    if !settings.headers.folded() || settings.chosen_key() != Some(Key::ExtraHeaders) {
        return None;
    }
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let choosing = !settings.headers.writing();
    match key.code {
        KeyCode::Esc => Some(shut(settings)),
        KeyCode::Up => Some(step(settings, true)),
        KeyCode::Down => Some(step(settings, false)),
        KeyCode::Enter => Some(hand(settings)),
        KeyCode::Char('u') if ctrl => Some(empty(settings)),
        // The fold's two letters, and only while the keys are the fold's: a line
        // being written holds both of them, so whose the keys are is said before
        // the fold can eat one.
        KeyCode::Char('a') if !ctrl && choosing => Some(add(settings)),
        KeyCode::Char('d') if !ctrl && choosing => Some(remove(settings)),
        KeyCode::Tab | KeyCode::BackTab => None,
        _ if ctrl || !choosing => None,
        // A line the fold has not handed over does nothing with a keystroke:
        // `Enter` is the rung that says the line is a reader's to type.
        _ => Some(Outcome::Held),
    }
}

/// `Esc`: the fold shut with the draft beside it exactly as it was — the
/// discarding `Esc` is the modal's, and it is the one after this.
fn shut(settings: &mut Settings) -> Outcome {
    settings.headers = Fold::Shut;
    Outcome::Staying
}

/// `↑` or `↓`: the fold's cursor a line either way, and the line it lands on
/// taken from its end. The fold takes the key, so a line being written gives it
/// back with the arrow rather than keeping the keys where the cursor left them.
fn step(settings: &mut Settings, up: bool) -> Outcome {
    settings.headers = Fold::Choosing;
    let Some(field) = settings.table() else {
        return Outcome::Staying;
    };
    let last = field.line_count() - 1;
    let at = match up {
        true => field.line().saturating_sub(1),
        false => (field.line() + 1).min(last),
    };
    field.onto(at);
    Outcome::Staying
}

/// `Enter`: the line under the cursor handed its own keys, and the `Enter` after
/// giving them back to the fold. A table holding nothing opens on its one blank
/// line, so the first header is an `Enter` and a value rather than a paste.
fn hand(settings: &mut Settings) -> Outcome {
    settings.headers = match settings.headers.writing() {
        true => Fold::Choosing,
        false => Fold::Writing,
    };
    Outcome::Staying
}

/// `a`: a blank line at the end of the table with the keys handed to it — the
/// offer the fold draws under the last header, taken.
fn add(settings: &mut Settings) -> Outcome {
    settings.table_edit(|field| {
        field.add_line();
    });
    settings.headers = Fold::Writing;
    Outcome::Staying
}

/// `d`: the header under the cursor removed, the lines either side of it left
/// whole, and the fold's cursor on the line that took its place.
fn remove(settings: &mut Settings) -> Outcome {
    let Some(at) = settings.table_line() else {
        return Outcome::Staying;
    };
    settings.table_edit(|field| field.remove_line(at));
    Outcome::Staying
}

/// `Ctrl-U` in the fold: the one line under the cursor emptied, its break kept,
/// and the keys handed to it, because an emptied line is the one about to be
/// typed. The shut row's `Ctrl-U` is still the key's own — it leaves the file
/// rather than emptying a table the row shows.
fn empty(settings: &mut Settings) -> Outcome {
    let Some(at) = settings.table_line() else {
        return Outcome::Staying;
    };
    settings.table_edit(|field| field.empty_line(at));
    settings.headers = Fold::Writing;
    Outcome::Staying
}
