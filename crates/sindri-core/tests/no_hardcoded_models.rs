//! Proves no model list is remembered anywhere in the tree, by reading every
//! Rust source and looking for the one shape a remembered list has: several
//! model identifiers written out as string literals, either as the elements of
//! a bracketed collection or joined inside a single literal.
//!
//! Reading rather than trusting a convention is the same trick the rig
//! confinement test uses, and for the same reason: a `const MODELS: &[&str]`
//! is one convenient line away at all times, and no reviewer should have to
//! catch it. §10 says served names are read off the wire and never invented, so
//! a list of them in source is the invention §10 forbids.
//!
//! What an identifier looks like is decided by shape and not by vendor: a word
//! that starts with a lower-case letter, runs to four characters or more of
//! letters, digits, dots, dashes, slashes, colons and underscores, and carries
//! at least one digit. A vendor's own names are all of that shape, and so is a
//! list someone wrote by hand.
//!
//! Comments are dropped before the scan, so a module may name a model in prose
//! — a doc example, a hint, an explanation — while holding no list of them. The
//! scan is deliberately blunt in three ways: it reads `[` as the only
//! collection (a hardcoded list in a tuple is out of scope), it wants two
//! identifiers before it calls one a list (a single default model is config's
//! business, §11, not a list), and it does not resolve `include_str!` (a
//! catalogue belongs in a recorded fixture, where the scan does not look).

use std::fs;
use std::path::{Path, PathBuf};

/// A bracketed collection being read: where it opened, how many identifiers it
/// holds, and whether anything in it is neither a literal nor the glue that
/// wraps one.
struct Collection {
    /// The line the `[` is on, which is what a violation reports.
    line: usize,
    /// How many of its own elements are model identifiers.
    identifiers: usize,
    /// Whether every element is a literal or glue, and so the collection is a
    /// list of names rather than a list of things that hold one.
    literals_only: bool,
}

/// The words that wrap a literal without making it something else: a list
/// spelled `["a".to_owned(), "b".into()]` or `[String::from("a"), …]` is still a
/// list written out by hand.
const GLUE: [&str; 6] = ["String", "from", "into", "to_owned", "to_string", "vec"];

/// Whether `word` wraps a literal rather than being one of the collection's
/// own elements.
fn is_glue(word: &str) -> bool {
    GLUE.contains(&word)
}

/// Whether `token` is the shape a served model name has, decided by shape and
/// never by a vendor's prefix: a lower-case word that starts with a letter,
/// runs to four characters or more, and carries a digit. A pair of these is a
/// list of models; a number with a unit (`120s`, `3600000ms`) opens with a
/// digit and cannot be mistaken for one, and neither can a path.
fn is_model_shaped(token: &str) -> bool {
    token.len() >= 4
        && token.starts_with(|character: char| character.is_ascii_lowercase())
        && token.chars().any(|character| character.is_ascii_digit())
        && token.chars().all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || matches!(character, '-' | '.' | '/' | ':' | '_')
        })
}

/// Every model identifier `text` holds, in the order it holds them: one ends
/// where a character no identifier carries begins, so a list reads the same
/// whether it was spelled as separate literals, as JSON, or as one joined
/// string.
fn named(text: &str) -> Vec<&str> {
    text.split(|character: char| {
        !(character.is_ascii_lowercase()
            || character.is_ascii_digit()
            || matches!(character, '-' | '.' | '/' | ':' | '_'))
    })
    .filter(|token| is_model_shaped(token))
    .collect()
}

/// How many model identifiers `text` holds.
fn identifiers(text: &str) -> usize {
    named(text).len()
}

/// The offset of the `"` that opens a raw string at `index`, when `index` is
/// on the `r` of one: `r"`, `r#"`, `r##"` and their byte-string kin.
fn raw_quote(bytes: &[u8], index: usize) -> Option<usize> {
    if bytes.get(index) != Some(&b'r') {
        return None;
    }
    let quote = index
        + 1
        + bytes[index + 1..]
            .iter()
            .take_while(|byte| **byte == b'#')
            .count();
    (bytes.get(quote) == Some(&b'"')).then_some(quote)
}

/// How many bytes the `'` at `index` opens: one for a lifetime, and the whole
/// literal for a character, so a `'"'` cannot open a string.
fn character_width(bytes: &[u8], index: usize) -> usize {
    match bytes.get(index + 1) {
        Some(b'\\') => bytes
            .get(index + 2..)
            .unwrap_or_default()
            .iter()
            .position(|byte| *byte == b'\'')
            .map_or(1, |found| found + 3),
        Some(_) if bytes.get(index + 2) == Some(&b'\'') => 3,
        _ => 1,
    }
}

/// `source` with its comments and character literals replaced by spaces: the
/// same bytes at the same offsets, so a line number found here is the line
/// number in the file. A string literal is copied whole, because a `//` inside
/// one is a URL and not a comment, and prose in a comment may name whatever it
/// likes.
fn masked(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut out = vec![b' '; bytes.len()];
    let mut index = 0;
    while index < bytes.len() {
        let rest = &bytes[index..];
        if rest.starts_with(b"//") {
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
        } else if rest.starts_with(b"/*") {
            index += 2;
            let mut depth = 1;
            while index < bytes.len() && depth > 0 {
                if bytes[index..].starts_with(b"/*") {
                    depth += 1;
                    index += 2;
                } else if bytes[index..].starts_with(b"*/") {
                    depth -= 1;
                    index += 2;
                } else {
                    index += 1;
                }
            }
        } else if let Some(quote) = raw_quote(bytes, index) {
            let (_, end) = literal(source, quote);
            out[index..end].copy_from_slice(&bytes[index..end]);
            index = end;
        } else if bytes[index] == b'"' {
            let (_, end) = literal(source, index);
            out[index..end].copy_from_slice(&bytes[index..end]);
            index = end;
        } else if bytes[index] == b'\'' {
            index += character_width(bytes, index);
        } else {
            out[index] = bytes[index];
            index += 1;
        }
    }
    String::from_utf8(out).expect("masking keeps the text valid")
}

/// The line `offset` falls on, counted from one.
fn line_at(source: &str, offset: usize) -> usize {
    source[..offset].matches('\n').count() + 1
}

/// The body of the string literal opening at `open`, and the offset just past
/// its closing quote. Raw strings are read as raw, so a body holding quotes
/// cannot desynchronise the scan of everything after it.
fn literal(source: &str, open: usize) -> (usize, usize) {
    let bytes = source.as_bytes();
    let hashes = bytes[..open]
        .iter()
        .rev()
        .take_while(|byte| **byte == b'#')
        .count();
    if hashes > 0 {
        let terminator = format!("\"{}", "#".repeat(hashes));
        let body = open + 1;
        let end = source[body..]
            .find(&terminator)
            .map_or(bytes.len(), |found| body + found);
        return (body, (end + terminator.len()).min(bytes.len()));
    }
    let mut index = open + 1;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => index += 2,
            b'"' => return (open + 1, index + 1),
            _ => index += 1,
        }
    }
    (open + 1, bytes.len())
}

/// The lines of `source` that hold a model list written out by hand: a bracket
/// holding two or more string literals of that shape, or one literal spelling
/// two identifiers.
fn remembered(source: &str) -> Vec<usize> {
    let text = masked(source);
    let bytes = text.as_bytes();
    let mut found = Vec::new();
    let mut open: Vec<Collection> = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'"' => {
                let (start, end) = literal(&text, index);
                let body = &text[start..end.saturating_sub(1).max(start)];
                if identifiers(body) >= 2 {
                    found.push(line_at(&text, index));
                }
                if let Some(collection) = open.last_mut()
                    && is_model_shaped(body)
                {
                    collection.identifiers += 1;
                }
                index = end;
            }
            b'[' => {
                open.push(Collection {
                    line: line_at(&text, index),
                    identifiers: 0,
                    literals_only: true,
                });
                index += 1;
            }
            b']' => {
                if let Some(collection) = open.pop()
                    && collection.literals_only
                    && collection.identifiers >= 2
                {
                    found.push(collection.line);
                }
                index += 1;
            }
            byte if byte.is_ascii_whitespace() || b".,()&:".contains(&byte) => index += 1,
            byte if byte.is_ascii_alphanumeric() || byte == b'_' => {
                let start = index;
                while index < bytes.len()
                    && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
                {
                    index += 1;
                }
                if !is_glue(&text[start..index]) {
                    for collection in &mut open {
                        collection.literals_only = false;
                    }
                }
            }
            _ => {
                for collection in &mut open {
                    collection.literals_only = false;
                }
                index += 1;
            }
        }
    }
    found.sort_unstable();
    found.dedup();
    found
}

/// Every Rust source of the workspace, skipping what a build leaves behind and
/// every dot directory, so a worktree nested in the repository is not read
/// twice.
fn sources(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let entries = fs::read_dir(&directory).expect("the tree is readable");
        for entry in entries {
            let path = entry.expect("a directory entry").path();
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if path.is_dir() {
                if !name.starts_with('.') && name != "target" {
                    pending.push(path);
                }
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

/// The workspace root: the crate this test belongs to, two directories up.
fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("the workspace root is reachable")
}

#[test]
fn no_source_file_holds_a_list_of_model_names() {
    let root = workspace();
    let files = sources(&root);
    assert!(
        files.len() > 20,
        "the scan read {} sources, which is too few to be the tree",
        files.len()
    );

    let mut offenders = Vec::new();
    for path in &files {
        let source = fs::read_to_string(path).expect("a source file is readable");
        for line in remembered(&source) {
            offenders.push(format!(
                "{}:{line}: a list of model names written out by hand",
                path.strip_prefix(&root).unwrap_or(path).display()
            ));
        }
    }

    assert!(
        offenders.is_empty(),
        "§10 says served names are read off the wire and never invented, but:\n  {}",
        offenders.join("\n  ")
    );
}

#[test]
fn the_scan_catches_a_model_list_planted_as_literals() {
    let one = "claude-sonnet-4-6";
    let other = "claude-haiku-4-5";

    let collection = format!("const MODELS: [&str; 2] = [\"{one}\", \"{other}\"];");
    let joined = format!("const MODELS: &str = \"{one} {other}\";");
    let owned = format!("vec![\"{one}\".to_owned(), \"{other}\".into()]");
    let built = format!("vec![String::from(\"{one}\"), String::from(\"{other}\")]");

    assert_eq!(
        remembered(&collection),
        [1],
        "a bracketed pair of identifiers is the shape a remembered list has"
    );
    assert_eq!(
        remembered(&joined),
        [1],
        "and so is two of them spelled inside one literal"
    );
    assert_eq!(
        remembered(&owned),
        [1],
        "wrapping a literal in a conversion does not make it less of a list"
    );
    assert_eq!(remembered(&built), [1], "and neither does building one");
}

#[test]
fn the_scan_leaves_the_honest_spellings_alone() {
    let (first, second, third) = ("anthropic", "openai", "copilot");
    let words = format!("const WORDS: [&str; 3] = [\"{first}\", \"{second}\", \"{third}\"];");
    let one_name = format!("let ask = Ask::new(Wire::Anthropic, \"{first}-5\");");
    let read = String::from("let names = client.served(&at, &key, &at);");

    assert!(
        remembered(&words).is_empty(),
        "the wire words are not model identifiers"
    );
    assert!(
        remembered(&one_name).is_empty(),
        "one model named by a caller is how a run without a list carries on"
    );
    assert!(
        remembered(&read).is_empty(),
        "and reading them off the wire is the whole point"
    );
    let named_once = format!("let pairs = [{first:?}, {second:?}].join(\" \");");
    assert!(
        remembered(&named_once).is_empty(),
        "constants joined at the call site hold no literal list"
    );
}

#[test]
fn the_shape_reads_the_recorded_catalogues_as_model_names() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/providers");
    for catalogue in ["anthropic_models.json", "openai_models.json"] {
        let body = fs::read_to_string(fixtures.join(catalogue)).expect("a fixture is readable");
        assert!(
            identifiers(&body) >= 2,
            "{catalogue} records the identifiers a wire served, so the shape \
             must recognise at least two in it: {}",
            identifiers(&body)
        );
    }
}

#[test]
fn the_listing_code_names_no_model_of_its_own() {
    let served = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/provider/served.rs");
    let source = fs::read_to_string(&served).expect("the listing code is readable");
    let text = masked(&source);
    let held = named(&text);

    assert!(
        held.is_empty(),
        "the module that reads served names must hold none of its own, but it \
         names {held:?}"
    );
}
