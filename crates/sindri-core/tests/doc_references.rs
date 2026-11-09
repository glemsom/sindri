//! Pins the two rules an ADR keeps: its file is named `NNNN-<lowercase-slug>.md`, and a document
//! citing `ADR-NNNN` either names a record the tree holds or marks the number `(retired)`. Files
//! are read at run time, because documentation is not an input the compiler watches.

use std::fs;
use std::path::{Path, PathBuf};

struct Adr {
    file: String,
    number: String,
    slug: String,
}

impl Adr {
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

struct Document {
    name: String,
    body: String,
}

struct Citation {
    number: String,
    retired: bool,
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

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

/// The ADRs the directory holds, listed at run time so a new one is covered without touching this
/// file. Anything but a hidden file is a record, so a stray name is judged rather than passed over.
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

fn citing_documents(root: &Path, records: &[Adr]) -> Vec<Document> {
    let mut documents = vec![read(root, "AGENTS.md"), read(root, "GLOSSARY.md")];
    for adr in records {
        documents.push(read(root, &format!("docs/adr/{}", adr.file)));
    }
    documents
}

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

fn citation_complaints(records: &[Adr], documents: &[Document]) -> Vec<String> {
    let mut complaints = Vec::new();
    for document in documents {
        for citation in cited_adrs(&document.body) {
            let number = citation.number;
            let held = records.iter().any(|adr| adr.number == number);
            match (held, citation.retired) {
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

fn citation_complaints_in(root: &Path) -> Vec<String> {
    let records = adrs(root);
    citation_complaints(&records, &citing_documents(root, &records))
}

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
        let slug_is_words = !adr.slug.is_empty()
            && adr.slug.split('-').all(|word| {
                !word.is_empty()
                    && word
                        .chars()
                        .all(|glyph| glyph.is_ascii_lowercase() || glyph.is_ascii_digit())
            });
        if !slug_is_words {
            complaints.push(format!(
                "docs/adr/{}: carries no slug of lowercase words",
                adr.file
            ));
        }
    }
    complaints
}

fn tree(files: &[(&str, &str)]) -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("a temp tree");
    for (name, body) in files {
        let path = root.path().join(name);
        fs::create_dir_all(path.parent().expect("a named file")).expect("a writable directory");
        fs::write(&path, body).expect("a writable file");
    }
    root
}

#[test]
fn every_cited_adr_is_a_file_or_marked_retired() {
    let complaints = citation_complaints_in(&repo_root());
    assert!(
        complaints.is_empty(),
        "references that name nothing:\n{}",
        complaints.join("\n")
    );
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
    let tree = tree(&[
        ("AGENTS.md", "the comments rule is ADR-0042"),
        ("GLOSSARY.md", "# Sindri"),
        ("docs/adr/0018-a-record.md", "# A record"),
    ]);

    assert_eq!(
        citation_complaints_in(tree.path()),
        [String::from(
            "AGENTS.md: ADR-0042 names no file in docs/adr/ and is not marked retired"
        )]
    );
}

#[test]
fn a_live_record_cited_as_retired_is_a_complaint() {
    let tree = tree(&[
        ("AGENTS.md", "the comments rule is ADR-0018 (retired)"),
        ("GLOSSARY.md", "# Sindri"),
        ("docs/adr/0018-a-record.md", "# A record"),
    ]);

    assert_eq!(
        citation_complaints_in(tree.path()),
        [String::from(
            "AGENTS.md: ADR-0018 (retired) is a file in docs/adr/, so it cannot be cited as retired"
        )]
    );
}

#[test]
fn a_held_record_and_a_retired_number_are_both_allowed() {
    let tree = tree(&[
        ("AGENTS.md", "ADR-0018 settled it, after ADR-0002 (retired)"),
        ("GLOSSARY.md", "# Sindri, and ADR-0018 again"),
        (
            "docs/adr/0018-a-record.md",
            "# A record, superseding ADR-0002 (retired)",
        ),
    ]);

    assert_eq!(citation_complaints_in(tree.path()), Vec::<String>::new());
}

#[test]
fn a_record_named_against_the_convention_is_a_complaint() {
    let records = [
        Adr::named("18-a-slug.md"),
        Adr::named("0018-Not_A_Slug.md"),
        Adr::named("0018-a--slug.md"),
        Adr::named("0018-a-slug.txt"),
    ];

    assert_eq!(
        adr_name_complaints(&records),
        [
            String::from("docs/adr/18-a-slug.md: opens with no four-digit number"),
            String::from("docs/adr/0018-Not_A_Slug.md: carries no slug of lowercase words"),
            String::from("docs/adr/0018-a--slug.md: carries no slug of lowercase words"),
            String::from("docs/adr/0018-a-slug.txt: is not a `.md` record"),
        ]
    );
}
