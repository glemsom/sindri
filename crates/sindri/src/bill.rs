//! The bill of a draft: the characters it holds, and the files and the command its words name — the
//! count the band's edge hangs on its far corner.

use crate::files::Files;

/// What one draft costs, counted once and said in words by the band's edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bill {
    files: usize,
    commands: usize,
    chars: usize,
}

impl Bill {
    /// The bill of `draft`, and none at all for a draft holding no character.
    pub fn of(draft: &str, files: &Files) -> Option<Bill> {
        match draft.is_empty() {
            true => None,
            false => Some(Bill {
                files: draft
                    .split_whitespace()
                    .filter(|word| files.names(word))
                    .count(),
                commands: usize::from(crate::commands::read(draft).is_some()),
                chars: draft.char_indices().count(),
            }),
        }
    }

    /// The count in words: one part per count that is not zero.
    pub fn parts(&self) -> Vec<String> {
        let mut parts = Vec::new();
        if self.files > 0 {
            parts.push(spoken(self.files, "file", "files"));
        }
        if self.commands > 0 {
            parts.push(spoken(self.commands, "command", "commands"));
        }
        if self.chars > 0 {
            parts.push(spoken(self.chars, "char", "chars"));
        }
        parts
    }
}

/// `count` and the word it counts, in the one number the word agrees with.
fn spoken(count: usize, one: &str, many: &str) -> String {
    match count {
        1 => format!("1 {one}"),
        count => format!("{count} {many}"),
    }
}

#[cfg(test)]
mod tests;
