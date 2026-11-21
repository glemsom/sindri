//! The bill of a draft: the files and the command its words name — the count the band's edge hangs
//! on its far corner.

use std::collections::BTreeSet;

use crate::files::Files;

/// What one draft would send, counted: the material the band's edge hangs in words.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bill {
    files: usize,
    command: bool,
}

impl Bill {
    /// The bill of `draft`, and none at all for a draft that names no file and opens with no
    /// command.
    pub fn of(draft: &str, files: &Files) -> Option<Bill> {
        // A file a draft names twice is one file of material, and the words in the order they sit
        // say nothing a count of the set does not.
        let bill = Bill {
            files: draft
                .split_whitespace()
                .filter(|word| files.names(word))
                .collect::<BTreeSet<&str>>()
                .len(),
            command: crate::commands::read(draft).is_some(),
        };
        match bill.files == 0 && !bill.command {
            true => None,
            false => Some(bill),
        }
    }

    /// The count in words: one part per count that is not zero.
    pub fn parts(&self) -> Vec<String> {
        let mut parts = Vec::new();
        match self.files {
            0 => {}
            1 => parts.push(String::from("1 file")),
            files => parts.push(format!("{files} files")),
        }
        if self.command {
            parts.push(String::from("1 command"));
        }
        parts
    }
}

#[cfg(test)]
mod tests;
