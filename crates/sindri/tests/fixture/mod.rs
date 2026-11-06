//! One frame rendered at a fixed size into the text a fixture holds: the harness
//! behind `assert_frame`, which paints the code the terminal is painted, writes
//! down every cell's glyph and the ink it is in, and holds the result to the
//! accepted picture beside it.
//!
//! A fixture is text, and its lines are the screen's: a header naming the frame and
//! the size it was drawn at, and then one line per row — the row's number, its
//! glyphs between two bars, and the ink of each run of cells from the left edge.
//!
//!     boot at 120x40
//!     00 |                                    ...                              | plain:120
//!     39 |Enter send · ? keys                                                    | muted:10 dim:3 muted:6 plain:101
//!
//! The bars are why a row of blank cells keeps its width in a file an editor may
//! touch: nothing a cell holds hides behind a trailing space. The inks are names —
//! `dim`, `muted`, `accent`, `ok`, `err`, and `plain` for a cell the terminal draws
//! in whatever ink its reader chose — and never colours, so a palette change cannot
//! break a fixture and a fixture cannot smuggle in a theme (ADR-0012).
//!
//! A fixture's file is a `.snap` and not the `.txt` ADR-0012 first named, because
//! `insta` compares and blesses it and hard-codes that extension
//! (`insta-1.49.0/src/runtime.rs:298`). The frame inside the file is the text below
//! the metadata fence, and the amendment to that ADR records the deviation.
//!
//! What this harness refuses to guess at, rather than freeze half a picture: a cell
//! holds one glyph in one column, and a cell is drawn in an `Ink` and nothing else.
//! A wide glyph eats its neighbour, a cell can arrive with no glyph at all, and a
//! cell can be painted a colour, a background or a modifier that is not an ink. Each
//! of those stops the test with a message naming the cell.

use std::path::PathBuf;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::{Buffer, Cell};

use sindri::composer::{Composer, Key};
use sindri::ink::Ink;
use sindri::rows::Scroll;
use sindri::view::View;
use sindri_core::records::Entry;
use sindri_core::redact::SecretSet;

/// Where the accepted pictures live: beside the tests that compare them, one file
/// per frame and size (ADR-0012).
const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");

/// Renders the frame `name` at `cols` columns by `rows` rows and holds it to the
/// fixture that is the accepted picture of it, naming the first cell that differs
/// before `insta` is given the last word.
pub fn assert_frame(name: &str, cols: u16, rows: u16) {
    let fixture = format!("{name}-{cols}x{rows}");
    let drawn = drawn(name, cols, rows);

    // `make frame F=…` prints one frame as the code draws it now: a picture nobody
    // can see is a picture nobody approves. This changes what is printed and never
    // what is compared, so a stray variable in a shell cannot make a test report a
    // picture it did not check.
    if std::env::var("SINDRI_FRAME").as_deref() == Ok(fixture.as_str()) {
        println!("{drawn}");
    }

    // The cell before the line: a frame is a picture, and `insta`'s diff of one says
    // which rows moved rather than which cell is wrong.
    if let Some(picture) = accepted(&fixture)
        && let Some(differed) = first_difference(&picture, &drawn)
    {
        println!("frame {fixture}: {differed}");
    }

    insta::with_settings!({ snapshot_path => FIXTURES, prepend_module_to_snapshot => false }, {
        insta::assert_snapshot!(fixture.as_str(), drawn);
    });

    // The line `make check` ends on, written on a line of its own: libtest has left the
    // test's name unterminated on the terminal while this test ran.
    println!("\ncheck: frame {fixture} ... ok");
}

/// The frame a fixture's name stands for: the one place a name and the facts it is
/// painted from are written down together, so the file, the test that holds it and
/// `make frame F=…` cannot disagree about what `boot` is.
fn named(name: &str) -> View {
    match name {
        "boot" => View::boot(),
        "transcript" => View::of(transcript(), Scroll::Tail),
        "transcript-scrolled" => View::of(transcript(), Scroll::Back(4)),
        // The composer's floor and its ceiling, and the window it scrolls to when the
        // draft outgrows the ceiling: the growth of §12 as three pictures rather than
        // as an opinion (ticket #41).
        "typing" => View::typing(draft(&["why does the retry test fail"])),
        "grown" => View::typing(draft(&[
            "read the transcript, then the failing test,",
            "then the patch, and say which line of the",
            "patch the test disagrees with. Keep it to",
            "one sentence and quote the line you mean,",
            "the way the report does.",
        ])),
        "scrolled" => View::typing(draft(&[
            "one: the test that fails",
            "two: the line it fails on",
            "three: the patch that moved it",
            "four: the retry that hid it",
            "five: the budget it spent",
            "six: the stop reason",
            "seven: the session it left",
            "eight: the question to ask",
        ])),
        // The `?` screen, opened through the same key a reader presses rather than by
        // reaching into the stack: the frame lists the keys of the tables that answer
        // them, so a picture of the screen is a picture of the keymap (ticket #45, A7).
        "keys" => {
            let mut view = View::boot();
            view.key(Key::Char('?'));
            view
        }
        unknown => panic!(
            "no frame is named {unknown}: `named` lists the frames this front end \
             paints, and a fixture under any other name is a picture of nothing"
        ),
    }
}

/// The entries a transcript frame is painted from: three turns of a run, each with the
/// call it asked for, what that call answered and the answer the turn ended with — so a
/// frame holds every shape a row takes, and enough of them to run past the foot of the
/// band and be scrolled.
///
/// The facts are the ones this ticket's own module header states, because a fixture is
/// a picture of a real frame rather than a grid of sample text.
fn transcript() -> Vec<Entry> {
    let secrets = SecretSet::default();
    let call = |turn, name: &str, arguments: &str| Entry::Call {
        turn,
        name: String::from(name),
        arguments: secrets.redact(arguments),
    };
    let result = |turn, name: &str, text: &str, bytes, elapsed_ms| Entry::Result {
        turn,
        name: String::from(name),
        text: secrets.redact(text),
        bytes,
        elapsed_ms,
    };
    let answer = |turn, text: &str| Entry::Assistant {
        turn,
        text: secrets.redact(text),
    };
    vec![
        Entry::Turn { turn: 1 },
        call(1, "bash", r#"{"command": "rg -n 'fence' src"}"#),
        result(
            1,
            "bash",
            "src/rows.rs: the glyph a turn opens with\nsrc/rows.rs: a word longer than a row is broken",
            412,
            23,
        ),
        answer(
            1,
            "Rendering lives in the front end and not in the core, because an entry is a \
             fact and never a rendering: the TUI and any reader draw one run from one \
             value, and the core may not name a terminal at all.",
        ),
        Entry::Turn { turn: 2 },
        call(
            2,
            "bash",
            r#"{"command": "cargo test -p sindri --test rows"}"#,
        ),
        result(
            2,
            "bash",
            "running 4 tests\ntest result: ok. 4 passed; 0 failed",
            208,
            97,
        ),
        answer(
            2,
            "A width change drops what the cache held and reflows on the next paint, once, \
             because a paint lays an entry out to draw it and then keeps the answer. The \
             cache is one width deep, which is what a resize being a reflow and not a \
             lookup means.",
        ),
        Entry::Turn { turn: 3 },
        call(3, "bash", r#"{"command": "make check STAGES=rows"}"#),
        result(3, "bash", "check: test ... ok", 64, 41),
        answer(
            3,
            "The window is measured from the tail backwards, so following the newest row \
             never asks how tall the history is: it lays out the entries it shows and \
             stops at the first row that fills the band. Only the rows a band reaches are \
             laid out, so a 250-turn history on a 40-row terminal pays for what it shows \
             and no more.",
        ),
        Entry::Turn { turn: 4 },
        call(4, "bash", r#"{"command": "make check"}"#),
        result(
            4,
            "bash",
            "check: fmt ... ok\ncheck: clippy ... ok\ncheck: test ... ok\ncheck: frames ... ok",
            96,
            118,
        ),
        answer(
            4,
            "What that buys is the plain case and what it costs is the scrolled one: a \
             window is a row offset from the tail, so resolving one walks the entries \
             between the tail and the window and lays out every one of them on the way. \
             Each is laid out once and kept, so the whole history scrolled through costs \
             one layout per entry and scrolling it again costs none.",
        ),
        Entry::Turn { turn: 5 },
        call(5, "bash", r#"{"command": "git diff --stat"}"#),
        result(
            5,
            "bash",
            "crates/sindri/src/rows.rs | 412 +++++++++++++\ncrates/sindri/tests/rows.rs | 198 +++++++++",
            128,
            9,
        ),
        answer(
            5,
            "The transcript's unit is the entry. An entry never carries a formatted line, \
             so the same run is drawn the same way by the TUI, by a reader of the \
             transcript and by whatever front end comes next, and a streaming tick that \
             changes one entry re-lays that entry alone.",
        ),
    ]
}

/// A composer holding `lines`, typed a line at a time: a fixture's draft is built
/// through the same keys a reader's fingers send, so a picture of the composer cannot
/// be made a way the product cannot.
fn draft(lines: &[&str]) -> Composer {
    let mut composer = Composer::new();
    for (index, line) in lines.iter().enumerate() {
        if index > 0 {
            composer.key(Key::Newline);
        }
        composer.paste(line);
    }
    composer
}

/// The frame `name` at `cols` by `rows`, as the text a fixture holds: painted through
/// a headless backend, because a look that cannot be rendered without a terminal
/// cannot be a test (ADR-0012).
fn drawn(name: &str, cols: u16, rows: u16) -> String {
    let mut view = named(name);
    let mut terminal =
        Terminal::new(TestBackend::new(cols, rows)).expect("a headless screen of that size");
    let completed = terminal
        .draw(|frame| view.paint(frame.area(), frame.buffer_mut()))
        .expect("the frame is painted");
    text(name, completed.buffer)
}

/// A painted buffer, as the fixture text that holds it.
fn text(name: &str, buffer: &Buffer) -> String {
    let area = buffer.area;
    let mut text = format!("{name} at {}x{}", area.width, area.height);
    for y in area.y..area.bottom() {
        let mut glyphs = String::new();
        let mut runs: Vec<(Ink, u16)> = Vec::new();
        for x in area.x..area.right() {
            let cell = buffer
                .cell((x, y))
                .unwrap_or_else(|| panic!("row {y}, column {x} is outside the frame it is in"));
            let ink = Ink::of(cell).unwrap_or_else(|| {
                panic!(
                    "row {y}, column {x} is drawn in a style that is not an ink: \
                     foreground {:?}, background {:?}, modifiers {:?} — a fixture names \
                     inks, so a cell no ink names cannot be written down",
                    cell.fg, cell.bg, cell.modifier
                )
            });
            glyphs.push(glyph(cell, x, y));
            match runs.last_mut() {
                Some((last, count)) if *last == ink => *count += 1,
                _ => runs.push((ink, 1)),
            }
        }
        let runs: Vec<String> = runs
            .iter()
            .map(|(ink, count)| format!("{}:{count}", ink.name()))
            .collect();
        text.push_str(&format!("\n{y:02} |{glyphs}| {}", runs.join(" ")));
    }
    text
}

/// The one glyph a cell holds, as a fixture can carry it: one character, which is one
/// column of the screen.
fn glyph(cell: &Cell, x: u16, y: u16) -> char {
    let symbol = cell.symbol();
    let mut characters = symbol.chars();
    match (characters.next(), characters.next()) {
        (Some(glyph), None) => glyph,
        _ => panic!(
            "row {y}, column {x} holds {symbol:?}, which is not one glyph: a fixture \
             counts one glyph per column, so a glyph wider than one column — and the \
             cell it ate — has nowhere to be written down"
        ),
    }
}

/// The picture a fixture holds, or nothing when no fixture was blessed yet: the
/// file's first fence is `insta`'s metadata, and the frame is the text below it.
fn accepted(fixture: &str) -> Option<String> {
    let path = PathBuf::from(FIXTURES).join(format!("{fixture}.snap"));
    let text = std::fs::read_to_string(path).ok()?;
    let mut lines = text.lines();
    if lines.next()? != "---" {
        return Some(text);
    }
    let frame: Vec<&str> = lines.skip_while(|line| *line != "---").skip(1).collect();
    Some(frame.join("\n"))
}

/// The first thing a fixture and a frame disagree about: the first cell, said with
/// both glyphs and both inks, or the first line that is not a row at all.
fn first_difference(picture: &str, drawn: &str) -> Option<String> {
    let picture: Vec<&str> = picture.lines().collect();
    let drawn: Vec<&str> = drawn.lines().collect();

    for (line, (expected, actual)) in picture.iter().zip(&drawn).enumerate() {
        if expected == actual {
            continue;
        }
        return Some(match (row_of(expected), row_of(actual)) {
            (Some(expected), Some(actual)) => differing_cell(&expected, &actual),
            _ => format!(
                "line {} reads {expected:?} where the frame draws {actual:?}",
                line + 1
            ),
        });
    }

    (picture.len() != drawn.len()).then(|| {
        format!(
            "the fixture holds {} lines and the frame draws {}",
            picture.len(),
            drawn.len()
        )
    })
}

/// The first cell two rows disagree about, named by row, column, and both glyphs
/// with the ink each of them is in.
fn differing_cell(expected: &Row, actual: &Row) -> String {
    let columns = expected.glyphs.len().max(actual.glyphs.len());
    for column in 0..columns {
        let (held, drawn) = (cell(expected, column), cell(actual, column));
        if held != drawn {
            return format!(
                "row {}, column {column}: the fixture holds {held} and the frame draws {drawn}",
                expected.number
            );
        }
    }
    format!(
        "row {}: every cell of it agrees and its line does not, which this harness wrote wrong",
        expected.number
    )
}

/// One cell of a row as a failure says it: its glyph and the ink it is in.
fn cell(row: &Row, column: usize) -> String {
    match (row.glyphs.get(column), row.inks.get(column)) {
        (Some(glyph), Some(ink)) => format!("{glyph:?} in {}", ink.name()),
        _ => String::from("no cell at all"),
    }
}

/// One row of a fixture, as the cells it stands for.
struct Row {
    /// The row number the fixture wrote, which is the row a reader counts.
    number: u16,
    /// The glyph in every column, left to right.
    glyphs: Vec<char>,
    /// The ink of every column, left to right.
    inks: Vec<Ink>,
}

/// The row a fixture line holds, or nothing when the line is not a row.
fn row_of(line: &str) -> Option<Row> {
    let (number, rest) = line.split_once(' ')?;
    let (glyphs, runs) = rest.strip_prefix('|')?.rsplit_once('|')?;
    let inks = inks_in(runs.trim())?;
    let glyphs: Vec<char> = glyphs.chars().collect();
    (glyphs.len() == inks.len()).then_some(Row {
        number: number.parse().ok()?,
        glyphs,
        inks,
    })
}

/// The ink of every cell a run list names, left to right: `plain:40 accent:4`, the
/// runs summing to the width of the row they belong to.
fn inks_in(runs: &str) -> Option<Vec<Ink>> {
    let mut inks = Vec::new();
    for run in runs.split_whitespace() {
        let (ink, count) = run.split_once(':')?;
        let ink = Ink::from_name(ink)?;
        let count: u16 = count.parse().ok()?;
        inks.extend(std::iter::repeat_n(ink, usize::from(count)));
    }
    Some(inks)
}
