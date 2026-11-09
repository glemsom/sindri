//! Every fence in the corpus is drawn here and measured, and held to that and not to a
//! golden: the renderer measures each label against the machine's fonts and offers no way to
//! supply one, so a golden of it would be a golden of the box that blessed it.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use sindri::graphics::CellSize;
use sindri::mermaid::Diagram;

/// Where the corpus of real fences lives: one `.mmd` file per fence, and nothing else,
/// because there is no golden beside it any more.
const CORPUS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/mermaid");

/// A shared runner's patience, not a reader's: `mermaid::BOUND` bounds a stall a person feels.
const EVAL_BOUND: Duration = Duration::from_secs(30);

fn corpus() -> Vec<PathBuf> {
    let mut fences: Vec<PathBuf> = std::fs::read_dir(CORPUS)
        .unwrap_or_else(|error| panic!("the corpus at {CORPUS} is readable: {error}"))
        .map(|file| file.expect("a readable directory entry").path())
        .filter(|path| path.extension().is_some_and(|kind| kind == "mmd"))
        .collect();
    fences.sort();
    fences
}

fn source(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("{} is readable: {error}", path.display()))
}

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

    // The first draw in a process pays the renderer's start-up as well as the drawing,
    // so one is drawn untimed and every number below is a drawing.
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

fn head_and_tail(svg: &str) -> String {
    let head: String = svg.chars().take(60).collect();
    let tail: String = {
        let mut last: Vec<char> = svg.chars().rev().take(30).collect();
        last.reverse();
        last.into_iter().collect()
    };
    format!("{head:?} … {tail:?}")
}
