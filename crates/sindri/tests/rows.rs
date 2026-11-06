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

use sindri::rows::{Row, Rows, Scroll, Window, layout};
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
