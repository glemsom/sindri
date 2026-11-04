//! The spill the reader asked for, as the frame holds the ask: which file a
//! thread is reading, and that one is out at all — so a key pressed while it is
//! being read is the same ask rather than a second one, and the file is never
//! read on the frame's own thread.

use std::path::PathBuf;

use super::App;
use crate::overlay::Overlay;
use crate::spill::Spill;

/// Where `Ctrl-O`'s read has got to, in the same three states as [`Loading`]:
/// the frame marks the request, the loop hands it to a thread once, and the box
/// opens where the answer lands.
///
/// [`Loading`]: super::Loading
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) enum Spilling {
    #[default]
    Idle,
    /// The file the reader asked for, not yet handed over.
    Asked(PathBuf),
    /// Handed over to a thread, and the words have not come back: the answer
    /// names its file, so one made of another file is not this read's.
    Reading(PathBuf),
}

impl App {
    /// Whether a spill is asked for or being read: a key pressed now is the same
    /// ask, and the frame says nothing new with it.
    pub fn is_spilling(&self) -> bool {
        self.spilling != Spilling::Idle
    }

    /// Asks for a spill of `path`, unless one is already asked for or out. A
    /// spill exists because the output was too big to show, so a held key is
    /// one read of one file rather than a read per keystroke.
    pub(crate) fn ask_spill(&mut self, path: impl Into<PathBuf>) {
        if self.spilling == Spilling::Idle {
            self.spilling = Spilling::Asked(path.into());
        }
    }

    /// The file the frame asked for, once: what the loop hands a thread of its
    /// own. The ask is waited on and not forgotten, so the keys pressed while
    /// the read is out find it still answered for.
    pub fn take_spill(&mut self) -> Option<PathBuf> {
        match std::mem::replace(&mut self.spilling, Spilling::Idle) {
            Spilling::Asked(path) => {
                self.spilling = Spilling::Reading(path.clone());
                Some(path)
            }
            asked => {
                self.spilling = asked;
                None
            }
        }
    }

    /// What a thread made of the ask, and whether it drew anything: the box
    /// opens on the words that arrived, or the transcript names the file that
    /// could not be read and why — an output nobody can open is a fact a reader
    /// is told rather than a key that did nothing. An answer the frame is no
    /// longer reading for belongs to a session that has gone, and is dropped
    /// where it lands.
    pub fn spilled(&mut self, path: PathBuf, read: std::io::Result<Spill>) -> bool {
        if !matches!(&self.spilling, Spilling::Reading(reading) if *reading == path) {
            return false;
        }
        self.spilling = Spilling::Idle;
        match read {
            Ok(spill) => self.show(Overlay::Spill(spill)),
            Err(error) => self.note_error(format!("cannot open {}: {error}", path.display())),
        }
        true
    }
}
