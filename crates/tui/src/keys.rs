//! The hands: what one key, or one gesture, does to the frame (ADR-0011).
//! Everything the mouse can do is here as a key as well, because a pointer is
//! an accelerator and never the only way in.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Position;

use crate::app::{App, Renew};
use crate::command::{self, Command};
use crate::completion::{self, Completion};
use crate::entry::Entry;
use crate::overlay::{Help, Overlay};
use crate::selection::Selection;
use crate::spill::Spill;
use crate::window::Ask;

const WHEEL_ROWS: usize = 3;

impl App {
    pub fn key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if self.overlay.is_some() {
            self.overlay_key(key, ctrl);
            return;
        }
        match key.code {
            KeyCode::Char('q') if ctrl => self.quit = true,
            KeyCode::Char('c') if ctrl => self.cancel_or_clear(),
            KeyCode::Char('y') if ctrl => self.copy_last_answer(),
            KeyCode::Char('o') if ctrl => self.open_spill(),
            KeyCode::Char('x') if ctrl => {
                self.drop_queued();
            }
            KeyCode::Char('g') if ctrl => self.copy_session_id(),
            KeyCode::Char('?') if self.composer.is_blank() => {
                self.show(Overlay::Help(Help::default()))
            }
            KeyCode::Esc => self.dismiss(),
            KeyCode::Tab => self.accept_completion(),
            KeyCode::PageUp => self.scroll_page(true),
            KeyCode::PageDown => self.scroll_page(false),
            KeyCode::Home => self.window.to_top(),
            KeyCode::End => self.window.to_tail(),
            KeyCode::Enter if key.modifiers.contains(KeyModifiers::SHIFT) => {
                self.composer.newline();
                self.refresh_completion();
            }
            KeyCode::Enter => self.enter(),
            KeyCode::Backspace => {
                self.composer.backspace();
                self.refresh_completion();
            }
            KeyCode::Left => self.composer.left(),
            KeyCode::Right => self.composer.right(),
            KeyCode::Up if key.modifiers.contains(KeyModifiers::SHIFT) => {
                self.select_by_keyboard(true)
            }
            KeyCode::Down if key.modifiers.contains(KeyModifiers::SHIFT) => {
                self.select_by_keyboard(false)
            }
            KeyCode::Up if self.choose_completion(false) => {}
            KeyCode::Down if self.choose_completion(true) => {}
            KeyCode::Up => self.up(),
            KeyCode::Down => self.composer.down(),
            KeyCode::Char(ch) if !ctrl => {
                self.composer.insert(ch);
                self.refresh_completion();
            }
            _ => {}
        }
    }

    /// One mouse event. `Shift` hands the gesture back to the terminal: the
    /// frame never sees a drag it must not take (ADR-0011).
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
            MouseEventKind::Drag(MouseButton::Left) => {
                if self.selection.dragging {
                    self.selection.head = at;
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                self.selection.finish();
                self.copy_selection();
            }
            MouseEventKind::ScrollUp => self.scroll_by(WHEEL_ROWS, true),
            MouseEventKind::ScrollDown => self.scroll_by(WHEEL_ROWS, false),
            _ => {}
        }
    }

    /// The keys an open overlay takes. It owns them: nothing behind it moves
    /// while it has the screen, and the page keys move it by the screenful its
    /// own box shows rather than the transcript's (ADR-0013, issue #49).
    fn overlay_key(&mut self, key: KeyEvent, ctrl: bool) {
        match key.code {
            KeyCode::PageUp => return self.scroll_page(true),
            KeyCode::PageDown => return self.scroll_page(false),
            _ => {}
        }
        if matches!(self.overlay, Some(Overlay::Settings(_))) {
            self.settings_key(key);
            return;
        }
        if matches!(self.overlay, Some(Overlay::Sessions(_))) {
            self.sessions_key(key, ctrl);
            return;
        }
        match key.code {
            KeyCode::Esc | KeyCode::Enter => self.overlay = None,
            KeyCode::Char('q') if ctrl => self.quit = true,
            KeyCode::Char('c') if ctrl => self.cancel_or_clear(),
            _ => {}
        }
    }

    /// A page, for whatever has the screen: the box that covers the frame
    /// sets the step it is paged by, and the transcript sets its own only when
    /// nothing covers it (issue #49).
    fn scroll_page(&mut self, up: bool) {
        match self.overlay.as_mut() {
            Some(Overlay::Help(help)) => help.page(self.window.page, up),
            Some(Overlay::Settings(settings)) => settings.page(self.window.page, up),
            Some(Overlay::Sessions(sessions)) => sessions.page(self.window.page, up),
            Some(Overlay::Spill(spill)) => spill.scroll_by(self.window.page, up),
            None => self.scroll_by(self.window.page, up),
        }
    }

    fn press(&mut self, at: Position) {
        // A row of the rail is a row like any other: the session id can be
        // taken out of the frame and pasted into `sindri sessions show`
        // (ADR-0006). The whole fact is copied, so a wrapped id is never half
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
        // never a selection: the row a reader points at is the one they mean
        // (ADR-0011 §9, issue #44).
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

    /// `↑` is the row above in the draft, and the newest queued prompt when
    /// the draft has no row above: the queue is reached from the top of the
    /// composer, never instead of it (issue #43).
    fn up(&mut self) {
        if self.composer.cursor().1 > 0 || !self.take_back() {
            self.composer.up();
        }
    }

    pub(crate) fn cancel_or_clear(&mut self) {
        match &self.running {
            Some(cancel) => cancel.trigger(),
            None => {
                self.composer.clear();
                self.completion = None;
            }
        }
    }

    fn enter(&mut self) {
        if self
            .completion
            .as_ref()
            .is_some_and(|open| !open.items.is_empty())
        {
            self.accept_completion();
            return;
        }
        // An empty composer has nothing to send: `Enter` there opens the call
        // the reader last touched, or the newest one, and folds it again
        // (ADR-0011 §9, issue #44).
        if self.composer.is_blank() && self.reveal() {
            return;
        }
        let draft = self.composer.text();
        let single = !draft.trim_end().contains('\n');
        if single && let Some(named) = command::parse(&draft) {
            self.composer.take();
            self.completion = None;
            self.run(named);
            return;
        }
        self.submit();
    }

    fn run(&mut self, named: Command) {
        match named {
            Command::New => self.new_session(),
            Command::Settings => self.open_settings(None),
            Command::Help => self.show(Overlay::Help(Help::default())),
            Command::Quit => self.quit = true,
            Command::Sessions => self.open_sessions(),
            Command::Export(path) => self.export(path.as_deref()),
            Command::Unknown(name) => self.note_error(command::refusal(&name)),
        }
    }

    /// `/new`: the front end is asked for a fresh session GUID, and the
    /// transcript is emptied only once it answers with one (ADR-0015): a
    /// session that cannot start leaves the conversation where it was. A run
    /// in flight is cancelled first, and the renew is queued behind it,
    /// because the driver's thread is one thread.
    fn new_session(&mut self) {
        if let Some(cancel) = &self.running {
            cancel.trigger();
        }
        self.selection = Selection::default();
        self.overlay = None;
        self.completion = None;
        self.renew = Renew::Asked;
    }

    fn dismiss(&mut self) {
        if self.completion.take().is_some() {
            self.walking = None;
            return;
        }
        self.selection = Selection::default();
    }

    fn show(&mut self, overlay: Overlay) {
        self.overlay = Some(overlay);
    }

    /// The keys know how many rows a page or a wheel is worth, and not where
    /// those rows land: the window is asked and the next frame answers, so a
    /// page back costs the rows it crosses and never the thread (issue #45).
    fn scroll_by(&mut self, rows: usize, up: bool) {
        let rows = rows as isize;
        self.window.want(Ask::Rows(if up { rows } else { -rows }));
    }

    /// Recomputes what the draft is completing — an `@` path, or a `/` command
    /// named at its start — and asks for a walk of the workspace only when the
    /// answer could have changed and only a path needs one (ADR-0011,
    /// ADR-0013).
    fn refresh_completion(&mut self) {
        let Some(query) = completion::ask(&self.composer.text(), self.composer.row()) else {
            self.completion = None;
            self.walking = None;
            return;
        };
        if self
            .completion
            .as_ref()
            .is_some_and(|open| open.answers(&query))
        {
            return;
        }
        self.generation = self.generation.wrapping_add(1);
        let open = Completion::new(query, self.generation);
        self.walking = open.walk();
        self.completion = Some(open);
    }

    fn accept_completion(&mut self) {
        let Some(open) = self.completion.as_ref() else {
            return;
        };
        let Some(path) = open.items.get(open.chosen).cloned() else {
            return;
        };
        let at = open.at;
        self.completion = None;
        self.walking = None;
        self.composer.splice(at, &path);
    }

    fn copy_last_answer(&mut self) {
        if !self.selection.is_empty() {
            self.copy_selection();
            return;
        }
        let answer = self.entries.iter().rev().find_map(|entry| match entry {
            Entry::Answer { text, .. } if !text.trim().is_empty() => Some(text.clone()),
            _ => None,
        });
        match answer {
            Some(text) => self.copy(&text),
            None => self.note_info("nothing to copy yet"),
        }
    }

    fn select_by_keyboard(&mut self, grow: bool) {
        let area = self.viewport.area;
        let rows = self.viewport.rows.len();
        if rows == 0 {
            return;
        }
        self.selecting = if grow {
            (self.selecting + 1).min(rows)
        } else {
            self.selecting.saturating_sub(1)
        };
        if self.selecting == 0 {
            self.selection = Selection::default();
            return;
        }
        let bottom = area.y + rows as u16 - 1;
        let top = bottom - (self.selecting as u16 - 1);
        self.selection = Selection {
            anchor: Position::new(area.x, top),
            head: Position::new(area.x + area.width.saturating_sub(1), bottom),
            dragging: false,
        };
    }

    fn choose_completion(&mut self, down: bool) -> bool {
        let Some(completion) = self.completion.as_mut() else {
            return false;
        };
        if completion.items.is_empty() {
            return false;
        }
        let last = completion.items.len() - 1;
        completion.chosen = if down {
            (completion.chosen + 1).min(last)
        } else {
            completion.chosen.saturating_sub(1)
        };
        true
    }

    fn open_spill(&mut self) {
        let path = self.entries.iter().rev().find_map(|entry| match entry {
            Entry::Call(call) => call.spill_file().map(std::path::Path::to_path_buf),
            _ => None,
        });
        match path {
            Some(path) => self.show_spill(&path),
            None => self.note_info("no output in this session was too large to show"),
        }
    }

    fn show_spill(&mut self, path: &std::path::Path) {
        match Spill::open(path) {
            Ok(spill) => self.overlay = Some(Overlay::Spill(spill)),
            Err(error) => self.note_error(format!("cannot open {}: {error}", path.display())),
        }
    }
}
