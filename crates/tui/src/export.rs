//! `/export`: the thread the frame is holding, written as markdown — where the
//! user names, or into the workspace under a name derived from the session id
//! (issue #51). The text comes from the record, so the file says what
//! `sindri sessions show` prints; the path is said before a byte is
//! written, and a file that is already there is never replaced on the first ask.

use std::path::PathBuf;

use sindri_core::paths::SessionDir;
use sindri_core::record::Stored;

use crate::app::App;

impl App {
    /// `/export [path]`. A path already written through this command, or a
    /// second `/export` for the same file, is the confirmation: the refusal
    /// says so and writes nothing.
    pub(crate) fn export(&mut self, named: Option<&str>) {
        let Some(root) = self.sessions.clone() else {
            return self.note_error(
                "export: the record directory is unknown, so there is no thread to write",
            );
        };
        let id = self.rail.session();
        let stored = match Stored::load_in(&SessionDir::at(root.join(&id))) {
            Ok(stored) => stored,
            Err(error) => {
                return self.note_error(format!(
                    "export: the record of {id} cannot be read: {error}"
                ));
            }
        };
        let path = self.export_path(named);
        if path.exists() && self.export_pending.as_deref() != Some(path.as_path()) {
            self.note_error(format!(
                "export: {} already exists — /export again to overwrite it",
                path.display()
            ));
            self.export_pending = Some(path);
            return;
        }
        self.note_info(format!("export: writing {}", path.display()));
        self.export_pending = None;
        let markdown = stored.markdown();
        match std::fs::write(&path, &markdown) {
            Ok(()) => self.note_info(format!(
                "exported {} ({} lines)",
                path.display(),
                markdown.lines().count()
            )),
            Err(error) => self.note_error(format!(
                "export: {} cannot be written: {error}",
                path.display()
            )),
        }
    }

    /// Where the file goes: the path that was named, relative to the workspace,
    /// or the workspace's own `sindri-<guid>.md`.
    fn export_path(&self, named: Option<&str>) -> PathBuf {
        let workspace = self.workspace_dir();
        match named.map(str::trim).filter(|named| !named.is_empty()) {
            Some(named) => {
                let path = PathBuf::from(named);
                if path.is_absolute() {
                    path
                } else {
                    workspace.join(path)
                }
            }
            None => workspace.join(format!("sindri-{}.md", self.rail.session())),
        }
    }
}
