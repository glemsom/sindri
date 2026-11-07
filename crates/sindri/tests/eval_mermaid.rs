//! The mermaid corpus, held to the fact that each diagram *is* a picture: every
//! diagram type a fence is really written as is drawn in this process and the
//! drawing is measured in cells, which is what the front end needs of it before a
//! pixel is spent (§12, §16, ADR-0003).
//!
//! What is held is the drawing and never the fence: a fence that parses is proved
//! elsewhere, and the corpus is here because a renderer's breadth is a claim the
//! package makes and nothing else in the suite would notice if a diagram type
//! stopped drawing.
//!
//! The comparison is deliberately not byte for byte. `mermaid-rs-renderer` measures
//! every label against the fonts installed on the machine (`fontdb`'s system load,
//! which it offers no way to replace), so the same fence is a different SVG, with
//! different widths and different wrapping, on a machine with different fonts: a
//! golden of it is a golden of the box that blessed it. What travels with this
//! binary is the font a *picture* is drawn in (ADR-0003), and that is the half the
//! goldens can hold — so the drawing is proved here to exist, to be whole, and to
//! measure, and its geometry goes back into a golden the day the renderer lets a
//! font be supplied.
//!
//! An eval is judged by `make evals` and never by `make test`, which is why the
//! test here is ignored: the stage that owns a failure is the stage that prints it.

use std::path::{Path, PathBuf};

use sindri::graphics::CellSize;
use sindri::mermaid::{self, Block, Fence};

/// Where the corpus of real fences lives: one `.mmd` file per fence, and nothing else,
/// because there is no golden beside it any more.
const CORPUS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/mermaid");

/// How a refusal says the renderer was given the half a second `mermaid::BOUND` allows and
/// was not finished: `Block::Refused` carries the sentence a caption prints and not the kind
/// of refusal, so the sentence is what a starved render is recognised by.
const STARVED: &str = "took longer than";

/// The corpus, in name order, so two runs of the test walk the same fences.
fn corpus() -> Vec<PathBuf> {
    let mut fences: Vec<PathBuf> = std::fs::read_dir(CORPUS)
        .unwrap_or_else(|error| panic!("the corpus at {CORPUS} is readable: {error}"))
        .map(|file| file.expect("a readable directory entry").path())
        .filter(|path| path.extension().is_some_and(|kind| kind == "mmd"))
        .collect();
    fences.sort();
    fences
}

/// `fence.mmd` as a test names it, which is how a failure says which fence it was.
fn named(path: &Path) -> String {
    path.file_stem()
        .expect("a corpus file has a name")
        .to_string_lossy()
        .into_owned()
}

#[test]
#[ignore = "an eval is judged by `make evals`, not by `make test`"]
fn every_fence_in_the_corpus_is_drawn_as_a_picture_that_measures() {
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
                Block::Picture(diagram) => break Some(diagram),
                // Only a render starved of a processor is tried again; a fence the renderer
                // will not read fails now, in the renderer's own words, rather than after a
                // deadline it was never going to beat.
                Block::Refused { caption } if caption.contains(STARVED) => {
                    if std::time::Instant::now() > deadline {
                        failures.push(format!(
                            "{}: still drawing after 20 s: {caption}",
                            named(path)
                        ));
                        break None;
                    }
                }
                Block::Refused { caption } => {
                    failures.push(format!("{}: refused, and said: {caption}", named(path)));
                    break None;
                }
                Block::Streaming => {
                    failures.push(format!(
                        "{}: a fence that closed is not still arriving",
                        named(path)
                    ));
                    break None;
                }
            }
        };
        let Some(diagram) = diagram else { continue };

        let svg = diagram.svg();
        if !svg.trim_start().starts_with("<svg") || !svg.trim_end().ends_with("</svg>") {
            failures.push(format!(
                "{}: the drawing is not a whole SVG: {}",
                named(path),
                head_and_tail(svg)
            ));
            continue;
        }
        match diagram.footprint(CellSize::NOMINAL) {
            Ok(footprint) if footprint.cols() > 0 && footprint.rows() > 0 => {}
            Ok(footprint) => failures.push(format!(
                "{}: the drawing takes no cells at all: {} by {}",
                named(path),
                footprint.cols(),
                footprint.rows()
            )),
            Err(refusal) => failures.push(format!(
                "{}: the drawing is a picture nothing can measure: {}",
                named(path),
                refusal.message()
            )),
        }
    }

    assert!(
        failures.is_empty(),
        "every fence in the corpus is drawn as a picture that measures:\n{}",
        failures.join("\n")
    );
    println!(
        "\ncheck: eval mermaid-corpus ... ok ({} fences)",
        fences.len()
    );
}

/// The beginning and the end of a drawing, so a failure says what the SVG actually was
/// rather than printing a whole one.
fn head_and_tail(svg: &str) -> String {
    let head: String = svg.chars().take(60).collect();
    let tail: String = {
        let mut last: Vec<char> = svg.chars().rev().take(30).collect();
        last.reverse();
        last.into_iter().collect()
    };
    format!("{head:?} … {tail:?}")
}
