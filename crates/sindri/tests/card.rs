//! The card every screen is drawn on: the rounded border and the name in its top row, the key that
//! leaves in its bottom row, the frame behind it painted and dimmed, and the geometry a card takes
//! at a frame too narrow to hold one.

use std::collections::HashSet;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use sindri::composer::Key;
use sindri::ink::Ink;
use sindri::keys::{Act, Mode};
use sindri::palette::Palette;
use sindri::view::View;
use sindri_core::config::Config;
use sindri_core::records::Entry;
use sindri_core::redact::SecretSet;

const SCREENS: [Mode; 5] = [
    Mode::Keys,
    Mode::Files,
    Mode::Models,
    Mode::Settings,
    Mode::Stats,
];

/// The name a screen wears in the border of the card it is drawn on.
fn name(mode: Mode) -> &'static str {
    mode.title().expect("a screen has a name to wear")
}

/// The key the screen's own table binds to leaving it.
fn leaves(mode: Mode) -> &'static str {
    mode.table()
        .iter()
        .find(|binding| binding.act == Act::Close)
        .map(|binding| binding.name)
        .expect("a screen binds one key to leaving it")
}

/// `over` with the screen `mode` opened on it, through the key a reader sends.
fn opened(over: View, mode: Mode) -> View {
    let root = tempfile::tempdir().expect("a temp workspace");
    std::fs::write(root.path().join("rows.rs"), "").expect("a file");
    let mut view = over;
    match mode {
        Mode::Keys => {
            view.key(Key::Char('?'));
        }
        Mode::Files => {
            view.set_workspace(root.path().to_path_buf());
            view.key(Key::Char('@'));
        }
        Mode::Models => {
            view.paste("/model");
            view.key(Key::Enter);
        }
        Mode::Settings => view.open_settings(&Config::default()),
        Mode::Stats => {
            view.paste("/stats");
            view.key(Key::Enter);
        }
        Mode::Composer => panic!("a draft is not a screen"),
    }
    assert_eq!(
        view.mode(),
        mode,
        "the screen a test asked for is the one up"
    );
    view
}

fn on_screen(mode: Mode) -> View {
    opened(conversation(), mode)
}

fn painted(view: &mut View, cols: u16, rows: u16) -> Buffer {
    let mut terminal =
        Terminal::new(TestBackend::new(cols, rows)).expect("a headless screen of that size");
    terminal
        .draw(|frame| view.paint(frame.area(), frame.buffer_mut()))
        .expect("the frame is painted");
    terminal.backend().buffer().clone()
}

/// A frame with a run in it and no draft, so every screen opens on its own key.
fn conversation() -> View {
    let entries = [
        Entry::Turn { turn: 1 },
        Entry::Assistant {
            turn: 1,
            text: SecretSet::default().redact(
                "The band the transcript is drawn in holds the run\nand the \ncard is opened over it",
            ),
        },
    ];
    View::of(entries, sindri::rows::Scroll::Tail)
}

/// The same frame with a draft typed into it, so the composer band is behind the card too.
fn drafted() -> View {
    let mut view = conversation();
    view.paste("a draft behind the card");
    view
}

fn row(buffer: &Buffer, y: u16) -> String {
    let area = buffer.area;
    (area.x..area.right())
        .map(|x| buffer.cell((x, y)).expect("a cell of the row").symbol())
        .collect()
}

fn rows(buffer: &Buffer) -> Vec<String> {
    (buffer.area.y..buffer.area.bottom())
        .map(|y| row(buffer, y))
        .collect()
}

fn column_of(buffer: &Buffer, y: u16, glyph: char) -> Option<u16> {
    let area = buffer.area;
    (area.x..area.right()).find(|x| {
        buffer
            .cell((*x, y))
            .is_some_and(|cell| cell.symbol() == glyph.to_string())
    })
}

/// The card's own columns of one row, found by its top row's corners: the frame a card was opened
/// from is still on the screen either side of it.
fn of_card(buffer: &Buffer, y: u16) -> String {
    let top: Vec<char> = row(buffer, 0).chars().collect();
    let left = top
        .iter()
        .position(|glyph| *glyph == '╭')
        .expect("a left corner");
    let right = top
        .iter()
        .rposition(|glyph| *glyph == '╮')
        .expect("a right corner");
    row(buffer, y)
        .chars()
        .skip(left)
        .take(right + 1 - left)
        .collect()
}

#[test]
fn a_card_is_at_most_seventy_two_columns_wide_and_centred() {
    for (cols, left) in [(120, 24), (100, 14), (73, 0), (72, 0)] {
        let buffer = painted(&mut on_screen(Mode::Keys), cols, 24);
        assert_eq!(
            column_of(&buffer, 0, '╭'),
            Some(left),
            "at {cols} columns a 72-column card stands in column {left}, and the border of \
             one that is not centred stands somewhere else: {:?}",
            row(&buffer, 0)
        );
        assert_eq!(
            column_of(&buffer, 0, '╮'),
            Some(left + 71),
            "and its right corner is seventy-one columns on from its left one: {:?}",
            row(&buffer, 0)
        );
    }
}

#[test]
fn a_screen_names_itself_in_the_top_border_and_the_key_that_leaves_in_the_bottom() {
    for mode in SCREENS {
        let buffer = painted(&mut on_screen(mode), 120, 40);
        let top = of_card(&buffer, 0);
        assert!(
            top.starts_with('╭') && top.ends_with('╮'),
            "{} is drawn on a card whose top row is a rounded rule: {top:?}",
            name(mode)
        );
        assert!(
            top.contains(name(mode)),
            "and the row says which screen it is, in {}'s own name: {top:?}",
            name(mode)
        );

        let last = buffer.area.bottom() - 1;
        let bottom = of_card(&buffer, last);
        assert!(
            bottom.starts_with('╰') && bottom.ends_with('╯'),
            "{} is drawn on a card whose bottom row is a rounded rule: {bottom:?}",
            name(mode)
        );
        assert!(
            bottom.contains(leaves(mode)),
            "and the row says `{}`, the key the screen's own table binds to leaving it, so the \
             reader need not close the screen to find out: {bottom:?}",
            leaves(mode)
        );

        let side = of_card(&buffer, 1);
        assert!(
            side.starts_with('│') && side.ends_with('│'),
            "and the rows between them are the card's two sides: {side:?}"
        );
    }
}

#[test]
fn the_frame_behind_a_card_is_painted_and_dimmed() {
    let mut view = drafted();
    let palette = view.palette();
    let before = painted(&mut view, 120, 40);

    view.open_settings(&Config::default());
    let after = painted(&mut view, 120, 40);

    // The card is seventy-two columns wide in a hundred and twenty, so its two gutters are all
    // that is left of the frame it was opened from.
    let words = (0..40)
        .flat_map(|y| (0..24).map(move |x| (x, y)))
        .filter(|(x, y)| {
            before
                .cell((*x, *y))
                .is_some_and(|cell| cell.symbol() != " ")
        })
        .count();
    assert!(
        words > 0,
        "the frame behind the card drew {words} glyphs in the gutter the card leaves it"
    );
    for y in 0..40 {
        for x in (0..24).chain(96..120) {
            let held = before.cell((x, y)).expect("a cell of the frame");
            let drawn = after.cell((x, y)).expect("a cell of the frame");
            assert_eq!(
                drawn.symbol(),
                held.symbol(),
                "row {y}, column {x} is the frame the card was opened from, and it is still \
                 drawn: {:?}",
                row(&after, y)
            );
            assert_eq!(
                Ink::of(drawn, &palette),
                Some(Ink::Dim),
                "and it is dimmed, so the reader sees a frame they cannot reach: row {y}, \
                 column {x} is on the band {:?} and reads back as {:?}",
                drawn.bg,
                Ink::of(drawn, &palette)
            );
            assert_eq!(
                drawn.bg,
                before.cell((x, y)).expect("a cell").bg,
                "and a cell behind the card keeps the band it sits on, at row {y}, column {x}"
            );
        }
    }
}

#[test]
fn a_frame_under_fifty_columns_draws_the_card_full_bleed_with_its_title_row_and_nothing_else() {
    let buffer = painted(&mut on_screen(Mode::Keys), 49, 24);
    let screen = rows(&buffer).join("\n");

    let first = &rows(&buffer)[0];
    assert!(
        first.starts_with('─') && first.contains("? keys"),
        "at forty-nine columns the card is the frame, its title row the first: {first:?}"
    );
    for border in ['╭', '╮', '╰', '╯', '│'] {
        assert!(
            !screen.contains(border),
            "and no border is drawn, because the columns one costs are the reader's: {screen}"
        );
    }
    assert!(
        !screen.contains("esc"),
        "the card keeps its title row and nothing else, so the key that leaves is not drawn \
         either: {screen}"
    );
    assert!(
        rows(&buffer)[1].starts_with("  keys"),
        "and the screen's rows are drawn under the title, inset by the card's own two columns: \
         {screen}"
    );
}

#[test]
fn the_card_covers_the_frame_behind_it_rather_than_letting_it_read_through() {
    let mut view = drafted();
    view.open_settings(&Config::default());
    let buffer = painted(&mut view, 120, 40);

    let inside: String = (1..39).map(|y| of_card(&buffer, y)).collect();
    assert!(
        !inside.contains("opened over it"),
        "the transcript behind the card is dimmed and covered, and no row of the card reads \
         through it: {inside:?}"
    );
    assert!(
        of_card(&buffer, 2).contains("model"),
        "and the card's own rows are what is on it: {:?}",
        of_card(&buffer, 2)
    );
}

#[test]
fn the_words_of_a_row_are_never_cut_by_the_card_they_are_drawn_in() {
    let buffer = painted(&mut on_screen(Mode::Keys), 120, 40);
    let body: Vec<String> = (1..39).map(|y| of_card(&buffer, y)).collect();
    let drawn: HashSet<&str> = body.iter().flat_map(|row| row.split_whitespace()).collect();

    for binding in Mode::Composer.table() {
        for word in binding.does.split_whitespace() {
            assert!(
                drawn.contains(word),
                "the card is 68 columns of body and `{}`'s line is one of the longest, but no \
                 row of it may lose `{word}`: {body:#?}",
                binding.name
            );
        }
    }
}

#[test]
fn the_reason_a_value_was_refused_is_drawn_whole_however_narrow_the_card() {
    let mut view = View::boot();
    view.open_settings(&Config::default());
    view.refuse("max_turns two hundred will not parse: invalid digit found in string");
    let buffer = painted(&mut view, 120, 40);
    let body: Vec<String> = (1..39).map(|y| of_card(&buffer, y)).collect();
    let drawn: HashSet<&str> = body.iter().flat_map(|row| row.split_whitespace()).collect();

    for word in ["parse:", "invalid", "digit", "found", "string"] {
        assert!(
            drawn.contains(word),
            "a refusal names the key and says why, and the card wraps it rather than cutting the \
             reason off at `{word}`: {body:#?}"
        );
    }
}

#[test]
fn no_band_behind_a_card_takes_the_key_the_screen_does_not() {
    for mode in SCREENS {
        let mut view = opened(conversation(), mode);
        let draft = view.composer().text().to_owned();

        for key in [Key::Char('x'), Key::Char('y'), Key::Down, Key::Up] {
            view.key(key);
        }

        assert_eq!(
            view.composer().text(),
            draft,
            "{} is drawn over the composer band and takes its keys, so not one of them is the \
             draft's",
            name(mode)
        );
        assert_eq!(
            view.mode(),
            mode,
            "and the screen the frame dimmed is still the one in force"
        );
    }
}

#[test]
fn a_frame_too_short_for_a_body_still_names_the_screen_and_the_key_that_leaves() {
    let buffer = painted(&mut on_screen(Mode::Keys), 120, 2);

    assert!(
        of_card(&buffer, 0).contains("? keys"),
        "two rows hold the card's top border and its bottom one: {:?}",
        row(&buffer, 0)
    );
    assert!(
        of_card(&buffer, 1).contains(leaves(Mode::Keys)),
        "and the bottom border of a card with no body is still the row that says how to leave: \
         {:?}",
        row(&buffer, 1)
    );
}

#[test]
fn a_name_too_wide_for_its_row_is_cut_at_the_rows_right_edge() {
    let mut buffer = Buffer::empty(Rect::new(0, 0, 80, 6));
    let name = "n".repeat(200);
    sindri::card::paint(buffer.area, &name, "", &mut buffer, &Palette::TRUECOLOR);

    let top = row(&buffer, 0).trim().to_owned();
    assert!(
        top.starts_with("╭─ nnn") && top.ends_with("n╮"),
        "a name longer than the row runs to the last column the border leaves and stops there: \
         {top:?}"
    );
    assert!(
        top.matches('n').count() < name.len(),
        "and the rest of it is not drawn: {} of {} glyphs are",
        top.matches('n').count(),
        name.len()
    );
}

#[test]
fn a_key_the_bottom_row_has_no_room_for_is_refused_whole() {
    let mut buffer = Buffer::empty(Rect::new(0, 0, 80, 6));
    let leaves = "k".repeat(100);
    sindri::card::paint(buffer.area, "?", &leaves, &mut buffer, &Palette::TRUECOLOR);

    let bottom = row(&buffer, 5).trim().to_owned();
    assert_eq!(
        bottom,
        format!("╰{}╯", "─".repeat(70)),
        "a key no card row holds is refused whole rather than cut, and the row is its rule"
    );
}

#[test]
fn a_key_as_wide_as_the_bottom_row_allows_keeps_the_corners_it_hangs_between() {
    let mut buffer = Buffer::empty(Rect::new(0, 0, 80, 6));
    let leaves = "k".repeat(66);
    sindri::card::paint(buffer.area, "?", &leaves, &mut buffer, &Palette::TRUECOLOR);

    let bottom = row(&buffer, 5).trim().to_owned();
    assert_eq!(
        bottom,
        format!("╰─ {} ─╯", "k".repeat(66)),
        "the widest key the bottom row holds hangs between a column of rule and the corners, \
         which it does not cover"
    );
}
