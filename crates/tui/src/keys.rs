//! The hands: what one key, or one gesture, does to the frame (ADR-0011 §7,
//! §14). Everything the mouse can do is here as a key as well, because a
//! pointer is an accelerator and never the only way in.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Position;

use crate::app::{App, Renew};
use crate::command::{self, Command};
use crate::completion::{self, Completion};
use crate::copy::Copied;
use crate::entry::Entry;
use crate::overlay::{Overlay, Spill};
use crate::selection::{self, Selection};

/// How many rows one turn of the wheel moves: a nudge, not a page.
const WHEEL_ROWS: usize = 3;

impl App {
    /// One key. Enter sends or queues, Shift+Enter breaks the line, Ctrl-C
    /// cancels the run and keeps the draft, Ctrl-Q leaves (ADR-0011 §14).
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
            KeyCode::Char('?') if self.composer.is_blank() => self.show(Overlay::Help),
            KeyCode::Esc => self.dismiss(),
            KeyCode::Tab => self.accept_completion(),
            KeyCode::PageUp => self.scroll_page(true),
            KeyCode::PageDown => self.scroll_page(false),
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
            // Up and Down walk the completion while one is open, and the
            // composer's own rows otherwise.
            KeyCode::Up if self.choose_completion(false) => {}
            KeyCode::Down if self.choose_completion(true) => {}
            KeyCode::Up => self.composer.up(),
            KeyCode::Down => self.composer.down(),
            KeyCode::Char(ch) if !ctrl => {
                self.composer.insert(ch);
                self.refresh_completion();
            }
            _ => {}
        }
    }

    /// One mouse event. `Shift` hands the gesture back to the terminal: the
    /// frame never sees a drag it must not take (ADR-0011 §7).
    pub fn mouse(&mut self, event: MouseEvent) {
        if event.modifiers.contains(KeyModifiers::SHIFT) {
            return;
        }
        if self.overlay.is_some() {
            // An overlay has the screen: the wheel pages it, and nothing else
            // reaches the transcript underneath.
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

    /// What a key does while an overlay has the screen: close it, page it, or
    /// leave. Nothing types behind an overlay the user cannot see through.
    fn overlay_key(&mut self, key: KeyEvent, ctrl: bool) {
        if matches!(self.overlay, Some(Overlay::Settings(_))) {
            self.settings_key(key);
            return;
        }
        match key.code {
            KeyCode::Esc | KeyCode::Enter => self.overlay = None,
            KeyCode::PageUp => self.scroll_page(true),
            KeyCode::PageDown => self.scroll_page(false),
            KeyCode::Char('q') if ctrl => self.quit = true,
            KeyCode::Char('c') if ctrl => self.cancel_or_clear(),
            _ => {}
        }
    }

    /// A click: the spill affordance opens its file, and anywhere else in the
    /// transcript begins a selection.
    fn press(&mut self, at: Position) {
        let offered = self
            .viewport
            .row_of(at)
            .and_then(|row| self.viewport.spilled(row))
            .cloned();
        if let Some(path) = offered {
            self.show_spill(&path);
            return;
        }
        if self.viewport.holds(at) {
            self.selection = Selection::start(at);
        }
    }

    /// Ctrl-C: the run stops and the draft stays; with nothing running, the
    /// draft is cleared.
    pub(crate) fn cancel_or_clear(&mut self) {
        match &self.running {
            Some(cancel) => cancel.trigger(),
            None => {
                self.composer.clear();
                self.completion = None;
            }
        }
    }

    /// Enter: close an overlay, take a completion, run a command, or send.
    fn enter(&mut self) {
        if self
            .completion
            .as_ref()
            .is_some_and(|open| !open.items.is_empty())
        {
            self.accept_completion();
            return;
        }
        let draft = self.composer.text();
        // A command is one row: a prompt whose first line happens to start with
        // a slash is still a prompt.
        let single = !draft.trim_end().contains('\n');
        if single && let Some(named) = command::parse(&draft) {
            self.composer.take();
            self.completion = None;
            self.run(named);
            return;
        }
        self.submit();
    }

    /// One `/` command (ADR-0011 §8, §14).
    fn run(&mut self, named: Command) {
        match named {
            Command::New => self.new_session(),
            Command::Settings => self.open_settings(),
            Command::Help => self.show(Overlay::Help),
            Command::Quit => self.quit = true,
            Command::Unknown(name) => {
                let name = name.clone();
                self.note_error(format!("{name}: no such command — /new, /help, /quit"));
            }
        }
    }

    /// `/new`: the front end is asked for a fresh session GUID, and the
    /// transcript is emptied only once it answers with one (ADR-0015 §2): a
    /// session that cannot start leaves the conversation where it was. A run in
    /// flight is cancelled first, and the renew is queued behind it, because the
    /// driver's thread is one thread.
    fn new_session(&mut self) {
        if let Some(cancel) = &self.running {
            cancel.trigger();
        }
        self.selection = Selection::default();
        self.overlay = None;
        self.completion = None;
        self.renew = Renew::Asked;
    }

    /// `Esc`: the completion, then the selection.
    fn dismiss(&mut self) {
        if self.completion.take().is_some() {
            self.walking = None;
            return;
        }
        self.selection = Selection::default();
    }

    /// Something that has the screen until `Esc`.
    fn show(&mut self, overlay: Overlay) {
        self.overlay = Some(overlay);
    }

    /// PgUp and PgDn, or one screenful of a spill file.
    fn scroll_page(&mut self, up: bool) {
        if let Some(Overlay::Spill(spill)) = self.overlay.as_mut() {
            spill.scroll_by(self.page, up);
            return;
        }
        self.scroll_by(self.page, up);
    }

    /// One scroll, clamped at the newest row. The window clamps the other end:
    /// a thread has a top, and scrolling past it shows nothing new.
    fn scroll_by(&mut self, rows: usize, up: bool) {
        self.scroll = if up {
            self.scroll.saturating_add(rows)
        } else {
            self.scroll.saturating_sub(rows)
        };
    }

    /// Recomputes what `@` is completing, and asks for a walk of the workspace
    /// only when the answer could have changed (ADR-0011 §16).
    fn refresh_completion(&mut self) {
        let Some(query) = completion::query(self.composer.row()) else {
            self.completion = None;
            self.walking = None;
            return;
        };
        if self
            .completion
            .as_ref()
            .is_some_and(|open| open.typed == query.typed && open.at == query.at)
        {
            return;
        }
        self.generation = self.generation.wrapping_add(1);
        self.walking = Some((self.generation, query.typed.clone()));
        self.completion = Some(Completion::new(query, self.generation));
    }

    /// `Tab` or `Enter`: the offered path replaces what was typed after the
    /// `@`, and the completion closes.
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

    /// Ctrl-Y, the keyboard twin of a drag: the rows a keyboard selection
    /// covers, or the newest answer when nothing is selected.
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

    /// Shift+Up and Shift+Down: a selection made without a pointer, over whole
    /// rows from the newest one upwards (ADR-0011 §7).
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

    /// Moves the mark down or up the offered paths, and says whether there was
    /// an offer to move in.
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

    /// The selection, as the rows it covers (ADR-0011 §7).
    fn copy_selection(&mut self) {
        if let Some(text) = selection::copied(&self.viewport, &self.selection) {
            self.copy(&text);
        }
        // Copied or not, the selection is spent: a second Ctrl-Y copies the
        // last answer rather than silently repeating the last drag.
        self.selection = Selection::default();
        self.selecting = 0;
    }

    /// A copy, and the truth about it: sent is not the same as copied, and a
    /// refusal is a refusal (ADR-0013).
    fn copy(&mut self, text: &str) {
        let rows = text.lines().count();
        let said = match self.clipboard.copy(text) {
            // A terminal does not answer, so this says the copy was sent and
            // says outright that nobody confirmed it.
            Copied::Sent => {
                format!("copy sent: {rows} rows as OSC 52, which the terminal does not confirm")
            }
            Copied::Tool(tool) => format!("copied {rows} rows via {tool}"),
            Copied::Refused(why) => return self.note_error(format!("copy refused: {why}")),
        };
        self.note_info(said);
    }

    /// Ctrl-O, and a click on the affordance row: the untruncated output of the
    /// newest command whose output did not fit (ADR-0011 §9).
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

    /// The spill file one row of the transcript offers, opened.
    fn show_spill(&mut self, path: &std::path::Path) {
        match Spill::open(path) {
            Ok(spill) => self.overlay = Some(Overlay::Spill(spill)),
            Err(error) => self.note_error(format!("cannot open {}: {error}", path.display())),
        }
    }
}
