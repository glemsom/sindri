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
use sindri::pulse::Pulse;
use sindri::rail::Facts;
use sindri::rows::Scroll;
use sindri::view::View;
use sindri_core::policy::Enforcement;
use sindri_core::provider::Deltas;
use sindri_core::records::Entry;
use sindri_core::redact::SecretSet;
use sindri_core::run::{Exit, Stop};

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
        "transcript" => running(transcript(), Scroll::Tail),
        "transcript-scrolled" => running(transcript(), Scroll::Back(4)),
        // The rail itself, at the five widths §46 names: the frame of a run just begun, so
        // the ledger, the strip and the row that counts what went are what the eye is asked
        // to approve and the picture is not the transcript frame checked in twice.
        "rail" => running(opening(), Scroll::Tail),
        // A run in the middle of a turn: the same history, a sixth turn opened, and that
        // turn's answer and reasoning still landing (ticket #38).
        "streaming" => streaming(),
        // Every verdict a call row can wear, one call each: the picture of §12's four
        // states, approved in the commit that decides them (#40).
        "calls" => running(calls(), Scroll::Tail),
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
        // The marker at the head of the newest answer, twice: the run still live and
        // carrying its elapsed time, and the same run stopped with the duration it stopped
        // at (#39). One history and one answer, so what the eye is asked to approve between
        // the two pictures is the marker and nothing else. The live one's answer is still
        // arriving — three of its four fragments have landed — because a marker carrying
        // elapsed time is for a turn that has not finished speaking.
        "pulse-live" => with_pulse(Pulse::live(4_200), &PULSE_ANSWER[..3]),
        // The settled marker says the duration the run stopped at, and nine point four
        // seconds rather than the ticket's dozen: §12's table gives a duration past ten
        // seconds in whole seconds, so the form is the ticket's and the value is the table's.
        "pulse-settled" => with_pulse(Pulse::settled(&Stop::Final, 9_400), &PULSE_ANSWER),
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

/// The frame a run is in: the entries it has written, where the transcript is scrolled to,
/// and the facts the rail draws them beside (#46).
fn running(entries: Vec<Entry>, scroll: Scroll) -> View {
    let mut view = View::of(entries, scroll);
    view.set_facts(facts());
    view
}

/// The facts a run's rail draws: one of each of §46's blocks, with a workspace, a branch
/// and a session temp as long as the real ones are — the transcript's six turns, so the
/// rail and the history beside it are one picture of one run.
///
/// A frame of chrome alone — boot, a draft being typed, the `?` screen — is handed no
/// facts and draws no rail: a frame knows what it was given, and nothing invents the rest
/// (§15).
fn facts() -> Facts {
    Facts {
        model: Some(String::from("claude-sonnet")),
        turns: Some((6, 250)),
        ctx: Some(21),
        cache: Some(88),
        workspace: Some(PathBuf::from(
            "/home/glemsom/Documents/git/sindri/.worktrees/46-rail",
        )),
        branch: Some(String::from("spec/1-capability")),
        skills: Some(2),
        sandbox: Some(Enforcement::Bubblewrap),
        session: Some(String::from("9f2c7a41-6a1e-4b3d-8f5c-0d2e7a4b9c61")),
        temp: Some(PathBuf::from(
            "/home/glemsom/.local/state/sindri/sessions/9f2c7a41-6a1e-4b3d-8f5c-0d2e7a4b9c61/tmp",
        )),
    }
}

/// The first turn of the run `transcript` holds, as the entries it opened with: what a
/// rail frame is drawn beside, because the rail is the subject of those five pictures.
fn opening() -> Vec<Entry> {
    transcript().into_iter().take(4).collect()
}

/// The entries a transcript frame is painted from: three turns of a run, each with the
/// call it asked for, what that call answered and the answer the turn ended with — so a
/// frame holds every shape a row takes, and enough of them to run past the foot of the
/// band and be scrolled.
///
/// The facts are the ones this ticket's own module header states, because a fixture is
/// a picture of a real frame rather than a grid of sample text.
fn transcript() -> Vec<Entry> {
    vec![
        Entry::Turn { turn: 1 },
        call(1, "bash", r#"{"command": "rg -n 'fence' src"}"#),
        result(
            1,
            "bash",
            Exit::Code(0),
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
            Exit::Code(0),
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
        result(3, "bash", Exit::Code(0), "check: test ... ok", 64, 41),
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
            Exit::Code(0),
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
            Exit::Code(0),
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
        Entry::Turn { turn: 6 },
        call(6, "bash", r#"{"command": "make check STAGES=frames"}"#),
        result(
            6,
            "bash",
            Exit::Code(0),
            "check: frame calls ... ok\ncheck: frame transcript ... ok",
            104,
            31,
        ),
        answer(
            6,
            "A row says the exit code and not the command, because the verdict is a fact \
             the record holds and the command is a fact the row above it holds: a call \
             that settled is one line, and the line is what happened to it.",
        ),
    ]
}

/// The frame a run is in the middle of: the history the transcript frame holds, a sixth
/// turn opened on it, and the thought and the answer of that turn landing a fragment at a
/// time — through `Deltas`, the same door the wire streams them through, so a picture of
/// a stream is made the way a stream is made (#43).
///
/// The answer is left with its code fence still open, which is the one markdown rule this
/// ticket keeps (ADR-0008): from the fence down, the tail is drawn as code and never as
/// prose that ate the rest of the answer — and #52 keeps that rule when it replaces the
/// plain rendering.
fn streaming() -> View {
    let mut entries = transcript();
    entries.push(Entry::Turn { turn: 6 });
    let mut view = running(entries, Scroll::Tail);
    for fragment in [
        "The ticket asks whether the tail survives. ",
        "A fence still streaming is a code block, ",
        "so its words are drawn as code and nothing is swallowed.",
    ] {
        view.reasoning(fragment);
    }
    for fragment in [
        "It survives: ",
        "the rule is one boolean over the lines, ",
        "and #52 keeps it.\n",
        "```rust\n",
        "fn main() {\n",
        "    let fence = Fence::open();\n",
        "    let  tail = fence.words();\n",
    ] {
        view.text(fragment);
    }
    view
}

/// The words the pulse frames' newest answer is made of, a fragment at a time: the same
/// answer in both pictures, so the difference the eye approves between them is the marker
/// and nothing else.
const PULSE_ANSWER: [&str; 4] = [
    "A braille marker at the head of the newest answer carries the run's elapsed ",
    "time, one step every 80 ms, and settles on the verdict it stopped with — ",
    "the duration it stopped at, kept and never recomputed. ",
    "The rows under it keep the answer's own gutter, held open and blank.",
];

/// The frame a run is in while its newest answer is landing, wearing `pulse` at that
/// answer's head: a settled turn above, a turn opened on it, and that turn's thought and
/// answer streamed through the same `Deltas` door the wire streams them through, so the
/// picture of a marker is made the way a marker is made (#39).
///
/// The answer is long enough to wrap past its first row, which is what shows the marker
/// sitting at the head of one row and the answer's own gutter holding the rest.
fn with_pulse(pulse: Pulse, said: &[&str]) -> View {
    let mut entries = opening();
    entries.push(Entry::Turn { turn: 2 });
    let mut view = running(entries, Scroll::Tail);
    for fragment in [
        "The marker is a clock, so the row it heads is laid out again whenever ",
        "the mark it wears moves, and a step that changes nothing costs nothing. ",
    ] {
        view.reasoning(fragment);
    }
    for fragment in said {
        view.text(fragment);
    }
    view.set_pulse(Some(pulse));
    view
}

/// The entries a verdict frame is painted from: four calls settled four ways — the
/// four states §12 gives a tool row — so a reader of the fixture sees what `✓ exit 0`,
/// `✗ exit N`, `✗ timed out` and `✗ cancelled` look like before meeting them in a run.
///
/// The two that printed nothing printed nothing because there was nothing to print: a
/// command the timeout killed and one the user's word stopped never reached their output.
fn calls() -> Vec<Entry> {
    vec![
        Entry::Turn { turn: 1 },
        call(1, "bash", r#"{"command": "rg -n 'verdict' src"}"#),
        result(
            1,
            "bash",
            Exit::Code(0),
            "src/rows.rs: the check or the cross in `ok`/`err`",
            64,
            18,
        ),
        answer(1, "The row is drawn from the verdict the record holds."),
        Entry::Turn { turn: 2 },
        call(2, "bash", r#"{"command": "cargo clippy --all-targets"}"#),
        result(
            2,
            "bash",
            Exit::Code(1),
            "warning: unused import: `std::fmt`\n  --> src/rows.rs:12:5",
            96,
            211,
        ),
        answer(2, "An unused import, and nowhere else to put it."),
        Entry::Turn { turn: 3 },
        call(3, "bash", r#"{"command": "cargo test --workspace"}"#),
        result(3, "bash", Exit::TimedOut, "", 0, 120_000),
        answer(3, "The call ran past its minute and was killed."),
        Entry::Turn { turn: 4 },
        call(4, "bash", r#"{"command": "make check STAGES=frames"}"#),
        result(4, "bash", Exit::Cancelled, "", 0, 3_400),
        answer(4, "The run stopped on the user's word, and the draft kept."),
        // A run in progress ends on a call nobody has answered yet: the row names the
        // call and claims nothing, which is what a cancelled call would stay as if the
        // verdict never reached it.
        Entry::Turn { turn: 5 },
        call(5, "bash", r#"{"command": "make check STAGES=test"}"#),
    ]
}

/// A call as the run writes one: the tool the model named and the arguments it wrote.
fn call(turn: u32, name: &str, arguments: &str) -> Entry {
    Entry::Call {
        turn,
        name: String::from(name),
        arguments: SecretSet::default().redact(arguments),
    }
}

/// What came back from a call: the verdict that ended it, and what it printed.
fn result(turn: u32, name: &str, exit: Exit, text: &str, bytes: u64, elapsed_ms: u64) -> Entry {
    Entry::Result {
        turn,
        name: String::from(name),
        text: SecretSet::default().redact(text),
        bytes,
        elapsed_ms,
        exit: Some(exit),
    }
}

/// The turn's answer: what the model said once the calls it asked for were done.
fn answer(turn: u32, text: &str) -> Entry {
    Entry::Assistant {
        turn,
        text: SecretSet::default().redact(text),
    }
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
