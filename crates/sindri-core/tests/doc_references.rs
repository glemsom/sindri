//! Pins the rule that every ADR number a document cites names a file in `docs/adr/`, unless
//! the citation marks it retired, so no reader is sent after a name the tree does not hold.
//!
//! The files are read at run time rather than pasted in at build time, because the
//! documentation is deliberately not an input a compiler watches: the no-relink proof
//! touches it precisely to show that the binary does not move.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// One ADR, as the tree holds it: the file it lives in, the number it is known by, and the slug
/// that follows the number.
struct Adr {
    /// The file name inside `docs/adr/`, which is what a failure quotes.
    file: String,
    /// The leading digits of `file`, which is what a document cites it by.
    number: String,
    /// What `file` says after `NNNN-`, without the extension — empty when it says nothing.
    slug: String,
}

/// One document that may cite an ADR: the path a failure quotes, and the text read from it.
struct Document {
    /// The path the document is read from, as a failure names it.
    name: String,
    body: String,
}

/// One ADR citation: the number a document names, and whether the citation marks it retired.
struct Citation {
    number: String,
    retired: bool,
}

/// The repository root, as the crate's manifest path reaches it.
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// One document read whole, so the references it makes can be checked.
fn read(root: &Path, name: &str) -> Document {
    let path = root.join(name);
    let body = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
    assert!(
        !body.trim().is_empty(),
        "{} says nothing, so its references cannot be checked",
        path.display()
    );
    Document {
        name: String::from(name),
        body,
    }
}

/// The ADRs the directory holds, listed at run time so a new one is covered without
/// touching this file.
fn adrs(root: &Path) -> Vec<Adr> {
    let directory = root.join("docs/adr");
    let listing = fs::read_dir(&directory)
        .unwrap_or_else(|error| panic!("cannot list {}: {error}", directory.display()));
    let mut adrs: Vec<Adr> = listing
        .map(|entry| entry.expect("a readable docs/adr entry").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "md"))
        .map(|path| {
            let file = path
                .file_name()
                .expect("a named ADR")
                .to_string_lossy()
                .into_owned();
            let stem = file.strip_suffix(".md").unwrap_or(&file).to_owned();
            let (number, slug) = stem.split_once('-').unwrap_or((stem.as_str(), ""));
            Adr {
                file,
                number: String::from(number),
                slug: String::from(slug),
            }
        })
        .collect();
    adrs.sort_by(|left, right| left.file.cmp(&right.file));
    assert!(
        !adrs.is_empty(),
        "{} holds no ADR to check references against",
        directory.display()
    );
    adrs
}

/// Every document allowed to cite an ADR: the agents' own file, the glossary, and every ADR.
fn citing_documents() -> Vec<Document> {
    let root = repo_root();
    let mut documents = vec![read(&root, "AGENTS.md"), read(&root, "GLOSSARY.md")];
    for adr in adrs(&root) {
        documents.push(read(&root, &format!("docs/adr/{}", adr.file)));
    }
    documents
}

/// Every ADR the text cites, with the one form a document may name an ADR the tree no longer
/// holds in — `(retired)` after the number.
fn cited_adrs(text: &str) -> Vec<Citation> {
    let mut cited = Vec::new();
    let mut rest = text;
    while let Some((_, after)) = rest.split_once("ADR-") {
        let digits: String = after.chars().take_while(char::is_ascii_digit).collect();
        if !digits.is_empty() {
            let retired = after[digits.len()..].trim_start().starts_with("(retired)");
            cited.push(Citation {
                number: digits,
                retired,
            });
        }
        rest = after;
    }
    cited
}

#[test]
fn every_cited_adr_is_a_file_or_marked_retired() {
    let held: BTreeSet<String> = adrs(&repo_root())
        .into_iter()
        .map(|adr| adr.number)
        .collect();
    let mut complaints = Vec::new();
    for document in citing_documents() {
        for citation in cited_adrs(&document.body) {
            let number = citation.number;
            match (held.contains(&number), citation.retired) {
                (true, false) | (false, true) => {}
                (true, true) => complaints.push(format!(
                    "{}: ADR-{number} (retired) is a file in docs/adr/, so it cannot be cited as retired",
                    document.name
                )),
                (false, false) => complaints.push(format!(
                    "{}: ADR-{number} names no file in docs/adr/ and is not marked retired",
                    document.name
                )),
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
fn every_adr_is_a_number_and_a_slug() {
    let mut complaints = Vec::new();
    for adr in adrs(&repo_root()) {
        if adr.number.len() != 4 || !adr.number.chars().all(|digit| digit.is_ascii_digit()) {
            complaints.push(format!(
                "docs/adr/{}: opens with no four-digit number",
                adr.file
            ));
        }
        if adr.slug.is_empty()
            || !adr
                .slug
                .chars()
                .all(|glyph| glyph.is_ascii_lowercase() || glyph.is_ascii_digit() || glyph == '-')
        {
            complaints.push(format!(
                "docs/adr/{}: carries no slug of lowercase words",
                adr.file
            ));
        }
    }
    assert!(
        complaints.is_empty(),
        "ADRs named against the convention:\n{}",
        complaints.join("\n")
    );
}
