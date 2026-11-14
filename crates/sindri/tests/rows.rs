//! The transcript's cache of rows, counted through its own `lays`: the promises here are about
//! how often layout happens, and a test that read the shape of the code instead would still
//! pass on the day the cost came back.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use sindri::graphics::CellSize;
use sindri::ink::{Ink, Look};
use sindri::link::Links;
use sindri::palette::{Band, Palette};
use sindri::pen;
use sindri::picture::Surface;
use sindri::pulse::Pulse;
use sindri::rows::{Row, Rows, Scroll, Window, layout, layout_on};
use sindri::size::Sizes;
use sindri::stream::Stream;
use sindri_core::config::Config;
use sindri_core::open::Ground;
use sindri_core::policy::{Boundary, Enforcement};
use sindri_core::provider::Deltas;
use sindri_core::records::Entry;
use sindri_core::redact::SecretSet;
use sindri_core::run::Exit;

fn turn(which: u32) -> Entry {
    Entry::Turn { turn: which }
}

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

fn transcript() -> Vec<Entry> {
    let mut entries = Vec::new();
    for which in 1..=3 {
        entries.push(turn(which));
        entries.push(answer(which));
    }
    entries
}

fn rows_shown(window: &Window) -> u16 {
    window.pieces().iter().map(|piece| piece.rows).sum()
}

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

    let again = rows.window(Scroll::Tail, 6);
    assert_eq!(again.pieces().len(), shown);
    assert_eq!(
        rows.lays(),
        shown,
        "a paint at an unchanged width re-lays nothing"
    );

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

    rows.resize(80);
    let restored = rows.window(Scroll::Tail, 6).pieces().len();
    assert_eq!(rows.lays(), shown + reflowed + restored);
}

#[test]
fn a_surface_that_changed_relays_the_band_once_and_one_that_did_not_relays_nothing() {
    let mut rows = Rows::new();
    rows.push(Entry::Assistant {
        turn: 1,
        text: SecretSet::default().redact("```mermaid\nflowchart TD\n  A-->B\n```"),
    });
    rows.resize(80);
    rows.window(Scroll::Tail, 10);

    let words = rows.rows(0).len();
    assert_eq!(
        rows.lays(),
        1,
        "the fence is laid out once, at the width the band draws it at"
    );
    assert!(
        rows.rows(0).iter().all(|row| row.picture().is_none()),
        "and a terminal that places no picture is given the fence as the code it is"
    );

    rows.retune(Surface::none());
    rows.window(Scroll::Tail, 10);
    assert_eq!(
        rows.lays(),
        1,
        "a surface that changed nothing drops no row and lays none out again"
    );

    rows.retune(Surface::of(None, CellSize::new(10, 20), true, false));
    rows.window(Scroll::Tail, 10);
    assert_eq!(
        rows.lays(),
        2,
        "a surface that changed drops what the cache held and lays the band out once"
    );

    let picture = rows
        .rows(0)
        .iter()
        .find_map(|row| row.picture())
        .map(|picture| picture.rows());
    let block = rows.rows(0).len();
    let picture = picture.expect("the fence is a diagram on a terminal that places pictures");
    assert!(
        block >= usize::from(picture),
        "and the rows laid are the diagram's block, as tall as the picture placed over it: \
         {block} rows of the answer, {picture} of the picture"
    );
    assert!(
        block >= words,
        "which is the taller of the two: the fence's own {words} rows are under the picture"
    );

    rows.rows(0);
    assert_eq!(
        rows.lays(),
        2,
        "and reading the rows back after it costs no third layout"
    );
}

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

    for back in (0..=260).rev() {
        rows.window(Scroll::Back(back), room);
    }
    assert_eq!(
        rows.lays(),
        history as usize,
        "scrolling a cached history costs no layout at all"
    );
}

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

#[test]
fn every_row_fits_the_width_it_was_laid_out_for() {
    let secrets = SecretSet::default();
    let entries = [
        Entry::Turn { turn: 1000 },
        Entry::Call {
            turn: 1,
            name: secrets.redact("a-tool-whose-name-is-longer-than-the-band-it-is-drawn-in"),
            arguments: secrets.redact(""),
        },
        Entry::Result {
            turn: 1,
            name: secrets.redact("bash"),
            text: secrets.redact("unbreakableandunforgivingwordthatnospacecansplit\n日本語の答え"),
            bytes: u64::MAX,
            elapsed_ms: u64::MAX,
            exit: Some(Exit::Code(u8::MAX.into())),
        },
        Entry::Assistant {
            turn: 1,
            text: secrets.redact(""),
        },
        Entry::Prompt {
            text: secrets.redact("a prompt whose words are longer than the band it is drawn in"),
        },
        Entry::Reasoning {
            turn: 1,
            text: secrets
                .redact("a thought, in its own gutter\nand its own ink\n```\nand a fence\n"),
        },
        answer(1),
    ];

    for cols in [2, 3, 5, 6, 7, 8, 40, 120] {
        for entry in &entries {
            for row in layout(entry, cols) {
                let drawn = row.width();
                assert!(
                    drawn <= cols,
                    "a row {drawn} columns wide was laid out for {cols}: {row:?}"
                );
            }
        }

        let head = Pulse::live(4_200).head();
        let mut stream = Stream::new();
        stream.text("a braille marker at the head of the newest answer, and its words");
        for row in stream.rows(cols, Some(&head)) {
            let drawn = row.width();
            assert!(
                drawn <= cols,
                "a row {drawn} columns wide was laid out for the marker's answer at {cols}: {row:?}"
            );
        }
    }
}

#[test]
fn the_readers_own_words_are_drawn_under_the_row_that_names_them() {
    let secrets = SecretSet::default();
    let rows = layout(
        &Entry::Prompt {
            text: secrets.redact("triage this"),
        },
        40,
    );

    assert!(
        words(&rows[0]).starts_with("─ prompt "),
        "the row that names the prompt says whose words these are: {:?}",
        rows[0]
    );
    assert_eq!(rows[0].width(), 40, "and runs to the band's width");
    assert_eq!(
        words(&rows[1]),
        "› triage this",
        "the words open with the prompt's sigil in the gutter an answer is set from"
    );
    assert_eq!(inks(&rows[1]), [Ink::Plain, Ink::Plain]);
}

/// The column a row's words begin in: the first glyph of it that is a letter, a digit or a bullet,
/// and nothing for a row that says no word at all.
fn words_column(row: &Row) -> Option<u16> {
    row.glyph_columns()
        .into_iter()
        .find(|(_, glyph, _)| glyph.is_alphanumeric() || *glyph == '•')
        .map(|(column, _, _)| column)
}

fn every_kind(secrets: &SecretSet) -> Vec<Entry> {
    vec![
        turn(5),
        Entry::Prompt {
            text: secrets.redact("triage the retry test"),
        },
        Entry::Call {
            turn: 1,
            name: secrets.redact("bash"),
            arguments: secrets.redact("ls -la"),
        },
        result(1, Exit::Code(0)),
        Entry::Reasoning {
            turn: 2,
            text: secrets.redact("the fence is the question"),
        },
        Entry::Assistant {
            turn: 2,
            text: secrets
                .redact("Here is the rule:\n```rust\nfn main() {\n}\n```\nand the prose goes on."),
        },
    ]
}

#[test]
fn every_entrys_words_begin_in_one_column() {
    let secrets = SecretSet::default();
    for cols in [120, 80, 40, 20, 8, 3] {
        for entry in every_kind(&secrets) {
            let mut said = 0;
            for row in layout(&entry, cols) {
                let Some(column) = words_column(&row) else {
                    continue;
                };
                said += 1;
                assert_eq!(
                    column, 2,
                    "a row of {entry:?} at {cols} columns begins its words at {column}: {row:?}"
                );
            }
            assert!(said > 0, "{entry:?} at {cols} columns says no word at all");
        }
    }
}

/// The glyph a row draws at a column of it.
fn drawn_at(row: &Row, column: u16) -> Option<char> {
    row.glyph_columns()
        .into_iter()
        .find(|(at, _, _)| *at == column)
        .map(|(_, glyph, _)| glyph)
}

#[test]
fn the_marker_that_says_what_a_row_is_stands_in_the_gutter() {
    let secrets = SecretSet::default();
    let turn_row = layout(&turn(5), 40);
    let prompt = layout(
        &Entry::Prompt {
            text: secrets.redact("triage this"),
        },
        40,
    );
    let flight = layout(&call(1), 60);
    let ok = layout(&result(1, Exit::Code(0)), 60);
    let failed = layout(&result(1, Exit::Code(1)), 60);
    let thought = layout(
        &Entry::Reasoning {
            turn: 1,
            text: secrets.redact("a thought"),
        },
        60,
    );
    let answer = layout(
        &Entry::Assistant {
            turn: 2,
            text: secrets.redact("prose\n```rust\nfn main()\n```\n"),
        },
        60,
    );
    let block = answer
        .iter()
        .find(|row| drawn_at(row, 0) == Some('▏'))
        .expect("the fence is rows of its own");

    let marked = [
        (&turn_row[0], '─'),
        (&prompt[0], '─'),
        (&prompt[1], '›'),
        (&flight[0], '…'),
        (&ok[0], '✓'),
        (&failed[0], '✗'),
        (&thought[0], '│'),
        (&answer[0], ' '),
        (block, '▏'),
    ];
    for (row, marker) in marked {
        assert_eq!(
            drawn_at(row, 0),
            Some(marker),
            "the marker {marker:?} is not the row's first column: {row:?}"
        );
        assert_eq!(
            drawn_at(row, 1),
            Some(' '),
            "the gutter is two cells, one glyph and one blank: {row:?}"
        );
        assert_eq!(
            words_column(row),
            Some(2),
            "and the words begin past it: {row:?}"
        );
    }
}

#[test]
fn a_row_too_narrow_for_a_marker_gives_up_the_marker_before_it_cuts_a_word() {
    let secrets = SecretSet::default();
    let thought = Entry::Reasoning {
        turn: 1,
        text: secrets.redact("thought"),
    };
    for (entry, marker) in [
        (result(1, Exit::Code(0)), '✓'),
        (result(1, Exit::Code(1)), '✗'),
        (call(1), '…'),
        (thought, '│'),
        (turn(5), '─'),
        (
            Entry::Prompt {
                text: secrets.redact("prompt"),
            },
            '─',
        ),
        (
            Entry::Assistant {
                turn: 1,
                text: secrets.redact("```rust\nfn main()\n```"),
            },
            '▏',
        ),
    ] {
        for row in layout(&entry, 2) {
            let Some((column, _, _)) = row.glyph_columns().first().copied() else {
                continue;
            };
            assert_eq!(column, 0, "a band of two columns holds no gutter: {row:?}");
            assert!(
                !row.glyphs().contains(marker),
                "the marker {marker:?} is given up rather than the words: {row:?}"
            );
        }

        for row in layout(&entry, 2) {
            assert!(
                row.width() <= 2,
                "a row of two columns holds no word past them: {row:?}"
            );
        }

        let narrow = layout(&entry, 3);
        let head = &narrow[0];
        assert_eq!(
            drawn_at(head, 0),
            Some(marker),
            "three columns hold the marker: {head:?}"
        );
        assert_eq!(drawn_at(head, 1), Some(' '), "and its blank: {head:?}");
        assert_eq!(
            head.width(),
            3,
            "with one column of words at 2 and no more: {head:?}"
        );
        assert_eq!(
            words_column(head),
            Some(2),
            "the words follow the marker: {head:?}"
        );
    }
}

#[test]
fn a_marker_no_band_holds_beside_a_word_leaves_the_answer_its_columns() {
    let head = Pulse::live(4_200).head();
    let mut stream = Stream::new();
    stream.text("the answer's own words");

    for cols in 6..=7 {
        let marked: Vec<String> = stream.rows(cols, Some(&head)).iter().map(words).collect();
        assert_eq!(
            marked,
            stream
                .rows(cols, None)
                .iter()
                .map(words)
                .collect::<Vec<String>>(),
            "a marker {cols} columns cannot hold beside a word of the answer takes no column of it"
        );
    }
}

#[test]
fn a_code_block_keeps_its_bar_in_the_gutter() {
    let rows = layout(
        &Entry::Assistant {
            turn: 1,
            text: SecretSet::default().redact(
                "The rule is one line:\n```rust\nfn main() {\nlet x = 1;\n}\n```\nand back to prose.",
            ),
        },
        40,
    );

    assert_eq!(
        rows.iter().map(words).collect::<Vec<String>>(),
        [
            "  The rule is one line:",
            "",
            "▏ rust",
            "▏ fn main() {",
            "▏ let x = 1;",
            "▏ }",
            "",
            "  and back to prose.",
        ],
        "the bar of a block stands in the answer's gutter and never past it"
    );
    for row in &rows[2..6] {
        assert_eq!(drawn_at(row, 0), Some('▏'), "the block's own bar: {row:?}");
        assert_eq!(drawn_at(row, 1), Some(' '), "and its blank: {row:?}");
        assert!(
            drawn_at(row, 2).is_some(),
            "so the code's words begin where the answer's do: {row:?}"
        );
    }
    assert_eq!(
        words_column(&rows[7]),
        Some(2),
        "and the prose the fence closed under begins there too: {:?}",
        rows[7]
    );
}

#[test]
fn a_thought_keeps_its_bar_in_the_gutter_and_its_own_ink() {
    let rows = layout(
        &Entry::Reasoning {
            turn: 1,
            text: SecretSet::default()
                .redact("the fence is the question: a fence still open is code"),
        },
        24,
    );

    assert!(rows.len() > 1, "a thought wider than the band is rows");
    for row in &rows {
        assert_eq!(drawn_at(row, 0), Some('│'), "the bar of a thought: {row:?}");
        assert_eq!(drawn_at(row, 1), Some(' '), "and its blank: {row:?}");
        assert_eq!(
            words_column(row),
            Some(2),
            "the thought's words stand behind the bar and not past it: {row:?}"
        );
        assert_eq!(
            inks(row),
            [Ink::Dim, Ink::Muted],
            "the bar is dim and the words it holds are muted: {row:?}"
        );
    }
}

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
            name: secrets.redact("bash"),
            arguments: secrets.redact(""),
        },
    ] {
        let rows: Vec<Row> = layout(&entry, 4);
        assert!(!rows.is_empty(), "{entry:?} laid out as no row at all");
    }
}

fn words(row: &Row) -> String {
    row.spans().iter().map(|(text, _)| text.as_str()).collect()
}

fn inks(row: &Row) -> Vec<Ink> {
    row.spans().iter().map(|(_, look)| look.ink()).collect()
}

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

    assert_eq!(
        pen::cells(&thought[0].spans()[0].0),
        2,
        "a thought's gutter is the width of an answer's"
    );
}

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
            "▏ rust",
            "▏ fn main() {",
            "▏     let  x = 1;",
            "▏ }",
            "",
            "  and the prose goes on.",
        ],
        "the tag stands over the code, the code is the code that was written, and the \
         answer's own prose resumes after the closing fence"
    );
    assert_eq!(
        inks(&rows[2]),
        [Ink::Dim, Ink::Dim],
        "the bar and the language tag are facts about what the text is: {:?}",
        rows[2]
    );
    assert_eq!(
        (drawn_at(&rows[3], 0), words_column(&rows[3])),
        (Some('▏'), Some(2)),
        "every row of the block keeps the block's bar in the answer's gutter: {:?}",
        rows[3]
    );
    assert_eq!(
        inks(&rows[7]),
        [Ink::Plain, Ink::Plain],
        "and the prose after the fence is the answer's own ink again: {:?}",
        rows[7]
    );
}

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
            "▏ a fence still open",
            "▏     is a code block",
        ],
        "the tail is drawn as code rather than swallowed as prose"
    );
    for row in &rows[2..] {
        assert_eq!(
            row.spans()[0],
            (String::from("▏ "), Look::plain(Ink::Dim)),
            "every row under an open fence is a code row: {row:?}"
        );
    }
}

#[test]
fn a_live_entry_is_laid_out_alone_as_its_fragments_land() {
    let mut rows = Rows::new();
    for entry in transcript() {
        rows.push(entry);
    }
    rows.resize(60);

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

    rows.live(&Stream::new(), None);
    assert_eq!(
        rows.len(),
        2,
        "an empty stream adds nothing to the transcript"
    );
    assert_eq!(rows_shown(&rows.window(Scroll::Tail, 20)), 2);
}

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

#[test]
fn a_live_run_opens_the_answers_row_before_it_has_said_anything() {
    let mut rows = Rows::new();
    rows.resize(40);
    rows.live(&Stream::new(), Some(&Pulse::live(0).head()));

    assert_eq!(rows.len(), 1, "the answer's row, opened by its marker");
    assert_eq!(rows_shown(&rows.window(Scroll::Tail, 20)), 1);
    assert_eq!(said(&painted(&rows.rows(0)[0], 40)), "⠋ 0.0s");

    rows.live(&Stream::new(), None);
    assert_eq!(rows.len(), 0);
}

fn call(which: u32) -> Entry {
    Entry::Call {
        turn: which,
        name: SecretSet::default().redact("bash"),
        arguments: SecretSet::default().redact(r#"{"command": "ls"}"#),
    }
}

fn result(which: u32, exit: Exit) -> Entry {
    Entry::Result {
        turn: which,
        name: SecretSet::default().redact("bash"),
        text: SecretSet::default().redact("exit 0\na.txt\n"),
        bytes: 7,
        elapsed_ms: 2,
        exit: Some(exit),
    }
}

fn named(which: u32, name: &str, arguments: &str) -> Entry {
    Entry::Call {
        turn: which,
        name: SecretSet::default().redact(name),
        arguments: SecretSet::default().redact(arguments),
    }
}

/// The ground a run standing in `workspace` judges a target from.
fn ground(workspace: &std::path::Path) -> Ground {
    let boundary = Boundary::from_config(&Config::default(), workspace, Enforcement::Bubblewrap)
        .expect("the boundary holds");
    Ground::new(&boundary, workspace, workspace).expect("the ground resolves")
}

/// The line a way's words are drawn under.
fn way() -> Look {
    Look::plain(Ink::Plain).underlined(Ink::Dim)
}

#[test]
fn a_path_a_call_names_is_a_way_to_the_file_the_run_judged_it_openable_as() {
    let tree = tempfile::tempdir().expect("a temp workspace");
    std::fs::write(tree.path().join("notes.html"), "body").expect("the file");
    let entry = named(1, "open", r#"{"path":"notes.html"}"#);

    let rows = layout_on(&entry, 40, &Surface::none(), Some(&ground(tree.path())));

    assert_eq!(
        said(&painted(&rows[1], 40)),
        r#"  {"path":"notes.html"}"#,
        "the call's arguments are laid as they were written, set in a body's gutter"
    );
    assert!(
        painted(&rows[1], 40)[11..21]
            .iter()
            .all(|(_, look)| *look == way()),
        "the path's own cells wear the line a way wears: {:?}",
        painted(&rows[1], 40)
    );

    let resolved = std::fs::canonicalize(tree.path().join("notes.html")).expect("the file");
    let destination = format!("file://{}", resolved.display());
    let (_, links) = laid(&rows[1], Rect::new(0, 0, 40, 1));
    assert_eq!(
        links.at(0, 11),
        Some(destination.as_str()),
        "the path's first cell opens the file the judgement resolved it to"
    );
    assert_eq!(
        links.at(0, 20),
        Some(destination.as_str()),
        "and so does its last"
    );
    assert_eq!(
        links.at(0, 21),
        None,
        "and no cell past the path is a way to the file"
    );
    assert_eq!(
        links.at(0, 10),
        None,
        "nor the quote the path was written after"
    );
}

#[test]
fn a_path_wider_than_the_body_is_one_destination_on_each_row() {
    let tree = tempfile::tempdir().expect("a temp workspace");
    let long = "a-note-on-the-ways-a-row-is-drawn-and-the-file-it-names.html";
    std::fs::write(tree.path().join(long), "body").expect("the file");
    let entry = named(1, "open", &format!(r#"{{"path":"{long}"}}"#));

    let rows = layout_on(&entry, 30, &Surface::none(), Some(&ground(tree.path())));

    assert!(
        rows.len() > 2,
        "a path wider than the body is broken over two rows: {:?}",
        rows.iter().map(Row::glyphs).collect::<Vec<String>>()
    );
    let resolved = std::fs::canonicalize(tree.path().join(long)).expect("the file");
    let destination = format!("file://{}", resolved.display());
    let area = Rect::new(0, 0, 30, u16::try_from(rows.len()).unwrap());
    let mut buffer = Buffer::empty(area);
    let mut links = Links::none();
    for (at, row) in rows[1..].iter().enumerate() {
        row.paint(
            Rect {
                y: u16::try_from(at).unwrap(),
                height: 1,
                ..area
            },
            &mut buffer,
            &Palette::TRUECOLOR,
            &mut links,
            &mut Sizes::none(),
        );
    }
    let destinations: Vec<&str> = (0..u16::try_from(rows.len() - 1).unwrap())
        .filter_map(|y| links.at(y, 4))
        .collect();
    assert!(
        destinations.len() > 1,
        "the path is drawn over more than one row: {destinations:?}"
    );
    assert!(
        destinations
            .iter()
            .all(|said| *said == destination.as_str()),
        "and each row it was broken over carries the whole destination, never half of it: \
         {destinations:?}"
    );
}

#[test]
fn the_words_a_way_is_drawn_on_are_the_value_the_run_read_the_path_from() {
    let tree = tempfile::tempdir().expect("a temp workspace");
    std::fs::write(tree.path().join("notes.html"), "body").expect("the file");
    let entry = named(1, "open", r#"{"note": "notes.html", "path": "notes.html"}"#);

    let rows = layout_on(&entry, 50, &Surface::none(), Some(&ground(tree.path())));

    let resolved = std::fs::canonicalize(tree.path().join("notes.html")).expect("the file");
    let destination = format!("file://{}", resolved.display());
    let (_, links) = laid(&rows[1], Rect::new(0, 0, 50, 1));
    assert_eq!(
        links.at(0, 12),
        None,
        "a value that spells the path is no way to it: only the path the run read is"
    );
    assert_eq!(
        links.at(0, 34),
        Some(destination.as_str()),
        "and the path's own value is the way there"
    );
    assert_eq!(
        links.at(0, 43),
        Some(destination.as_str()),
        "to its last cell"
    );
}

#[test]
fn a_path_a_call_names_that_the_run_would_have_refused_is_drawn_as_it_is_today() {
    let tree = tempfile::tempdir().expect("a temp workspace");
    let entry = named(1, "open", r#"{"path":"outside.html"}"#);

    let judged = layout_on(&entry, 40, &Surface::none(), Some(&ground(tree.path())));
    let plain = layout(&entry, 40);

    assert_eq!(
        painted(&judged[1], 40),
        painted(&plain[1], 40),
        "a file the run would have refused is drawn exactly as the frame draws it with no \
         judgement at all: the words and no line under them"
    );
    let (_, links) = laid(&judged[1], Rect::new(0, 0, 40, 1));
    assert!(
        links.is_empty(),
        "and no cell of the row is a way to anywhere"
    );
}

fn painted(row: &Row, cols: u16) -> Vec<(char, Look)> {
    let palette = Palette::TRUECOLOR;
    let area = Rect::new(0, 0, cols, 1);
    let mut buffer = Buffer::empty(area);
    row.paint(
        area,
        &mut buffer,
        &palette,
        &mut Links::none(),
        &mut Sizes::none(),
    );
    (0..row.width())
        .map(|x| {
            let cell = buffer
                .cell((x, 0))
                .unwrap_or_else(|| panic!("column {x} is outside the row"));
            let look = Look::of(cell, &palette)
                .unwrap_or_else(|| panic!("column {x} is drawn in a style that is not a look"));
            let glyph = cell.symbol().chars().next().expect("one glyph a cell");
            (glyph, look)
        })
        .collect()
}

/// A row as a terminal that sizes text draws it: the cells the runs it names leave, written once the
/// band it stands in is known, which is when the frame writes them.
fn spelled(row: &Row, cols: u16) -> String {
    let band = Rect::new(0, 0, cols, 4);
    let mut buffer = Buffer::empty(band);
    let mut sizes = Sizes::none();
    row.paint(
        Rect { height: 1, ..band },
        &mut buffer,
        &Palette::TRUECOLOR,
        &mut Links::none(),
        &mut sizes,
    );
    sizes.settle(band, &mut buffer, &Palette::TRUECOLOR);
    (0..row.width())
        .map(|x| {
            buffer
                .cell((x, 0))
                .expect("a cell of the row")
                .symbol()
                .to_owned()
        })
        .collect::<String>()
        .trim_end()
        .to_owned()
}

#[test]
fn a_live_answer_that_opens_on_a_large_heading_keeps_the_cells_the_sizing_leaves() {
    let mut rows = Rows::new();
    rows.resize(40);
    rows.retune(Surface::of(None, CellSize::new(10, 20), false, true));
    let mut stream = Stream::new();
    stream.text("# A gate rule");
    rows.live(&stream, Some(&Pulse::live(4_200).head()));

    let heading = rows.rows(0)[1].clone();
    assert_eq!(
        spelled(&heading, 40),
        "  A   g a t e   r u l e",
        "the heading under the marker's row is drawn at twice the cell: {heading:?}"
    );
    assert_eq!(
        heading.glyphs().trim_start(),
        "A gate rule",
        "and the row is copied as its words: {heading:?}"
    );
}

fn said(row: &[(char, Look)]) -> String {
    row.iter()
        .map(|(glyph, _)| *glyph)
        .collect::<String>()
        .trim_end()
        .to_owned()
}

/// The ink of every column of a painted row, where `inks` reads the spans the row was laid out as.
fn cell_inks(row: &[(char, Look)]) -> Vec<Ink> {
    row.iter().map(|(_, look)| look.ink()).collect()
}

#[test]
fn a_call_that_exited_zero_settles_on_a_check_and_its_code() {
    let rows = layout(&result(1, Exit::Code(0)), 40);
    let header = painted(&rows[0], 40);

    assert_eq!(said(&header), "✓ exit 0  bash  7 bytes · 2 ms");
    assert_eq!(cell_inks(&header)[0], Ink::Ok, "the check is the good ink");
    assert!(
        cell_inks(&header)[2..].iter().all(|ink| *ink == Ink::Dim),
        "and the facts beside it are faded: {:?}",
        cell_inks(&header)
    );
}

#[test]
fn a_call_that_failed_settles_on_a_cross_and_the_code_it_exited_with() {
    let rows = layout(&result(1, Exit::Code(1)), 40);
    let header = painted(&rows[0], 40);

    assert_eq!(said(&header), "✗ exit 1  bash  7 bytes · 2 ms");
    assert_eq!(cell_inks(&header)[0], Ink::Err, "the cross is the bad ink");
}

#[test]
fn a_call_the_timeout_killed_says_timed_out() {
    let header = painted(&layout(&result(1, Exit::TimedOut), 40)[0], 40);

    assert_eq!(said(&header), "✗ timed out  bash  7 bytes · 2 ms");
    assert_eq!(cell_inks(&header)[0], Ink::Err);
}

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

/// A live answer's first row is the marker's and the answer's together, and a fence that opens the
/// answer is the well's from that row on: a marker beside a tag is no hole in the panel.
#[test]
fn a_fence_that_opens_a_live_answer_keeps_its_well_under_the_marker() {
    let mut rows = Rows::new();
    rows.resize(40);
    let mut stream = Stream::new();
    stream.text("```rust\nfn main() {\n");
    rows.live(&stream, Some(&Pulse::live(4_200).head()));

    let laid: Vec<Option<Band>> = rows.rows(0).iter().map(Row::band).collect();
    assert!(
        laid.len() > 1 && laid.iter().all(|band| *band == Some(Band::Well)),
        "the fence opens the answer, so every row of it sits in the well: {laid:?}"
    );
}

/// A row is painted at the frame's own row and column, and what it names is laid there beside it.
fn laid(row: &Row, area: Rect) -> (Buffer, Links) {
    let mut buffer = Buffer::empty(area);
    let mut links = Links::none();
    row.paint(
        area,
        &mut buffer,
        &Palette::TRUECOLOR,
        &mut links,
        &mut Sizes::none(),
    );
    (buffer, links)
}

#[test]
fn a_run_of_a_row_that_names_a_destination_is_drawn_word_for_word_and_names_it_beside_the_frame() {
    let row = Row::of([
        (String::from("  "), Ink::Plain),
        (String::from("the spec"), Ink::Muted),
    ])
    .linking(2, 8, "https://example.test/spec");

    let (_, links) = laid(&row, Rect::new(0, 3, 40, 1));

    assert_eq!(
        links.at(3, 2),
        Some("https://example.test/spec"),
        "the run's first cell names the destination the frame writes before it"
    );
    assert_eq!(
        links.at(3, 9),
        Some("https://example.test/spec"),
        "and so does its last"
    );
    assert_eq!(links.at(3, 10), None, "no cell past the run is a way there");
    assert_eq!(
        links.at(3, 1),
        None,
        "and the gutter before it is no way to anywhere"
    );
    assert_eq!(
        row.width(),
        10,
        "the destination spent no column: the row is as wide as its words"
    );
}

#[test]
fn a_row_that_names_no_destination_is_drawn_as_it_was_and_lays_nothing() {
    let row = Row::of([
        (String::from("  "), Ink::Plain),
        (String::from("keys"), Ink::Plain),
    ]);

    let (buffer, links) = laid(&row, Rect::new(0, 0, 40, 1));

    assert!(
        links.is_empty(),
        "a row that names nothing leaves the frame no way to anywhere"
    );
    assert_eq!(
        buffer.cell((2, 0)).expect("a cell of the row").symbol(),
        "k",
        "and its words are drawn where they always were"
    );
}

#[test]
fn a_run_wider_than_the_row_it_was_laid_on_names_nothing_on_the_row_below() {
    let area = Rect::new(0, 0, 20, 2);
    let mut buffer = Buffer::empty(area);
    let mut links = Links::none();
    let row = Row::of([(String::from("the spec"), Ink::Plain)]).linking(0, 80, "https://x.test/s");
    row.paint(
        Rect { height: 1, ..area },
        &mut buffer,
        &Palette::TRUECOLOR,
        &mut links,
        &mut Sizes::none(),
    );
    Row::of([(String::from("and its gate"), Ink::Plain)]).paint(
        Rect {
            y: 1,
            height: 1,
            ..area
        },
        &mut buffer,
        &Palette::TRUECOLOR,
        &mut links,
        &mut Sizes::none(),
    );

    assert_eq!(links.at(0, 18), Some("https://x.test/s"));
    assert_eq!(
        links.at(0, 19),
        Some("https://x.test/s"),
        "the run covers every cell of the row it was laid on, to its last"
    );
    assert_eq!(
        links.at(1, 0),
        None,
        "and no cell of the row below it is a way anywhere: a run is one row's, and a wrap is \
         the next row's own name"
    );
}

#[test]
fn a_row_set_under_a_gutter_moves_the_run_it_names_with_its_words() {
    let row = Row::of([(String::from("the spec"), Ink::Plain)])
        .linking(0, 8, "https://example.test/spec")
        .indented("│ ", Ink::Dim);

    let (buffer, links) = laid(&row, Rect::new(0, 0, 40, 1));

    assert_eq!(
        buffer.cell((0, 0)).expect("a cell of the row").symbol(),
        "│",
        "the gutter is the row's first column"
    );
    assert_eq!(
        links.at(0, 2),
        Some("https://example.test/spec"),
        "and the way begins where the words it covers do"
    );
    assert_eq!(
        links.at(0, 1),
        None,
        "never on the gutter the words were moved past"
    );
}
