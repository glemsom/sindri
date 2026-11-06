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

use sindri::ink::Ink;
use sindri::rows::{Row, Rows, Scroll, Window, layout};
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
fn painted(row: &Row, cols: u16) -> Vec<(char, Ink)> {
    let area = Rect::new(0, 0, cols, 1);
    let mut buffer = Buffer::empty(area);
    row.paint(area, &mut buffer);
    (0..row.width())
        .map(|x| {
            let cell = buffer
                .cell((x, 0))
                .unwrap_or_else(|| panic!("column {x} is outside the row"));
            let ink = Ink::of(cell)
                .unwrap_or_else(|| panic!("column {x} is drawn in a style that is not an ink"));
            let glyph = cell.symbol().chars().next().expect("one glyph a cell");
            (glyph, ink)
        })
        .collect()
}

/// The words of a painted row, cut where the row stops saying anything.
fn said(row: &[(char, Ink)]) -> String {
    row.iter()
        .map(|(glyph, _)| *glyph)
        .collect::<String>()
        .trim_end()
        .to_owned()
}

/// The ink every column of a painted row is in, so a test can say where the verdict's
/// ink stops and the faded ink of the facts beside it begins.
fn inks(row: &[(char, Ink)]) -> Vec<Ink> {
    row.iter().map(|(_, ink)| *ink).collect()
}

/// §12's first state: a call that exited zero settles on a check, the code it exited
/// with, and the call's name — the verdict in `ok` and everything beside it in the faded
/// ink a tool row is painted in.
#[test]
fn a_call_that_exited_zero_settles_on_a_check_and_its_code() {
    let rows = layout(&result(1, Exit::Code(0)), 40);
    let header = painted(&rows[0], 40);

    assert_eq!(said(&header), "✓ exit 0  bash  7 bytes · 2 ms");
    assert_eq!(inks(&header)[0], Ink::Ok, "the check is the good ink");
    assert!(
        inks(&header)[1..].iter().all(|ink| *ink == Ink::Dim),
        "and the facts beside it are faded: {:?}",
        inks(&header)
    );
}

/// §12's second state: a call that exited with a code that is not zero settles on a
/// cross, and the code it says is the code the record holds.
#[test]
fn a_call_that_failed_settles_on_a_cross_and_the_code_it_exited_with() {
    let rows = layout(&result(1, Exit::Code(1)), 40);
    let header = painted(&rows[0], 40);

    assert_eq!(said(&header), "✗ exit 1  bash  7 bytes · 2 ms");
    assert_eq!(inks(&header)[0], Ink::Err, "the cross is the bad ink");
}

/// §12's third state: the per-call timeout says so in words, because a command that was
/// killed never exited and has no code to print.
#[test]
fn a_call_the_timeout_killed_says_timed_out() {
    let header = painted(&layout(&result(1, Exit::TimedOut), 40)[0], 40);

    assert_eq!(said(&header), "✗ timed out  bash  7 bytes · 2 ms");
    assert_eq!(inks(&header)[0], Ink::Err);
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
    assert_eq!(inks(&settled)[0], Ink::Err);
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
