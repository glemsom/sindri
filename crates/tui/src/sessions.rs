//! The record picker: what every run left under the state directory, read from
//! `session.json`, newest first (ADR-0006, issue #51). Reading is not resuming —
//! nothing here writes a record, and nothing opens a run (ADR-0011 §8).

use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent};
use sindri_core::paths::SessionDir;
use sindri_core::record::{Stored, Summary, summaries};

use crate::app::App;
use crate::overlay::Overlay;
use crate::spill::Spill;

/// One listing of the records, and where the reader is in it. The rows are read
/// once, when `/sessions` opens: a picker is a list of what existed at that
/// moment, not a directory watched while a frame is up.
#[derive(Debug, Clone)]
pub struct Sessions {
    root: PathBuf,
    rows: Vec<Summary>,
    chosen: usize,
}

impl Sessions {
    pub fn open(root: impl Into<PathBuf>) -> Sessions {
        let root = root.into();
        Sessions {
            rows: summaries(&root),
            root,
            chosen: 0,
        }
    }

    pub fn rows(&self) -> &[Summary] {
        &self.rows
    }

    pub fn chosen(&self) -> usize {
        self.chosen
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn choose(&mut self, down: bool) {
        self.chosen = crate::overlay::step(self.chosen, 1, self.rows.len(), !down);
    }

    /// A screenful at a time: `PgUp`/`PgDn` and the wheel move the picker, and
    /// the transcript beneath it stays where it was (issue #49).
    pub fn page(&mut self, rows: usize, up: bool) {
        self.chosen = crate::overlay::step(self.chosen, rows, self.rows.len(), up);
    }

    pub fn current(&self) -> Option<&Summary> {
        self.rows.get(self.chosen)
    }

    /// The record itself, as the same [`Stored`] `sindri sessions show` reads:
    /// the frame and the command cannot disagree about what a session said.
    pub fn read(&self) -> Result<Stored, String> {
        let Some(row) = self.current() else {
            return Err("that record is no longer listed".to_string());
        };
        Stored::load_in(&SessionDir::at(self.root.join(&row.id)))
            .map_err(|error| format!("session {} cannot be read: {error}", row.id))
    }
}

impl App {
    /// The picker's own keys: a list is walked with the arrows, `Enter` reads
    /// the row, and `Esc` closes it. Nothing here can write: the one reader it
    /// opens is the record `sindri sessions show` prints.
    pub(crate) fn sessions_key(&mut self, key: KeyEvent, ctrl: bool) {
        match key.code {
            KeyCode::Esc => self.overlay = None,
            KeyCode::Enter => self.read_session(),
            KeyCode::Up => self.choose_session(false),
            KeyCode::Down => self.choose_session(true),
            KeyCode::Char('q') if ctrl => self.quit = true,
            KeyCode::Char('c') if ctrl => self.cancel_or_clear(),
            _ => {}
        }
    }

    /// `/sessions`: the records the state directory holds, newest first.
    pub(crate) fn open_sessions(&mut self) {
        let Some(root) = self.sessions.clone() else {
            return self.note_error("sessions: the record directory is unknown");
        };
        let sessions = Sessions::open(root);
        if sessions.is_empty() {
            return self.note_info(format!(
                "no sessions recorded in {} yet",
                sessions.root().display()
            ));
        }
        self.overlay = Some(Overlay::Sessions(sessions));
    }

    fn choose_session(&mut self, down: bool) {
        if let Some(Overlay::Sessions(sessions)) = self.overlay.as_mut() {
            sessions.choose(down);
        }
    }

    /// `Enter` on a record: its transcript, in the box a spill opens in, read
    /// only (ADR-0011 §8 — reading is not resuming).
    fn read_session(&mut self) {
        let Some(Overlay::Sessions(sessions)) = self.overlay.as_ref() else {
            return;
        };
        let title = match sessions.current() {
            Some(row) => format!("session {}", row.id),
            None => return,
        };
        match sessions.read() {
            Ok(stored) => self.overlay = Some(Overlay::Spill(Spill::text(title, &stored.render()))),
            Err(error) => self.note_error(error),
        }
    }
}
