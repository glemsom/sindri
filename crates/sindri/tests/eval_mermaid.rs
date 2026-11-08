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
use std::time::{Duration, Instant};

use sindri::graphics::CellSize;
use sindri::mermaid::Diagram;

/// Where the corpus of real fences lives: one `.mmd` file per fence, and nothing else,
/// because there is no golden beside it any more.
const CORPUS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/mermaid");

/// How long this eval waits for one fence: its own patience, deliberately not
/// `mermaid::BOUND`, because a bound on a stall a reader feels says nothing about what a
/// corpus costs on a shared runner.
const EVAL_BOUND: Duration = Duration::from_secs(30);

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

/// The fence `path` holds, or a panic naming the file that is not readable.
fn source(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("{} is readable: {error}", path.display()))
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

    // The first draw in a process pays the renderer's start-up as well as the drawing, and
    // that is no fence's cost: one is drawn untimed so every number below is a drawing.
    let _ = Diagram::of_within(EVAL_BOUND, &source(&fences[0]));

    let mut failures = Vec::new();
    let mut slowest: Option<(String, Duration)> = None;
    for path in &fences {
        let source = source(path);
        let started = Instant::now();
        let diagram = match Diagram::of_within(EVAL_BOUND, &source) {
            Ok(diagram) => diagram,
            Err(refusal) => {
                failures.push(format!(
                    "{}: refused, and said: {}",
                    named(path),
                    refusal.message()
                ));
                continue;
            }
        };
        let drawn = started.elapsed();
        if slowest.as_ref().is_none_or(|(_, taken)| drawn > *taken) {
            slowest = Some((named(path), drawn));
        }

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
    let (slowest, taken) = slowest.expect("every fence drew, so one of them was the slowest");
    println!(
        "\ncheck: eval mermaid-corpus ... ok ({} fences, slowest {slowest} at {} ms)",
        fences.len(),
        taken.as_millis()
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
