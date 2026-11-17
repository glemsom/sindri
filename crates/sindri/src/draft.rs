//! The inks one row of the draft is drawn in: the command it opens with, the files its words name,
//! and the spans it sets in backticks.

use crate::commands;
use crate::files::Files;
use crate::ink::Ink;

const TICK: char = '`';

/// The runs one drawn row of the draft is made of, in the order they sit, each with the ink it
/// carries. `opens` says the row is the draft's own first row, the only row a command can open.
pub fn runs<'a>(row: &'a str, opens: bool, files: &Files) -> Vec<(&'a str, Ink)> {
    let mut runs: Vec<(&str, Ink)> = Vec::new();
    let mut first = opens;
    let mut rest = row;
    while !rest.is_empty() {
        let (plain, span) = match rest.find(TICK) {
            Some(at) => rest.split_at(at),
            None => (rest, ""),
        };
        first = words(&mut runs, plain, first, files);
        let closed = match span.strip_prefix(TICK).and_then(|after| after.find(TICK)) {
            Some(at) => at + 2,
            None => {
                words(&mut runs, span, first, files);
                break;
            }
        };
        runs.push((&span[..closed], Ink::Muted));
        first = false;
        rest = &span[closed..];
    }
    runs
}

/// The words and the blanks of one plain stretch, in order, and whether the row still opens.
fn words<'a>(runs: &mut Vec<(&'a str, Ink)>, plain: &'a str, first: bool, files: &Files) -> bool {
    let mut rest = plain;
    let mut first = first;
    while !rest.is_empty() {
        let at = rest.find(char::is_whitespace).unwrap_or(rest.len());
        let (word, after) = rest.split_at(at);
        if !word.is_empty() {
            runs.push((word, ink(word, first, files)));
            first = false;
        }
        let blank = after.len() - after.trim_start().len();
        if blank > 0 {
            runs.push((&after[..blank], Ink::Plain));
        }
        rest = &after[blank..];
    }
    first
}

/// The ink one word carries: the accent for the row's own first word where it names a command the
/// card holds, the ok ink where it names a file of the workspace, and the plain ink otherwise.
fn ink(word: &str, first: bool, files: &Files) -> Ink {
    match (first && commands::recognised(word), files.names(word)) {
        (true, _) => Ink::Accent,
        (false, true) => Ink::Ok,
        (false, false) => Ink::Plain,
    }
}

#[cfg(test)]
mod tests;
