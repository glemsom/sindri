//! The transcript's cache of rows, held to its three promises through the seams a
//! later ticket will spend: a width change reflows once, a scroll across a long
//! history re-lays nothing, and an entry the window never reaches is never laid out
//! at all (§12, ADR-0008).
//!
//! Every assertion here counts layouts through the cache's own `lays`, because the
//! promises are about how often layout happens and not about the shape of the code
//! that does it: a test that read the shape would still pass on the day the cost came
//! back. The one assertion that is not a count is the window's own arithmetic — the
//! rows it shows add up to the rows it was given — which is what makes a scroll cut
//! an entry in half rather than count it whole.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use sindri::ink::{Ink, Look};
use sindri::pulse::Pulse;
use sindri::rows::{Row, Rows, Scroll, Window, layout};
use sindri::stream::Stream;
use sindri_core::provider::Deltas;
use sindri_core::records::Entry;
use sindri_core::redact::SecretSet;
use sindri_core::run::Exit;

/// A turn as the run writes one: a single row at any width, so a test that counts
/// entries and a test that counts rows can be told apart.
fn turn(which: u32) -> Entry {
    Entry::Turn { turn: which }
}

/// An answer long enough to wrap, so a window's height is not one row per entry and a
/// width change is visible as a different number of rows rather than as the same one.
fn answer(which: u32) -> Entry {
    Entry::Assistant {
        turn: which,
        text: SecretSet::default().redact(
            "The closing fence is what ends a code block, so a fence still streaming is \
             rendered as one until that line arrives: a half-written diagram never costs \
             a raster pass and never flickers while it is being typed.",
        ),
    }
}

/// A transcript of the shapes a row takes: a turn, a call and what came back, and an
/// answer that wraps.
fn transcript() -> Vec<Entry> {
    let mut entries = Vec::new();
    for which in 1..=3 {
        entries.push(turn(which));
        entries.push(answer(which));
    }
    entries
}

/// The rows a window shows, summed over the entries it holds: the band's own height
/// whenever the transcript is longer than the window.
fn rows_shown(window: &Window) -> u16 {
    window.pieces().iter().map(|piece| piece.rows).sum()
}

/// A width change drops every row the cache held and lays the window out once: the
/// first promise of ADR-0008, counted rather than described.
#[test]
fn a_width_change_invalidates_the_cache_and_reflows_once() {
    let mut rows = Rows::new();
    for entry in transcript() {
        rows.push(entry);
    }

    rows.resize(80);
    let wide = rows.window(Scroll::Tail, 6);
    let shown = wide.pieces().len();
    assert!(
        shown > 0,
        "a window at the tail shows the end of the transcript"
    );
    assert_eq!(rows.lays(), shown, "the first pass lays out what it shows");

    // The same width, asked again, is the same picture and costs no layout at all.
    let again = rows.window(Scroll::Tail, 6);
    assert_eq!(again.pieces().len(), shown);
    assert_eq!(
        rows.lays(),
        shown,
        "a paint at an unchanged width re-lays nothing"
    );

    // The newest answer is on screen either way, and narrower is taller: the rows
    // itself are relaid, not merely recounted.
    let newest = rows.len() - 1;
    let wide_rows = rows.rows(newest).len();

    rows.resize(40);
    let narrow = rows.window(Scroll::Tail, 6);
    let reflowed = narrow.pieces().len();
    assert!(reflowed > 0);
    assert_eq!(
        rows.lays(),
        shown + reflowed,
        "a width change lays each visible entry out once"
    );
    let narrow_rows = rows.rows(newest).len();
    assert!(
        narrow_rows > wide_rows,
        "the answer wraps into more rows at 40 columns than at 80: {narrow_rows} against {wide_rows}"
    );
    assert_eq!(
        rows.lays(),
        shown + reflowed,
        "reading the rows costs nothing extra"
    );

    // The cache holds one width, as the ADR decided: going back is a reflow, not a
    // lookup of what the wider width used to hold.
    rows.resize(80);
    let restored = rows.window(Scroll::Tail, 6).pieces().len();
    assert_eq!(rows.lays(), shown + reflowed + restored);
}

/// Two hundred and sixty turns scrolled from the tail to the head and back: every
/// entry is laid out exactly once, and the way back costs nothing.
#[test]
fn scrolling_the_whole_transcript_lays_each_entry_out_once() {
    let history = 260;
    let mut rows = Rows::new();
    for which in 1..=history {
        rows.push(turn(which));
    }

    let room = 40;
    let mut back = 0;
    loop {
        let window = rows.window(Scroll::Back(back), room);
        // The band fills from the first row above the tail, so the only window that
        // shows less than the band is the one the head of the transcript cuts short.
        let expected = u32::from(room).min(history - back.min(history));
        assert_eq!(
            u32::from(rows_shown(&window)),
            expected,
            "the window {back} rows back shows exactly the rows it has"
        );
        assert!(
            rows.lays() <= history as usize,
            "an entry was laid out twice: {} layouts after {back} rows back",
            rows.lays()
        );
        let at_head = window
            .pieces()
            .first()
            .is_some_and(|piece| piece.entry == 0 && piece.skip == 0);
        if at_head {
            break;
        }
        back += u32::from(room);
        assert!(
            back < 4 * history,
            "the walk back never reached the head of the transcript"
        );
    }
    assert_eq!(
        rows.lays(),
        history as usize,
        "the whole history was laid out, one entry at a time"
    );

    // Scrolling back down to the tail is the same walk over rows the cache already
    // holds, and it lays out nothing new.
    for back in (0..=260).rev() {
        rows.window(Scroll::Back(back), room);
    }
    assert_eq!(
        rows.lays(),
        history as usize,
        "scrolling a cached history costs no layout at all"
    );
}

/// The rule §12 states: an entry that never intersects the viewport is never laid
/// out, so a long history on a short terminal pays for what it shows.
#[test]
fn an_entry_above_the_window_is_never_laid_out() {
    let mut rows = Rows::new();
    for which in 1..=250 {
        rows.push(turn(which));
    }

    let window = rows.window(Scroll::Tail, 40);
    let shown = window.pieces().len();
    assert_eq!(
        shown, 40,
        "a one-row turn each, so the last 40 are the window"
    );
    assert_eq!(rows.lays(), shown, "and nothing above them was laid out");
    assert!(
        rows.lays() < 250,
        "a 250-entry transcript on a 40-row terminal pays for {} entries",
        rows.lays()
    );
}

/// Heights come from the cache when it has them and from a layout when it does not,
/// and that is the trade-off the module's header records: a caller that wants the
/// whole transcript measured pays for what it has never seen, and the window never
/// does.
#[test]
fn the_whole_height_costs_a_layout_only_for_what_it_has_not_seen() {
    let mut rows = Rows::new();
    for which in 1..=250 {
        rows.push(turn(which));
    }
    rows.window(Scroll::Tail, 40);

    assert_eq!(rows.height(), 250);
    assert_eq!(
        rows.lays(),
        250,
        "measuring everything lays out everything it lacks"
    );
    assert_eq!(rows.height(), 250);
    assert_eq!(rows.lays(), 250, "and the second measurement is free");
}

/// Every row a layout produces fits the width it was laid out at, whatever the facts
/// are: a word with no space to break, an empty answer, a name longer than the band and
/// glyphs that take two cells each. A row that ran over would be clipped by the paint
/// and lost from whatever copies it, so the width is held here instead.
#[test]
fn every_row_fits_the_width_it_was_laid_out_for() {
    let secrets = SecretSet::default();
    let entries = [
        Entry::Turn { turn: 1000 },
        Entry::Call {
            turn: 1,
            name: String::from("a-tool-whose-name-is-longer-than-the-band-it-is-drawn-in"),
            arguments: secrets.redact(""),
        },
        Entry::Result {
            turn: 1,
            name: String::from("bash"),
            text: secrets.redact("unbreakableandunforgivingwordthatnospacecansplit\n日本語の答え"),
            bytes: u64::MAX,
            elapsed_ms: u64::MAX,
            exit: Some(Exit::Code(u8::MAX.into())),
        },
        Entry::Assistant {
            turn: 1,
            text: secrets.redact(""),
        },
        Entry::Reasoning {
            turn: 1,
            text: secrets
                .redact("a thought, in its own gutter\nand its own ink\n```\nand a fence\n"),
        },
        answer(1),
    ];

    for cols in [2, 3, 7, 40, 120] {
        for entry in &entries {
            for row in layout(entry, cols) {
                let drawn = row.width();
                assert!(
                    drawn <= cols,
                    "a row {drawn} columns wide was laid out for {cols}: {row:?}"
                );
            }
        }
    }
}

/// A row with nothing said in it is a row, not no row: an empty answer, a call that
/// carried no arguments, and a band narrower than any word all lay out as something the
/// window can count.
#[test]
fn an_entry_with_nothing_to_say_is_still_one_row() {
    let secrets = SecretSet::default();
    for entry in [
        Entry::Assistant {
            turn: 1,
            text: secrets.redact(""),
        },
        Entry::Call {
            turn: 1,
            name: String::from("bash"),
            arguments: secrets.redact(""),
        },
    ] {
        let rows: Vec<Row> = layout(&entry, 4);
        assert!(!rows.is_empty(), "{entry:?} laid out as no row at all");
    }
}

/// The words a row is drawn with, its gutter and all: what a test reads a laid-out
/// row back as when the ink is not the point.
fn words(row: &Row) -> String {
    row.spans().iter().map(|(text, _)| text.as_str()).collect()
}

/// The inks a row's spans are drawn in, in order.
fn inks(row: &Row) -> Vec<Ink> {
    row.spans().iter().map(|(_, look)| look.ink()).collect()
}

/// §4's two gutters and two inks, held at the row and not at the frame: an answer is
/// printed from its own gutter in the frame's own ink, and a thought is set inside a
/// rule in the ink secondary prose is read in, so a reader can tell what the run said
/// from what it said to itself without a label for either.
#[test]
fn a_thought_and_an_answer_each_keep_their_own_gutter_and_ink() {
    let secrets = SecretSet::default();
    let thought = layout(
        &Entry::Reasoning {
            turn: 1,
            text: secrets.redact("the fence is the question: a fence still open is code"),
        },
        60,
    );
    assert!(!thought.is_empty(), "a thought is rows like anything else");
    for row in &thought {
        let spans = row.spans();
        assert_eq!(
            spans.first(),
            Some(&(String::from("│ "), Look::plain(Ink::Dim))),
            "a thought opens with its own rule: {row:?}"
        );
        assert!(
            spans[1..]
                .iter()
                .all(|(_, look)| *look == Look::plain(Ink::Muted)),
            "a thought is read in the ink reasoning is read in: {row:?}"
        );
    }

    let answer = layout(
        &Entry::Assistant {
            turn: 1,
            text: secrets.redact("the fence is the question"),
        },
        60,
    );
    for row in &answer {
        assert_eq!(
            words(row).chars().take(2).collect::<String>(),
            String::from("  "),
            "an answer is printed from its own gutter: {row:?}"
        );
        assert!(
            inks(row).iter().all(|ink| *ink == Ink::Plain),
            "and in the frame's own ink: {row:?}"
        );
    }

    // The thought's rule sits in the answer's own margin, two columns wide, so the
    // two kinds of row line up under each other and a thought reads as prose set
    // inside the answer's own gutter rather than as a second, wider layout.
    assert_eq!(
        thought[0].spans()[0].0.chars().count(),
        2,
        "a thought's gutter is the width of an answer's"
    );
}

/// §12's code row, and ADR-0008's rule with it: a fenced block is guttered by its
/// language's tag and drawn as the code it was written, its own spacing kept and its
/// words broken at the band's edge and nowhere else — and the answer's prose resumes
/// under the closing fence. A fence whose closing line has not landed yet is the same
/// block, open to the end of the text, so a half-streamed block is never prose that ate
/// the rest of the answer.
#[test]
fn a_fenced_block_is_guttered_by_its_language_and_the_prose_resumes_after_it() {
    let rows = layout(
        &Entry::Assistant {
            turn: 1,
            text: SecretSet::default().redact(
                "The closing fence is what ends a block:\n```rust\nfn main() {\n    let  x = 1;\n}\n```\nand the prose goes on.",
            ),
        },
        60,
    );

    let drawn: Vec<String> = rows.iter().map(words).collect();
    assert_eq!(
        drawn,
        [
            "  The closing fence is what ends a block:",
            "",
            "  ▏ rust",
            "  ▏ fn main() {",
            "  ▏     let  x = 1;",
            "  ▏ }",
            "",
            "  and the prose goes on.",
        ],
        "the tag stands over the code, the code is the code that was written, and the \
         answer's own prose resumes after the closing fence"
    );
    assert_eq!(
        inks(&rows[2]),
        [Ink::Plain, Ink::Dim, Ink::Dim],
        "the bar and the language tag are facts about what the text is: {:?}",
        rows[2]
    );
    assert_eq!(
        rows[3].spans()[..2],
        [
            (String::from("  "), Look::plain(Ink::Plain)),
            (String::from("▏ "), Look::plain(Ink::Dim)),
        ],
        "every row of the block is set behind the answer's gutter and the block's bar"
    );
    assert_eq!(
        inks(&rows[7]),
        [Ink::Plain, Ink::Plain],
        "and the prose after the fence is the answer's own ink again: {:?}",
        rows[7]
    );
}

/// A fence whose closing line has not landed yet is a code block and not prose that ate
/// the rest of the answer: everything after an open fence is drawn as code, its words
/// whole and its spacing kept, so a half-streamed block costs no raster pass, no
/// flicker, and no words (ADR-0008, kept by #52 when it replaced the plain rendering).
#[test]
fn a_fence_that_is_still_open_draws_the_rest_of_the_answer_as_code() {
    let rows = layout(
        &Entry::Assistant {
            turn: 1,
            text: SecretSet::default()
                .redact("Here is the rule:\n```\na fence still open\n    is a code block"),
        },
        40,
    );

    let drawn: Vec<String> = rows.iter().map(words).collect();
    assert_eq!(
        drawn,
        [
            "  Here is the rule:",
            "",
            "  ▏ a fence still open",
            "  ▏     is a code block",
        ],
        "the tail is drawn as code rather than swallowed as prose"
    );
    for row in &rows[2..] {
        assert_eq!(
            row.spans()[1],
            (String::from("▏ "), Look::plain(Ink::Dim)),
            "every row under an open fence is a code row: {row:?}"
        );
    }
}

/// ADR-0008's reason for existing, counted rather than described: a hundred fragments
/// into one live entry are a hundred layouts of that entry and none at all of the
/// entries above it. The settled transcript is laid out once to be drawn and never
/// again while the turn in flight moves under it, which is what "a streaming tick
/// re-renders exactly one entry" means as a number.
#[test]
fn a_live_entry_is_laid_out_alone_as_its_fragments_land() {
    let mut rows = Rows::new();
    for entry in transcript() {
        rows.push(entry);
    }
    rows.resize(60);

    // The settled picture first: the entries the band reaches, laid out to be drawn.
    rows.window(Scroll::Tail, 20);
    let settled = rows.lays();
    assert!(
        settled > 0,
        "the entries on screen were laid out to draw them"
    );
    let held = rows.len();

    let mut stream = Stream::new();
    for fragment in [
        "a fence ", "still ", "open ", "is ", "a ", "code ", "block. ",
    ]
    .repeat(15)
    {
        stream.text(fragment);
        rows.live(&stream, None);
        rows.window(Scroll::Tail, 20);
    }

    assert_eq!(
        stream.revision(),
        105,
        "a hundred and five fragments landed in the live entry"
    );
    assert_eq!(
        rows.len(),
        held + 1,
        "one live entry, however many fragments landed in it"
    );
    assert_eq!(
        rows.lays(),
        settled + 105,
        "one layout per fragment, and the settled entries never laid out again"
    );
}

/// A live entry is no fact (ADR-0007), so it is laid out from the stream while it is
/// live and dropped the moment the facts it became are pushed: the turn is drawn from
/// the entries it settled into, and its answer is never on the screen twice. The stream
/// behind it says nothing once the turn is over, and a frame that keeps asking it for
/// rows gets none.
#[test]
fn a_settled_turn_is_drawn_from_its_facts_and_never_twice() {
    let secrets = SecretSet::default();
    let mut rows = Rows::new();
    rows.push(turn(1));
    rows.resize(40);

    let mut stream = Stream::new();
    stream.text("a fence still open is a code block.");
    rows.live(&stream, None);
    let live = rows.window(Scroll::Tail, 20);
    assert_eq!(rows.len(), 2, "the turn and the live entry under it");
    assert!(
        !live.pieces().is_empty(),
        "the live entry is drawn while it is live"
    );

    // The turn ends: what it said is written down as the fact it is, and the live entry
    // goes with the same call.
    rows.push(Entry::Assistant {
        turn: 1,
        text: secrets.redact("a fence still open is a code block."),
    });
    let settled = rows.window(Scroll::Tail, 20);
    assert_eq!(rows.len(), 2, "the fact took the live entry's place");
    assert_eq!(
        rows_shown(&settled),
        2,
        "one row for the turn and one for the answer, and no third for a block that was never a fact"
    );

    // The stream is emptied when its turn ends (#43 hands the next turn a new one), and
    // an empty stream draws no live entry at all.
    rows.live(&Stream::new(), None);
    assert_eq!(
        rows.len(),
        2,
        "an empty stream adds nothing to the transcript"
    );
    assert_eq!(rows_shown(&rows.window(Scroll::Tail, 20)), 2);
}

/// The marker at the head of the newest answer is a clock, so the live entry it heads is
/// laid out again whenever the mark moves and at no other time: a step that draws the same
/// glyph and the same words costs nothing, and a step that changes them costs the one row
/// it heads (#39, ADR-0008).
#[test]
fn a_live_entry_is_laid_out_again_when_the_marker_moves() {
    let mut rows = Rows::new();
    rows.resize(40);
    let mut stream = Stream::new();
    stream.text("an answer arriving");
    rows.live(&stream, Some(&Pulse::live(4_200).head()));
    let laid = rows.lays();
    assert_eq!(laid, 1, "the live entry, laid out once to be drawn");

    rows.live(&stream, Some(&Pulse::live(4_200).head()));
    assert_eq!(
        rows.lays(),
        laid,
        "the same mark at the same time is the same picture"
    );

    // 80 ms on, the glyph has stepped: the width is the same and the row is not, so the
    // entry is laid out again rather than drawn with yesterday's mark on it.
    rows.live(&stream, Some(&Pulse::live(4_280).head()));
    assert_eq!(
        rows.lays(),
        laid + 1,
        "a moved marker is one layout of the live entry"
    );

    stream.text(" and settling");
    rows.live(&stream, Some(&Pulse::live(4_280).head()));
    assert_eq!(
        rows.lays(),
        laid + 2,
        "a fragment and a moved marker are one layout each, and never two for one"
    );
}

/// A run is live before it has said a word, and the marker is what says so: the answer's
/// row is opened by it, empty beneath it, because a frame that showed nothing at all for
/// the length of a request would be the one silence §12's pulse exists to break.
#[test]
fn a_live_run_opens_the_answers_row_before_it_has_said_anything() {
    let mut rows = Rows::new();
    rows.resize(40);
    rows.live(&Stream::new(), Some(&Pulse::live(0).head()));

    assert_eq!(rows.len(), 1, "the answer's row, opened by its marker");
    assert_eq!(rows_shown(&rows.window(Scroll::Tail, 20)), 1);
    assert_eq!(said(&painted(&rows.rows(0)[0], 40)), "⠋ 0.0s");

    // The mark goes when the run does, and the row it opened goes with it: no mark, no
    // live entry, and nothing left on the frame claiming a run is in flight.
    rows.live(&Stream::new(), None);
    assert_eq!(rows.len(), 0);
}

/// A call the model asked for: its name, and the command it wrote.
fn call(which: u32) -> Entry {
    Entry::Call {
        turn: which,
        name: String::from("bash"),
        arguments: SecretSet::default().redact(r#"{"command": "ls"}"#),
    }
}

/// What came back from a call: the verdict that ended it, and the output it printed.
fn result(which: u32, exit: Exit) -> Entry {
    Entry::Result {
        turn: which,
        name: String::from("bash"),
        text: SecretSet::default().redact("exit 0\na.txt\n"),
        bytes: 7,
        elapsed_ms: 2,
        exit: Some(exit),
    }
}

/// One row as a reader meets it: every cell of it, glyph and ink, painted through the
/// same `Row::paint` a frame is drawn with — so a test reads what the terminal would
/// and never a field the row happens to hold. The cells are the ones the row fills: a
/// blank past its end is the band's, not the row's.
fn painted(row: &Row, cols: u16) -> Vec<(char, Look)> {
    let area = Rect::new(0, 0, cols, 1);
    let mut buffer = Buffer::empty(area);
    row.paint(area, &mut buffer);
    (0..row.width())
        .map(|x| {
            let cell = buffer
                .cell((x, 0))
                .unwrap_or_else(|| panic!("column {x} is outside the row"));
            let look = Look::of(cell)
                .unwrap_or_else(|| panic!("column {x} is drawn in a style that is not a look"));
            let glyph = cell.symbol().chars().next().expect("one glyph a cell");
            (glyph, look)
        })
        .collect()
}

/// The words of a painted row, cut where the row stops saying anything.
fn said(row: &[(char, Look)]) -> String {
    row.iter()
        .map(|(glyph, _)| *glyph)
        .collect::<String>()
        .trim_end()
        .to_owned()
}

/// The ink every column of a painted row is in, so a test can say where the verdict's
/// ink stops and the faded ink of the facts beside it begins. Named apart from the
/// span-level `inks` above because the two read different things: one a laid-out row,
/// the other the cells that row was painted into.
fn cell_inks(row: &[(char, Look)]) -> Vec<Ink> {
    row.iter().map(|(_, look)| look.ink()).collect()
}

/// §12's first state: a call that exited zero settles on a check, the code it exited
/// with, and the call's name — the verdict in `ok` and everything beside it in the faded
/// ink a tool row is painted in.
#[test]
fn a_call_that_exited_zero_settles_on_a_check_and_its_code() {
    let rows = layout(&result(1, Exit::Code(0)), 40);
    let header = painted(&rows[0], 40);

    assert_eq!(said(&header), "✓ exit 0  bash  7 bytes · 2 ms");
    assert_eq!(cell_inks(&header)[0], Ink::Ok, "the check is the good ink");
    assert!(
        cell_inks(&header)[1..].iter().all(|ink| *ink == Ink::Dim),
        "and the facts beside it are faded: {:?}",
        cell_inks(&header)
    );
}

/// §12's second state: a call that exited with a code that is not zero settles on a
/// cross, and the code it says is the code the record holds.
#[test]
fn a_call_that_failed_settles_on_a_cross_and_the_code_it_exited_with() {
    let rows = layout(&result(1, Exit::Code(1)), 40);
    let header = painted(&rows[0], 40);

    assert_eq!(said(&header), "✗ exit 1  bash  7 bytes · 2 ms");
    assert_eq!(cell_inks(&header)[0], Ink::Err, "the cross is the bad ink");
}

/// §12's third state: the per-call timeout says so in words, because a command that was
/// killed never exited and has no code to print.
#[test]
fn a_call_the_timeout_killed_says_timed_out() {
    let header = painted(&layout(&result(1, Exit::TimedOut), 40)[0], 40);

    assert_eq!(said(&header), "✗ timed out  bash  7 bytes · 2 ms");
    assert_eq!(cell_inks(&header)[0], Ink::Err);
}

/// §12's fourth state and the reason it is one: a call the user's word stopped is a row
/// that says so, and not a spinner left turning for a result that will never come.
#[test]
fn a_cancelled_call_is_a_row_and_not_a_dangling_spinner() {
    let mut rows = Rows::new();
    rows.resize(80);
    rows.push(call(1));
    let in_flight = said(&painted(&rows.rows(0)[0], 80));
    assert_eq!(
        in_flight, "… bash",
        "a call that has not settled names itself and claims nothing"
    );

    rows.push(result(1, Exit::Cancelled));
    let settled = painted(&rows.rows(0)[0], 80);

    assert_eq!(
        said(&settled),
        "✗ cancelled  bash  7 bytes · 2 ms",
        "the row a cancelled call opened says what stopped it"
    );
    assert_eq!(cell_inks(&settled)[0], Ink::Err);
}

/// A turn that asks for two calls writes them as a wave — call, call, result, result —
/// and the rows still come out one per call, each settled by the result that answers it
/// and not by whichever one happened to land last.
#[test]
fn a_wave_of_calls_settles_its_rows_in_the_order_the_run_wrote_them() {
    let mut rows = Rows::new();
    rows.resize(80);
    rows.push(call(1));
    rows.push(call(1));
    rows.push(result(1, Exit::Code(0)));
    rows.push(result(1, Exit::Code(1)));

    assert_eq!(rows.len(), 2, "two calls, two rows");
    assert_eq!(
        said(&painted(&rows.rows(0)[0], 80)),
        "✓ exit 0  bash  7 bytes · 2 ms",
        "the first result settles the first call"
    );
    assert_eq!(
        said(&painted(&rows.rows(1)[0], 80)),
        "✗ exit 1  bash  7 bytes · 2 ms",
        "and the second settles the second"
    );
}

/// The cache's half of §12: a call and the result that answers it are one row, so a
/// finished call is not a second line under the one it opened.
#[test]
fn a_call_and_its_result_are_one_row() {
    let mut rows = Rows::new();
    rows.resize(80);
    rows.push(call(1));
    rows.push(result(1, Exit::Code(0)));

    assert_eq!(
        rows.len(),
        1,
        "the result settled the call instead of joining it"
    );
    assert_eq!(
        said(&painted(&rows.rows(0)[0], 80)),
        "✓ exit 0  bash  7 bytes · 2 ms"
    );
}

/// The row is updated when the call settles, and nothing above it is laid out again:
/// ADR-0008's streaming tick, which this time is a command finishing.
#[test]
fn settling_a_call_lays_that_entry_again_and_leaves_the_history_alone() {
    let mut rows = Rows::new();
    for which in 1..=3 {
        rows.push(turn(which));
        rows.push(answer(which));
    }
    rows.push(call(4));
    rows.resize(80);
    rows.window(Scroll::Tail, 40);
    let laid = rows.lays();

    // What the history was drawn as, entry by entry, before the call settled: every
    // entry but the last, which is the call this test is about.
    let settled_at = rows.len() - 1;
    let history: Vec<Vec<Row>> = (0..settled_at).map(|at| rows.rows(at).to_vec()).collect();
    assert_eq!(
        rows.lays(),
        laid,
        "reading back what was laid out costs nothing"
    );

    rows.push(result(4, Exit::Code(1)));
    rows.window(Scroll::Tail, 40);

    assert_eq!(
        rows.lays(),
        laid + 1,
        "one entry was laid again when the call settled"
    );
    for (at, was) in history.into_iter().enumerate() {
        assert_eq!(
            rows.rows(at),
            was.as_slice(),
            "entry {at} was laid out again, and nothing above the call changed"
        );
    }
    assert_eq!(
        said(&painted(&rows.rows(settled_at)[0], 80)),
        "✗ exit 1  bash  7 bytes · 2 ms",
        "and the row the call opened is the one that was laid again"
    );
    assert_eq!(
        rows.lays(),
        laid + 1,
        "reading the settled row back costs nothing either"
    );
}
