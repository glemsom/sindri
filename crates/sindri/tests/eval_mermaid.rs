//! The mermaid corpus, compared as the SVG the renderer answers with: every
//! diagram type a fence is really written as is drawn in this process and held to
//! the golden checked in beside it (§12, §16, ADR-0003).
//!
//! What is held is the drawing and never the fence: a fence that parses is proved
//! elsewhere, and a corpus of diagrams is here because a renderer's layout is a
//! fact a reader can only approve by reading the diff. Every case fails on the
//! first tag that moved, so a layout change is named rather than shown as two
//! thousand pixels nobody looked at.
//!
//! `SINDRI_BLESS=1` rewrites a golden instead of comparing it, which is the only
//! way one moves: deliberately, in the commit that changed the drawing.
//!
//! An eval is judged by `make evals` and never by `make test`, which is why the
//! test here is ignored: the stage that owns a failure is the stage that prints it.

use std::path::{Path, PathBuf};

use sindri::mermaid::{self, Block, Fence};

/// Where the corpus of real fences lives: one file per fence, and the SVG the renderer
/// answered with checked in beside it, because a rewrite of a renderer's layout is a
/// deliberate change and never one nobody looked at (§16).
const CORPUS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/mermaid");

/// The variable that rewrites a golden instead of comparing it. It is how the corpus was
/// landed and how a deliberate change to it lands later; nothing else moves a golden.
const BLESS: &str = "SINDRI_BLESS";

/// The corpus, in name order, so two runs of the test compare the same fences.
fn corpus() -> Vec<PathBuf> {
    let mut fences: Vec<PathBuf> = std::fs::read_dir(CORPUS)
        .unwrap_or_else(|error| panic!("the corpus at {CORPUS} is readable: {error}"))
        .map(|file| file.expect("a readable directory entry").path())
        .filter(|path| path.extension().is_some_and(|kind| kind == "mmd"))
        .collect();
    fences.sort();
    fences
}

/// `fence.mmd` as a test names it, which is how a failure says which fence moved.
fn named(path: &Path) -> String {
    path.file_stem()
        .expect("a corpus file has a name")
        .to_string_lossy()
        .into_owned()
}

/// The first tag two goldens disagree on, with its number, because a diff nobody can
/// read is a test nobody fixes: an SVG is one long line, so the unit a failure names is
/// the tag.
fn diff(accepted: &str, rendered: &str) -> String {
    let mut ours = accepted.split_inclusive('>');
    let mut theirs = rendered.split_inclusive('>');
    let mut at = 0;
    loop {
        at += 1;
        match (ours.next(), theirs.next()) {
            (Some(left), Some(right)) if left == right => continue,
            (left, right) => {
                return format!(
                    "at tag {at}: accepted {left:?}, rendered {right:?}",
                    left = left.map(str::trim),
                    right = right.map(str::trim)
                );
            }
        }
    }
}

#[test]
#[ignore = "an eval is judged by `make evals`, not by `make test`"]
fn the_corpus_of_real_fences_matches_the_svg_checked_in_beside_it() {
    let fences = corpus();
    assert!(
        fences.len() >= 10,
        "the corpus holds the diagram types a fence is really written as: {} of them",
        fences.len()
    );

    let mut failures = Vec::new();
    for path in &fences {
        let source = std::fs::read_to_string(path)
            .unwrap_or_else(|error| panic!("{} is readable: {error}", path.display()));
        // Rendered again until it draws: `mermaid::BOUND` is a stall a reader should not feel,
        // and a machine running every eval at once can spend it on scheduling rather than on
        // drawing. A fence the renderer really will not read still fails, with its own words.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        let diagram = loop {
            match mermaid::block(&source, Fence::Closed) {
                Block::Picture(diagram) => break diagram,
                refused if std::time::Instant::now() > deadline => panic!(
                    "{} is a fence the renderer draws; a corpus of refusals is a different \
                     test, and this one said {refused:?}",
                    named(path)
                ),
                _ => continue,
            }
        };

        let accepted = path.with_extension("svg");
        match std::fs::read_to_string(&accepted) {
            _ if std::env::var_os(BLESS).is_some() => {
                // A trailing newline and nothing else, so a golden is a text file a reader's
                // editor and `git diff` both treat as one line of SVG.
                let golden = format!("{}\n", diagram.svg());
                std::fs::write(&accepted, golden)
                    .unwrap_or_else(|error| panic!("{} is written: {error}", accepted.display()));
            }
            Ok(golden) if golden.trim_end_matches('\n') == diagram.svg() => {}
            Ok(golden) => failures.push(format!(
                "{}: {}",
                named(path),
                diff(golden.trim_end_matches('\n'), diagram.svg())
            )),
            Err(error) => failures.push(format!("{}: no golden: {error}", named(path))),
        }
    }

    assert!(
        failures.is_empty(),
        "the corpus renders as the SVG checked in beside it, or {BLESS}=1 is run to move a \
         golden deliberately:\n{}",
        failures.join("\n")
    );
    println!(
        "\ncheck: eval mermaid-corpus ... ok ({} fences)",
        fences.len()
    );
}
