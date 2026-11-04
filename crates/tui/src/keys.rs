//! The hands: what one key does to the frame, and the overlays that take the
//! keys before it does. The pointer is `mouse`'s, beside this — everything it
//! can do is here as a key as well, because a pointer is an accelerator and
//! never the only way in.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Position;

use crate::app::{App, Renew};
use crate::command::{self, Command};
use crate::completion::{self, Completion};
use crate::entry::Entry;
use crate::overlay::{Help, Overlay};
use crate::selection::Selection;
use crate::spill::Spill;
use crate::window::Ask;

/// The rows one notch of the wheel is worth.
pub(crate) const WHEEL_ROWS: usize = 3;

mod mouse;
mod skill;

impl App {
    /// One key, and whether it left the frame something new to draw. Every arm
    /// answers for itself: a key no arm takes says nothing, a cursor that cannot
    /// move where it was asked to says nothing, and a key that spends a standing
    /// mark says something even where the rest of it does nothing, because the
    /// mark going is a row of the frame changing.
    pub fn key(&mut self, key: KeyEvent) -> bool {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if self.overlay.is_some() {
            return self.overlay_key(key, ctrl);
        }
        // A mark stands until the reader's next gesture, and every key is one —
        // but the copy the mark is waiting for, and the two keys that draw a
        // mark of their own over it and must not lose the count they grow by.
        let spent = !keeps_mark(key) && self.clear_mark();
        let said = match key.code {
            KeyCode::Char('q') if ctrl => {
                self.quit = true;
                true
            }
            KeyCode::Char('c') if ctrl => self.cancel_or_clear(),
            KeyCode::Char('y') if ctrl => {
                self.copy_last_answer();
                true
            }
            KeyCode::Char('o') if ctrl => {
                self.open_spill();
                true
            }
            KeyCode::Char('x') if ctrl => self.drop_queued(),
            KeyCode::Char('g') if ctrl => {
                self.copy_session_id();
                true
            }
            KeyCode::Char('?') if self.composer.is_blank() => {
                self.show(Overlay::Help(Help::default()));
                true
            }
            KeyCode::Esc => self.dismiss(),
            KeyCode::Tab => self.accept_completion(),
            // A page, the first turn and the tail are questions asked of the
            // window, which is the only thing that knows how many rows the
            // thread holds: the next frame answers them, so the key has said
            // something even where it was already there.
            KeyCode::PageUp => {
                self.scroll_page(true);
                true
            }
            KeyCode::PageDown => {
                self.scroll_page(false);
                true
            }
            KeyCode::Home => {
                self.window.to_top();
                true
            }
            KeyCode::End => {
                self.window.to_tail();
                true
            }
            KeyCode::Enter if key.modifiers.contains(KeyModifiers::SHIFT) => {
                self.composer.newline();
                self.refresh_completion();
                true
            }
            // `0x0a` is `Ctrl-J` in raw mode, which is how a newline inside a
            // paste arrives at a terminal that sends off the clipboard without
            // asking: it breaks the row here, as `Shift+Enter` does.
            KeyCode::Char('j') if ctrl => {
                self.composer.newline();
                self.refresh_completion();
                true
            }
            KeyCode::Enter => self.enter(),
            KeyCode::Backspace => {
                let erased = self.composer.backspace();
                self.refresh_completion();
                erased
            }
            KeyCode::Left => self.composer.left(),
            KeyCode::Right => self.composer.right(),
            KeyCode::Up if key.modifiers.contains(KeyModifiers::SHIFT) => {
                self.select_by_keyboard(true)
            }
            KeyCode::Down if key.modifiers.contains(KeyModifiers::SHIFT) => {
                self.select_by_keyboard(false)
            }
            KeyCode::Up if self.choose_completion(false) => true,
            KeyCode::Down if self.choose_completion(true) => true,
            KeyCode::Up => self.up(),
            KeyCode::Down => self.composer.down(),
            KeyCode::Char(ch) if !ctrl => {
                self.composer.insert(ch);
                self.refresh_completion();
                true
            }
            // A key the frame binds to nothing: it takes no row with it.
            _ => false,
        };
        said || spent
    }

    /// A paste is one draft and never a keystroke: the text lands at the cursor
    /// as its own rows, and nothing is sent or queued — the reader presses
    /// `Enter` when they are ready, and that key is the only one that sends.
    /// A run in flight changes nothing, because the queue is `Enter`'s. An open
    /// overlay owns the screen, so the paste is the overlay's: the two that
    /// carry a field take it as one edit of the field they are on — `/settings`'
    /// row and `/model`'s fragment — and the ones that carry none, the bindings'
    /// list and a spill, ignore it like every other event that is not a key.
    pub fn paste(&mut self, text: &str) -> bool {
        match self.overlay.as_mut() {
            Some(Overlay::Settings(settings)) => settings.paste(text, &self.served),
            Some(Overlay::Model(model)) => model.paste(text, &self.served),
            Some(Overlay::Help(_) | Overlay::Spill(_)) => false,
            None => {
                // A mark goes with the draft it stood over, so a paste of
                // nothing that spends one has still taken a row of the frame.
                let spent = self.clear_mark();
                self.composer.insert_str(text) || spent
            }
        }
    }

    /// The keys an open overlay takes. It owns them: nothing behind it moves
    /// while it has the screen, and the page keys move it by the screenful its
    /// own box shows rather than the transcript's. A key the overlay does not
    /// take changes nothing in the frame, the box under the key included.
    fn overlay_key(&mut self, key: KeyEvent, ctrl: bool) -> bool {
        match key.code {
            KeyCode::PageUp => {
                self.scroll_page(true);
                return true;
            }
            KeyCode::PageDown => {
                self.scroll_page(false);
                return true;
            }
            _ => {}
        }
        if matches!(self.overlay, Some(Overlay::Settings(_))) {
            return self.settings_key(key);
        }
        if matches!(self.overlay, Some(Overlay::Model(_))) {
            return self.model_key(key);
        }
        match key.code {
            KeyCode::Esc | KeyCode::Enter => {
                self.overlay = None;
                true
            }
            KeyCode::Char('q') if ctrl => {
                self.quit = true;
                true
            }
            KeyCode::Char('c') if ctrl => self.cancel_or_clear(),
            _ => false,
        }
    }

    /// A page, for whatever has the screen: the box that covers the frame
    /// sets the step it is paged by, and the transcript sets its own only when
    /// nothing covers it.
    fn scroll_page(&mut self, up: bool) {
        match self.overlay.as_mut() {
            Some(Overlay::Help(help)) => help.page(self.window.page, up),
            Some(Overlay::Settings(settings)) => settings.page(self.window.page, up),
            Some(Overlay::Model(model)) => model.page(self.window.page, up),
            Some(Overlay::Spill(spill)) => spill.scroll.page_by(self.window.page, up),
            None => self.scroll_by(self.window.page, up),
        }
    }

    /// `↑` is the row above in the draft, and the newest queued prompt when
    /// the draft has no row above: the queue is reached from the top of the
    /// composer, never instead of it.
    fn up(&mut self) -> bool {
        if self.composer.cursor().1 > 0 || !self.take_back() {
            return self.composer.up();
        }
        true
    }

    pub(crate) fn cancel_or_clear(&mut self) -> bool {
        let Some(cancel) = self.running.clone() else {
            let cleared = self.composer.clear();
            let offered = self.completion.take().is_some();
            self.walking = None;
            return cleared || offered;
        };
        cancel.trigger();
        // A cancel is answered the moment the key lands, so the thought the run
        // was having ends where it stands: its marker row stops saying the run
        // is thinking, and it claims only the time the run spent.
        self.close_thought();
        true
    }

    fn enter(&mut self) -> bool {
        if self
            .completion
            .as_ref()
            .is_some_and(|open| !open.items.is_empty())
        {
            return self.accept_completion();
        }
        // An empty composer has nothing to send: `Enter` there opens the call
        // the reader last touched, or the newest one, and folds it again.
        if self.composer.is_blank() && self.reveal() {
            return true;
        }
        let draft = self.composer.text();
        let single = !draft.trim_end().contains('\n');
        if single && let Some(named) = command::parse(&draft) {
            self.composer.take();
            self.completion = None;
            self.run(named);
            return true;
        }
        self.submit()
    }

    fn run(&mut self, named: Command) {
        match named {
            Command::New => self.new_session(),
            Command::Model(name) => match name {
                Some(name) => self.set_model(&name),
                None => self.open_model(),
            },
            Command::Settings => self.open_settings(None),
            Command::Help => self.show(Overlay::Help(Help::default())),
            Command::Quit => self.quit = true,
            Command::Export(path) => self.export(path.as_deref()),
            Command::Skill(ask) => self.skill_command(ask),
            Command::Unknown(name) => self.note_error(command::refusal(&name)),
        }
    }

    /// `/new`: the front end is asked for a fresh session GUID, and the
    /// transcript is emptied only once it answers with one: a
    /// session that cannot start leaves the conversation where it was. A run
    /// in flight is cancelled first, and the renew is queued behind it,
    /// because the driver's thread is one thread.
    fn new_session(&mut self) {
        if let Some(cancel) = &self.running {
            cancel.trigger();
        }
        self.clear_mark();
        self.overlay = None;
        self.completion = None;
        self.renew = Renew::Asked;
    }

    fn dismiss(&mut self) -> bool {
        if self.completion.take().is_some() {
            self.walking = None;
            return true;
        }
        self.clear_mark()
    }

    /// An overlay takes the screen, and a mark under it would be a lit cell
    /// nobody can see: the mark goes with the frame it was drawn on.
    pub(crate) fn show(&mut self, overlay: Overlay) {
        self.clear_mark();
        self.overlay = Some(overlay);
    }

    /// The keys know how many rows a page or a wheel is worth, and not where
    /// those rows land: the window is asked and the next frame answers, so a
    /// page back costs the rows it crosses and never the thread.
    fn scroll_by(&mut self, rows: usize, up: bool) {
        let rows = rows as isize;
        self.window.want(Ask::Rows(if up { rows } else { -rows }));
    }

    /// Recomputes what the draft is completing — an `@` path, or a `/` command
    /// named at its start — and asks for a walk of the workspace only when the
    /// answer could have changed and only a path needs one.
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
        let open = Completion::new(query, self.generation, &self.rail.launch().skills);
        self.walking = open.walk();
        self.completion = Some(open);
    }

    fn accept_completion(&mut self) -> bool {
        let Some(open) = self.completion.as_ref() else {
            return false;
        };
        let Some(path) = open.items.get(open.chosen).cloned() else {
            return false;
        };
        let at = open.at;
        self.completion = None;
        self.walking = None;
        self.composer.splice(at, &path);
        true
    }

    fn copy_last_answer(&mut self) {
        if !self.selection.is_empty() {
            self.copy_selection();
            return;
        }
        let answer = self
            .thread
            .entries()
            .iter()
            .rev()
            .find_map(|entry| match entry {
                Entry::Answer { text, .. } if !text.trim().is_empty() => Some(text.clone()),
                _ => None,
            });
        match answer {
            Some(text) => self.copy(&text),
            None => self.note_info("nothing to copy yet"),
        }
    }

    fn select_by_keyboard(&mut self, grow: bool) -> bool {
        let area = self.viewport.area;
        let rows = self.viewport.rows.len();
        if rows == 0 {
            return false;
        }
        self.selecting = if grow {
            (self.selecting + 1).min(rows)
        } else {
            self.selecting.saturating_sub(1)
        };
        if self.selecting == 0 {
            self.clear_mark();
            return true;
        }
        let bottom = area.y + rows as u16 - 1;
        let top = bottom - (self.selecting as u16 - 1);
        self.selection = Selection {
            anchor: Position::new(area.x, top),
            head: Position::new(area.x + area.width.saturating_sub(1), bottom),
            dragging: false,
        };
        true
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
        let path = self
            .thread
            .entries()
            .iter()
            .rev()
            .find_map(|entry| match entry {
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
            Ok(spill) => self.show(Overlay::Spill(spill)),
            Err(error) => self.note_error(format!("cannot open {}: {error}", path.display())),
        }
    }
}

/// Whether a key is the mark's own rather than a gesture that spends it:
/// `Ctrl-Y` copies the rows a standing mark covers, and `Shift`+`↑`/`↓` redraw
/// the mark as a selection grown from the bottom row — which replaces what the
/// drag took and copies nothing. Neither has the mark cleared under it, the
/// first because it is what the mark is for and the second because clearing it
/// would take the count it grows by.
fn keeps_mark(key: KeyEvent) -> bool {
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    match key.code {
        KeyCode::Char('y') if key.modifiers.contains(KeyModifiers::CONTROL) => true,
        KeyCode::Up | KeyCode::Down if shift => true,
        _ => false,
    }
}
