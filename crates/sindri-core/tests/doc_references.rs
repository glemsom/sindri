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

impl Adr {
    /// The record a file name claims to be, read as it is rather than judged: a name that breaks
    /// the convention still yields the number and slug it opens with, so the complaint can say why.
    fn named(file: &str) -> Adr {
        let stem = file.strip_suffix(".md").unwrap_or(file);
        let (number, slug) = stem.split_once('-').unwrap_or((stem, ""));
        Adr {
            file: String::from(file),
            number: String::from(number),
            slug: String::from(slug),
        }
    }
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
/// touching this file. Anything but a hidden file is a record, so a stray name is judged rather
/// than passed over.
fn adrs(root: &Path) -> Vec<Adr> {
    let directory = root.join("docs/adr");
    let listing = fs::read_dir(&directory)
        .unwrap_or_else(|error| panic!("cannot list {}: {error}", directory.display()));
    let mut adrs: Vec<Adr> = listing
        .map(|entry| entry.expect("a readable docs/adr entry").path())
        .filter(|path| path.is_file())
        .map(|path| {
            path.file_name()
                .expect("a named ADR")
                .to_string_lossy()
                .into_owned()
        })
        .filter(|file| !file.starts_with('.'))
        .map(|file| Adr::named(&file))
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

/// Every citation the documents make that the numbers the tree holds do not allow: a number with
/// no file and no `(retired)` marker, or a live number wearing one.
fn citation_complaints(held: &BTreeSet<String>, documents: &[Document]) -> Vec<String> {
    let mut complaints = Vec::new();
    for document in documents {
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
    complaints
}

#[test]
fn every_cited_adr_is_a_file_or_marked_retired() {
    let held: BTreeSet<String> = adrs(&repo_root())
        .into_iter()
        .map(|adr| adr.number)
        .collect();
    let complaints = citation_complaints(&held, &citing_documents());
    assert!(
        complaints.is_empty(),
        "references that name nothing:\n{}",
        complaints.join("\n")
    );
}

/// Every record whose name the convention `NNNN-<lowercase-slug>.md` rejects, and why.
fn adr_name_complaints(adrs: &[Adr]) -> Vec<String> {
    let mut complaints = Vec::new();
    for adr in adrs {
        if !adr.file.ends_with(".md") {
            complaints.push(format!("docs/adr/{}: is not a `.md` record", adr.file));
            continue;
        }
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
    complaints
}

#[test]
fn every_adr_is_a_number_and_a_slug() {
    let complaints = adr_name_complaints(&adrs(&repo_root()));
    assert!(
        complaints.is_empty(),
        "ADRs named against the convention:\n{}",
        complaints.join("\n")
    );
}

#[test]
fn a_citation_of_a_record_the_tree_does_not_hold_is_a_complaint() {
    let held = BTreeSet::from([String::from("0018")]);
    let documents = [Document {
        name: String::from("GLOSSARY.md"),
        body: String::from("the word was settled by ADR-0002, whose record is retired"),
    }];

    let complaints = citation_complaints(&held, &documents);

    assert_eq!(complaints.len(), 1, "{complaints:?}");
    assert!(complaints[0].contains("GLOSSARY.md"), "{complaints:?}");
    assert!(complaints[0].contains("ADR-0002"), "{complaints:?}");
}

#[test]
fn a_live_record_cited_as_retired_is_a_complaint() {
    let held = BTreeSet::from([String::from("0018")]);
    let documents = [Document {
        name: String::from("AGENTS.md"),
        body: String::from("the comments rule is ADR-0018 (retired)"),
    }];

    let complaints = citation_complaints(&held, &documents);

    assert_eq!(complaints.len(), 1, "{complaints:?}");
    assert!(complaints[0].contains("AGENTS.md"), "{complaints:?}");
    assert!(complaints[0].contains("ADR-0018"), "{complaints:?}");
}

#[test]
fn a_record_named_against_the_convention_is_a_complaint() {
    let records = [
        Adr::named("18-a-slug.md"),
        Adr::named("0018-Not_A_Slug.md"),
        Adr::named("0018-a-slug.txt"),
    ];

    let complaints = adr_name_complaints(&records);

    assert_eq!(complaints.len(), 3, "{complaints:?}");
    for record in &records {
        assert!(
            complaints
                .iter()
                .any(|complaint| complaint.contains(&record.file)),
            "{} earns no complaint of its own: {complaints:?}",
            record.file
        );
    }
}
