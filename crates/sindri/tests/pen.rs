//! The pen is the one module that writes a glyph into a cell, measures a column, fills a row to its
//! edge and hangs a run on one. A copy is what a habit leaves behind, so each of those is a search
//! of the tree with a test behind it rather than a convention nobody rereads.

use std::fs;
use std::path::{Path, PathBuf};

struct Source {
    name: String,
    body: String,
}

impl Source {
    fn names(&self, needle: &str) -> bool {
        self.body.contains(needle)
    }

    /// The file and the line of every `needle` in it, as a search would report them, so a failure
    /// says where the copy is and not only that one exists.
    fn found(&self, needle: &str) -> Vec<String> {
        self.body
            .lines()
            .enumerate()
            .filter(|(_, line)| line.contains(needle))
            .map(|(at, _)| format!("{}:{}", self.name, at + 1))
            .collect()
    }
}

fn front_end() -> Vec<Source> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut sources = Vec::new();
    walk(&root, &mut sources);
    assert!(
        sources.len() > 20,
        "the front end is {} modules, so the walk reaches the tree",
        sources.len()
    );
    sources
}

fn walk(directory: &Path, sources: &mut Vec<Source>) {
    for entry in fs::read_dir(directory).expect("the front end's own directory") {
        let path = entry.expect("a directory entry").path();
        if path.is_dir() {
            walk(&path, sources);
            continue;
        }
        if path.extension().is_none_or(|extension| extension != "rs") {
            continue;
        }
        let name = path
            .strip_prefix(Path::new(env!("CARGO_MANIFEST_DIR")))
            .expect("a path inside the crate")
            .display()
            .to_string();
        let body = fs::read_to_string(&path).expect("a module of the front end");
        sources.push(Source { name, body });
    }
}

/// Every place `needle` is written, across the tree, in one list.
fn everywhere(sources: &[Source], needle: &str) -> Vec<String> {
    sources
        .iter()
        .flat_map(|source| source.found(needle))
        .collect()
}

/// The one module a search may find, which is the pen.
const PEN: &str = "src/pen.rs";

#[test]
fn the_pen_is_the_only_module_that_writes_a_glyph_into_a_cell() {
    let sources = front_end();
    for writing in ["cell_mut(", "set_stringn(", "set_char("] {
        let found = everywhere(&sources, writing);
        assert!(!found.is_empty(), "the pen writes a glyph with {writing}");
        assert!(
            found.iter().all(|at| at.starts_with(PEN)),
            "one module writes a glyph into a cell, and it is the pen: {writing} is written in \
             {found:?}"
        );
    }
}

#[test]
fn no_module_but_the_pen_answers_a_column_by_counting_glyphs() {
    let sources = front_end();

    let counted = everywhere(&sources, "chars().count()");
    assert_eq!(
        counted,
        Vec::<String>::new(),
        "a column is measured by the cells a run draws in, never by the glyphs it is made of, and \
         the pen is the one module that measures"
    );

    let padded: Vec<String> = sources
        .iter()
        .flat_map(|source| {
            ["{:<", "{:>", "{:^"]
                .into_iter()
                .flat_map(|width| source.found(width))
                .collect::<Vec<String>>()
        })
        .collect();
    assert_eq!(
        padded,
        Vec::<String>::new(),
        "a format width counts a run's glyphs, so a run is padded out to a column by the pen"
    );
}

#[test]
fn the_pen_fills_a_row_to_its_edge_and_hangs_a_run_on_one_and_no_other_module_does() {
    let sources = front_end();

    let fills = everywhere(&sources, "repeat(");
    assert!(
        fills.iter().all(|at| at.starts_with(PEN)),
        "a row that fills to its edge is the pen's, and repeating a glyph to spell one is not a \
         second way to fill it: {fills:?}"
    );

    for (signature, written) in [
        ("fn rule(", "the row that fills to its edge"),
        ("fn hang<", "the run that hangs on one"),
        ("fn hanging(", "the column it hangs from"),
    ] {
        let defining: Vec<&str> = sources
            .iter()
            .filter(|source| source.names(signature))
            .map(|source| source.name.as_str())
            .collect();
        assert_eq!(
            defining,
            vec![PEN],
            "{written} is written once, in the pen, and every caller asks it"
        );
    }
}

#[test]
fn no_caller_names_a_layout_or_a_spelling_of_the_rail() {
    let sources = front_end();

    let named = everywhere(&sources, "rail::");
    assert_eq!(
        named.len(),
        1,
        "the rail is asked to paint and named for nothing else: {named:?}"
    );
    assert!(
        named[0].starts_with("src/view.rs")
            && sources
                .iter()
                .any(|source| { source.name == "src/view.rs" && source.names("rail::paint") }),
        "and the one caller is the frame's own paint pass: {named:?}"
    );
}
