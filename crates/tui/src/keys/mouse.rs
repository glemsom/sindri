//! The pointer: a press, a drag, a release and the wheel. A gesture is never
//! the only way in — every one of these has a key that does the same thing —
//! and `Shift` hands the gesture back to the terminal, so the frame never takes
//! a drag that is the terminal's own selection.

use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Position;

use super::WHEEL_ROWS;
use crate::app::App;
use crate::selection::Selection;

impl App {
    /// One mouse event. `Shift` hands the gesture back to the terminal: the
    /// frame never sees a drag it must not take.
    pub fn mouse(&mut self, event: MouseEvent) {
        if event.modifiers.contains(KeyModifiers::SHIFT) {
            return;
        }
        if self.overlay.is_some() {
            match event.kind {
                MouseEventKind::ScrollUp => self.scroll_page(true),
                MouseEventKind::ScrollDown => self.scroll_page(false),
                _ => {}
            }
            return;
        }
        let at = Position::new(event.column, event.row);
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => self.press(at),
            // A right or middle button is a click too, and a click is the
            // reader's next gesture: it spends the mark and takes nothing.
            MouseEventKind::Down(_) => self.clear_mark(),
            MouseEventKind::Drag(MouseButton::Left) => {
                if self.selection.dragging {
                    self.selection.head = at;
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                // The button coming up copies what the mark covers and leaves
                // the mark standing: the note names rows the reader can still
                // see, and `Ctrl-Y` copies them again.
                self.selection.finish();
                self.copy_selection();
            }
            MouseEventKind::ScrollUp => {
                self.clear_mark();
                self.scroll_by(WHEEL_ROWS, true);
            }
            MouseEventKind::ScrollDown => {
                self.clear_mark();
                self.scroll_by(WHEEL_ROWS, false);
            }
            _ => {}
        }
    }

    /// A press: what a row is worth before it is a place in the text. A click
    /// of any kind is also the reader's next gesture, so the mark goes first.
    fn press(&mut self, at: Position) {
        self.clear_mark();
        // A row of the rail is a row like any other: the session id can be
        // taken out of the frame and pasted into `sindri sessions show`.
        // The whole fact is copied, so a wrapped id is never half
        // an id.
        if let Some(row) = self.rail_view.row_at(at) {
            let text = if self.rail_view.session.contains(&row) {
                self.rail.session()
            } else {
                self.rail_view.rows.get(row).cloned().unwrap_or_default()
            };
            if !text.trim().is_empty() {
                self.copy(&text);
            }
            return;
        }
        let offered = self
            .viewport
            .row_of(at)
            .and_then(|row| self.viewport.spilled(row))
            .cloned();
        if let Some(path) = offered {
            self.show_spill(&path);
            return;
        }
        // A click on an expandable entry's own row opens that entry, and is
        // never a selection: the row a reader points at is the one they mean.
        if let Some(index) = self
            .viewport
            .row_of(at)
            .and_then(|row| self.viewport.expandable(row))
        {
            self.chosen = Some(index);
            self.toggle(index);
            return;
        }
        if self.viewport.holds(at) {
            self.selection = Selection::start(at);
        }
    }
}
