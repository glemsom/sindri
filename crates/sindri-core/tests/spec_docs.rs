//! Pins the rule that every cross-reference the documentation makes names something the
//! tree really holds: a `§N` is a numbered section of `docs/SPEC.md` and an `ADR-NNNN` is
//! a file in `docs/adr/`, so a section renumbered or a record retired cannot leave a
//! reader chasing a name that is gone.
//!
//! The files are read at run time rather than pasted in at build time, because the spec is
//! deliberately not an input a compiler watches: the no-relink proof touches it precisely
//! to show that the binary does not move.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// One document that may cite a section or a decision: the name a failure quotes, and the
/// text it was read from.
struct Doc {
    /// The path the document is read from, as a failure names it.
    name: String,
    /// What the document says.
    body: String,
}

/// The repository root, as the crate's manifest path reaches it.
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// One document read whole, so the references it makes can be checked.
fn read(root: &Path, name: &str) -> Doc {
    let path = root.join(name);
    let body = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
    assert!(
        !body.trim().is_empty(),
        "{} says nothing, so its references cannot be checked",
        path.display()
    );
    Doc {
        name: String::from(name),
        body,
    }
}

/// The ADR files the directory holds, as the names a reader asks for them by: listed at
/// run time so a new record is covered without touching this file.
fn adr_files(root: &Path) -> Vec<String> {
    let directory = root.join("docs/adr");
    let listing = fs::read_dir(&directory)
        .unwrap_or_else(|error| panic!("cannot list {}: {error}", directory.display()));
    let mut names: Vec<String> = listing
        .map(|entry| entry.expect("a readable docs/adr entry").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "md"))
        .map(|path| {
            format!(
                "docs/adr/{}",
                path.file_name().expect("a named ADR").to_string_lossy()
            )
        })
        .collect();
    names.sort();
    assert!(
        !names.is_empty(),
        "{} holds no ADR to check references against",
        directory.display()
    );
    names
}

/// Every document allowed to cite a section or a decision: the spec, the glossary, and
/// every ADR.
fn documentation() -> Vec<Doc> {
    let root = repo_root();
    let mut docs = vec![read(&root, "docs/SPEC.md"), read(&root, "GLOSSARY.md")];
    for name in adr_files(&root) {
        docs.push(read(&root, &name));
    }
    docs
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

/// Every run of digits the text opens with `marker`, as the digits it names.
fn marked_numbers(text: &str, marker: &str) -> Vec<String> {
    text.split(marker)
        .skip(1)
        .filter_map(|rest| {
            let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
            (!digits.is_empty()).then_some(digits)
        })
        .collect()
}

#[test]
fn every_section_reference_names_a_section_the_spec_holds() {
    let sections = section_numbers(&documentation()[0].body);
    assert!(
        sections.len() > 1,
        "docs/SPEC.md is parsed as {} sections, so the parse found nothing to check against",
        sections.len()
    );
    let mut complaints = Vec::new();
    for doc in documentation() {
        for section in marked_numbers(&doc.body, "§") {
            if !sections.contains(&section) {
                complaints.push(format!(
                    "{}: §{section} names no section of docs/SPEC.md",
                    doc.name
                ));
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
fn every_decision_reference_names_a_record_in_docs_adr() {
    let adrs: BTreeSet<String> = adr_files(&repo_root())
        .into_iter()
        .filter_map(|name| name.strip_prefix("docs/adr/").map(str::to_owned))
        .collect();
    let mut complaints = Vec::new();
    for doc in documentation() {
        for adr in marked_numbers(&doc.body, "ADR-") {
            let named = format!("{adr}-");
            if !adrs.iter().any(|name| name.starts_with(&named)) {
                complaints.push(format!(
                    "{}: ADR-{adr} names no file in docs/adr/",
                    doc.name
                ));
            }
        }
    }
    assert!(
        complaints.is_empty(),
        "references that name nothing:\n{}",
        complaints.join("\n")
    );
}
