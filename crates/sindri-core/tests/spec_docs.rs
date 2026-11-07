//! Pins the rules that keep the documentation's own record honest: every
//! Appendix A entry states a question and a resolution that says where the
//! answer lives, and every cross-reference the docs make names something real.
//!
//! The appendix is read as text, because its failure mode is invisible: a
//! resolution pasted over the end of the question it answers leaves an entry
//! that still renders, still compiles and still reads as closed.
//!
//! Deliberately out of scope: whether an entry's answer is the *right* one (an
//! ADR judges that), and the wording of any entry that carries both halves.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

/// `docs/SPEC.md`, read at compile time so a missing or renamed file is a build
/// error rather than a guard that passes on nothing.
const SPEC: &str = include_str!("../../../docs/SPEC.md");
/// `GLOSSARY.md`, which cites an appendix entry from the vocabulary.
const GLOSSARY: &str = include_str!("../../../GLOSSARY.md");
/// The clause that closes an entry. An entry carrying no such clause is open.
const RESOLVED: &str = "**Resolved:**";
/// How many entries the appendix held when this guard was written: a smaller
/// count means the parse below found less than the appendix holds, or an entry
/// was dropped, and either way this guard must not pass on an empty list.
const ENTRIES: usize = 17;

/// One Appendix A entry, cut into the parts these tests read: the label it is
/// cited by, the question it states, and the clause that closes it.
struct Entry<'a> {
    /// The label the entry is cited by, as in `A14`.
    label: String,
    /// The question the entry states: everything its bold headline trails, cut
    /// at the resolution clause. Empty when the entry states no question at all.
    question: &'a str,
    /// The resolution clause's body, or nothing when the entry is still open.
    resolution: Option<&'a str>,
}

/// The repository root, as the crate's manifest path reaches it.
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The label an entry's headline opens with, when the line opens one: a bold
/// `A` and a number, an em dash, then the gap's name.
fn entry_label(line: &str) -> Option<String> {
    let rest = line.strip_prefix("**A")?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() || !rest[digits.len()..].starts_with(" — ") {
        return None;
    }
    Some(format!("A{digits}"))
}

/// Appendix A's entries, in the order they are written: each runs from its own
/// headline to the next one, and is cut at the first `**Resolved:**` it holds.
fn entries(spec: &str) -> Vec<Entry<'_>> {
    let appendix = match spec.find("## Appendix A") {
        Some(at) => &spec[at..],
        None => panic!("docs/SPEC.md holds no Appendix A"),
    };
    let mut starts = Vec::new();
    let mut offset = 0;
    for line in appendix.split_inclusive('\n') {
        if let Some(label) = entry_label(line) {
            starts.push((offset, label));
        }
        offset += line.len();
    }
    starts
        .iter()
        .enumerate()
        .map(|(index, (start, label))| {
            let end = starts
                .get(index + 1)
                .map_or(appendix.len(), |(next, _)| *next);
            let body = &appendix[*start..end];
            let (asked, resolution) = match body.find(RESOLVED) {
                Some(at) => (&body[..at], Some(&body[at + RESOLVED.len()..])),
                None => (body, None),
            };
            // The bold label is the heading proper; the question is what it trails.
            let question = asked
                .strip_prefix("**")
                .and_then(|rest| rest.find("**").map(|close| &rest[close + 2..]))
                .unwrap_or(asked);
            Entry {
                label: label.clone(),
                question,
                resolution,
            }
        })
        .collect()
}

/// Every document that is allowed to cite an entry, a section or a decision:
/// the spec, the glossary, and whatever ADRs the directory holds, which are
/// listed at run time so a new record is covered without touching this file.
fn documentation() -> Vec<(String, String)> {
    let mut docs = vec![
        (String::from("docs/SPEC.md"), String::from(SPEC)),
        (String::from("GLOSSARY.md"), String::from(GLOSSARY)),
    ];
    let directory = repo_root().join("docs/adr");
    let listing = fs::read_dir(&directory)
        .unwrap_or_else(|error| panic!("cannot list {}: {error}", directory.display()));
    let mut records: Vec<PathBuf> = listing
        .map(|entry| entry.expect("a readable docs/adr entry").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "md"))
        .collect();
    records.sort();
    assert!(
        !records.is_empty(),
        "{} holds no ADR to check references against",
        directory.display()
    );
    for path in records {
        let name = format!(
            "docs/adr/{}",
            path.file_name().expect("a named ADR").to_string_lossy()
        );
        let body = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
        docs.push((name, body));
    }
    docs
}

/// The numbers of the ADRs on disk, as the file-name stems that open with them.
fn adr_stems() -> BTreeSet<String> {
    documentation()
        .into_iter()
        .filter_map(|(name, _)| name.strip_prefix("docs/adr/").map(str::to_owned))
        .collect()
}

/// The sections `docs/SPEC.md` actually holds, from its numbered headings.
fn section_numbers(spec: &str) -> BTreeSet<String> {
    spec.lines()
        .filter_map(|line| {
            let rest = line.strip_prefix("## ")?;
            let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
            let closes = rest[digits.len()..].starts_with(". ");
            (!digits.is_empty() && closes).then_some(digits)
        })
        .collect()
}

/// Every `§N` the text makes, as the `N` it names.
fn section_references(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut found = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        let mut end = index + 1;
        if chars[index] == '§' {
            while end < chars.len() && chars[end].is_ascii_digit() {
                end += 1;
            }
            if end > index + 1 {
                found.push(chars[index + 1..end].iter().collect());
            }
        }
        index = end.max(index + 1);
    }
    found
}

/// Every `A<n>` the text makes, as the label it names: an `A` that opens a word,
/// one to three digits, and no letter or digit trailing them, so a longer word
/// that happens to hold an `A` names nothing.
fn entry_references(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut found = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        let mut end = index + 1;
        if chars[index] == 'A' {
            while end < chars.len() && chars[end].is_ascii_digit() && end - index <= 3 {
                end += 1;
            }
            let opens_word = index == 0 || !chars[index - 1].is_alphanumeric();
            let closed = end == chars.len() || !chars[end].is_alphanumeric();
            if opens_word && end > index + 1 && closed {
                found.push(chars[index..end].iter().collect());
            }
        }
        index = end.max(index + 1);
    }
    found
}

/// Every `ADR-<digits>` the text makes, as the digits it names.
fn adr_references(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut found = Vec::new();
    let mut index = 0;
    while index + 4 < chars.len() {
        if chars[index..index + 4].iter().collect::<String>() == "ADR-" {
            let mut end = index + 4;
            while end < chars.len() && chars[end].is_ascii_digit() {
                end += 1;
            }
            if end > index + 4 {
                found.push(chars[index + 4..end].iter().collect());
            }
            index = end;
            continue;
        }
        index += 1;
    }
    found
}

/// The last few words of a question, so a failure names what it read.
fn tail(text: &str) -> String {
    let trimmed = text.trim_end();
    let start = trimmed.char_indices().rev().nth(59).map_or(0, |(at, _)| at);
    format!("…{}", &trimmed[start..])
}

#[test]
fn every_appendix_entry_carries_a_resolution() {
    let all = entries(SPEC);
    assert!(
        all.len() >= ENTRIES,
        "the appendix is parsed as {} entries, and held at least {ENTRIES}",
        all.len()
    );
    let open: Vec<&str> = all
        .iter()
        .filter(|entry| entry.resolution.is_none())
        .map(|entry| entry.label.as_str())
        .collect();
    assert!(
        open.is_empty(),
        "entries with no `**Resolved:**` clause, so nothing says where their answer lives: {}",
        open.join(", ")
    );
}

#[test]
fn every_appendix_entry_states_a_question_before_its_resolution() {
    let mut complaints = Vec::new();
    for entry in entries(SPEC) {
        // An entry with no clause at all is the test above's failure, not this one.
        if entry.resolution.is_none() {
            continue;
        }
        let asked = entry.question.trim();
        if asked.is_empty() {
            complaints.push(format!("{}: states no question", entry.label));
            continue;
        }
        let ends_as_a_sentence =
            matches!(asked.chars().next_back(), Some('.') | Some('?') | Some('!'));
        if !ends_as_a_sentence {
            complaints.push(format!(
                "{}: the question runs into its resolution instead of ending: {}",
                entry.label,
                tail(asked)
            ));
        }
    }
    assert!(
        complaints.is_empty(),
        "entries whose question is cut short:\n{}",
        complaints.join("\n")
    );
}

#[test]
fn every_entry_reference_names_a_real_entry() {
    let labels: BTreeSet<String> = entries(SPEC).into_iter().map(|entry| entry.label).collect();
    let mut complaints = Vec::new();
    for (name, text) in documentation() {
        for label in entry_references(&text) {
            if !labels.contains(&label) {
                complaints.push(format!("{name}: {label} names no Appendix A entry"));
            }
        }
    }
    assert!(
        complaints.is_empty(),
        "references that name nothing:\n{}",
        complaints.join("\n")
    );
}

#[test]
fn every_section_and_decision_reference_names_a_real_target() {
    let sections = section_numbers(SPEC);
    assert!(
        sections.len() >= 17,
        "docs/SPEC.md is parsed as {} sections, and holds at least 17",
        sections.len()
    );
    let adrs = adr_stems();
    let mut complaints = Vec::new();
    for (name, text) in documentation() {
        for section in section_references(&text) {
            if !sections.contains(&section) {
                complaints.push(format!(
                    "{name}: §{section} names no section of docs/SPEC.md"
                ));
            }
        }
        for adr in adr_references(&text) {
            let named = format!("{adr}-");
            if !adrs.iter().any(|stem| stem.starts_with(&named)) {
                complaints.push(format!("{name}: ADR-{adr} names no file in docs/adr/"));
            }
        }
    }
    assert!(
        complaints.is_empty(),
        "references that name nothing:\n{}",
        complaints.join("\n")
    );
}
