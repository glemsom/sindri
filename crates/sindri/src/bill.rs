use std::collections::BTreeSet;

use crate::files::Files;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bill {
    files: usize,
    command: bool,
}

impl Bill {
    pub fn of(draft: &str, files: &Files) -> Option<Bill> {
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
