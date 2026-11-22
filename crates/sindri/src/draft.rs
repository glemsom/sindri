use crate::commands;
use crate::files::Files;
use crate::ink::Ink;

const TICK: char = '`';

pub fn runs<'a>(row: &'a str, opens: bool, files: &Files) -> Vec<(&'a str, Ink)> {
    let mut runs: Vec<(&'a str, Ink)> = Vec::new();
    let mut first = opens;
    let mut rest = row;
    loop {
        let Some(at) = rest.find(TICK) else {
            words(&mut runs, rest, first, files);
            break;
        };
        words(&mut runs, &rest[..at], first, files);
        first = false;
        let span = &rest[at..];
        let Some(close) = span[1..].find(TICK) else {
            runs.push((span, Ink::Plain));
            break;
        };
        runs.push((&span[..close + 2], Ink::Muted));
        rest = &span[close + 2..];
    }
    runs
}

fn words<'a>(runs: &mut Vec<(&'a str, Ink)>, plain: &'a str, first: bool, files: &Files) {
    let mut first = first;
    let mut rest = plain;
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
            first = false;
        }
        rest = &after[blank..];
    }
}

fn ink(word: &str, first: bool, files: &Files) -> Ink {
    match (first && commands::recognised(word), files.names(word)) {
        (true, _) => Ink::Accent,
        (false, true) => Ink::Ok,
        (false, false) => Ink::Plain,
    }
}

#[cfg(test)]
mod tests;
