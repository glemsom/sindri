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

use sindri::ink::Ink;
use sindri::rows::{Row, Rows, Scroll, Window, layout};
use sindri::stream::Stream;
use sindri_core::provider::Deltas;
use sindri_core::records::Entry;
use sindri_core::redact::SecretSet;

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
    row.spans().iter().map(|(_, ink)| *ink).collect()
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
            Some(&(String::from("│ "), Ink::Dim)),
            "a thought opens with its own rule: {row:?}"
        );
        assert!(
            spans[1..].iter().all(|(_, ink)| *ink == Ink::Muted),
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

/// ADR-0008's one markdown rule, the rule #52 inherits when it replaces the plain
/// rendering: a fenced block is code until its closing fence arrives. Inside the fence
/// a line keeps its own spacing and is broken at the band's edge and nowhere else, so
/// what is drawn is the code the model wrote and not the prose a word-wrapper would
/// have made of it.
#[test]
fn a_fenced_block_is_drawn_as_code_and_the_prose_resumes_after_it() {
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
            "  ```rust",
            "  fn main() {",
            "      let  x = 1;",
            "  }",
            "  ```",
            "  and the prose goes on.",
        ],
        "the fence is drawn, the code between it is the code that was written, and the \
         answer's own prose resumes after the closing fence"
    );
    assert_eq!(
        inks(&rows[1]),
        [Ink::Plain, Ink::Dim],
        "the fence line is a fact about what the text is, and is not read as prose"
    );
    assert_eq!(
        inks(&rows[3]),
        [Ink::Plain, Ink::Muted],
        "the code inside the fence is drawn in the ink code is read in: {:?}",
        rows[3]
    );
    assert_eq!(
        inks(&rows[6]),
        [Ink::Plain, Ink::Plain],
        "and the prose after the fence is the answer's own ink again: {:?}",
        rows[6]
    );
}

/// A fence whose closing line has not landed yet is a code block and not prose that ate
/// the rest of the answer: everything after an open fence is drawn as code, its words
/// whole and its spacing kept, so a half-streamed block costs no raster pass, no
/// flicker, and no words (ADR-0008).
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
            "  ```",
            "  a fence still open",
            "      is a code block",
        ],
        "the tail is drawn as code rather than swallowed as prose"
    );
    for row in &rows[2..] {
        assert_eq!(
            inks(row),
            [Ink::Plain, Ink::Muted],
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
        rows.live(&stream);
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
    rows.live(&stream);
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
    rows.live(&Stream::new());
    assert_eq!(
        rows.len(),
        2,
        "an empty stream adds nothing to the transcript"
    );
    assert_eq!(rows_shown(&rows.window(Scroll::Tail, 20)), 2);
}
