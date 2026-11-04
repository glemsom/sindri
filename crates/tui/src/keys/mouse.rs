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
    /// One mouse event, and whether the pointer said anything the frame draws:
    /// the answer is what the gesture did, and a gesture that did nothing says
    /// nothing. `Shift` hands the report back to the terminal: the frame never
    /// sees a drag it must not take, so nothing of its own moves for one.
    ///
    /// Motion with no button held is the report that made this worth saying. The
    /// frame asks the terminal for any-event tracking, so the pointer moving
    /// over a still frame arrives as one report per sample — and it has no arm
    /// of its own below, because no part of the frame reads it. What a frame
    /// drawn for it would cost is the cursor's blink: every draw ends by showing
    /// the terminal's cursor and moving it back into the composer, and a
    /// terminal restarts a blink on both. So a moving pointer leaves the
    /// composer's cursor blinking at the pointer's sample rate, and a still one
    /// leaves it alone.
    pub fn mouse(&mut self, event: MouseEvent) -> bool {
        if event.modifiers.contains(KeyModifiers::SHIFT) {
            return false;
        }
        if self.overlay.is_some() {
            return match event.kind {
                MouseEventKind::ScrollUp => {
                    self.scroll_page(true);
                    true
                }
                MouseEventKind::ScrollDown => {
                    self.scroll_page(false);
                    true
                }
                // An overlay owns the screen: a press or a drag under it is no
                // part of the frame's.
                _ => false,
            };
        }
        let at = Position::new(event.column, event.row);
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => self.press(at),
            // A right or middle button is a click too, and a click is the
            // reader's next gesture: it spends the mark and takes nothing. With
            // no mark standing there is nothing to spend.
            MouseEventKind::Down(_) => self.clear_mark(),
            // A drag is the mark the press began growing: with no press behind
            // it there is no mark to grow.
            MouseEventKind::Drag(MouseButton::Left) if self.selection.dragging => {
                self.selection.head = at;
                true
            }
            // The button coming up copies what the mark covers and leaves the
            // mark standing: the note names rows the reader can still see, and
            // `Ctrl-Y` copies them again. A button coming up over nothing took
            // nothing.
            MouseEventKind::Up(MouseButton::Left) => self.lift(),
            MouseEventKind::ScrollUp => {
                self.clear_mark();
                self.scroll_by(WHEEL_ROWS, true);
                true
            }
            MouseEventKind::ScrollDown => {
                self.clear_mark();
                self.scroll_by(WHEEL_ROWS, false);
                true
            }
            // Motion with no button held, and every report the frame has no use
            // for: no arm of its own, so no row changed and nothing is drawn.
            _ => false,
        }
    }

    /// A release: the mark stops following the pointer and is copied from. What
    /// a release over no mark is worth is nothing at all — a press nobody
    /// dragged took no row.
    fn lift(&mut self) -> bool {
        let marked = !self.selection.is_empty();
        self.selection.finish();
        if !marked {
            return false;
        }
        self.copy_selection();
        true
    }

    /// A press: what a row is worth before it is a place in the text. A click
    /// of any kind is also the reader's next gesture, so the mark goes first.
    fn press(&mut self, at: Position) -> bool {
        let spent = self.clear_mark();
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
            let copied = !text.trim().is_empty();
            if copied {
                self.copy(&text);
            }
            return spent || copied;
        }
        let offered = self
            .viewport
            .row_of(at)
            .and_then(|row| self.viewport.spilled(row))
            .cloned();
        if let Some(path) = offered {
            // A click on the affordance asks for the file the way `Ctrl-O` does:
            // the box opens where the words arrive, and the click itself puts no
            // row on screen — only the mark going is a row of the frame changing.
            self.ask_spill(path);
            return spent;
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
            return true;
        }
        if self.viewport.holds(at) {
            self.selection = Selection::start(at);
            return true;
        }
        spent
    }
}
