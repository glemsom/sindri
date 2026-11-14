//! The card every screen is drawn on: the rounded border and the name in its top row, the key that
//! leaves in its bottom row, the frame behind it painted and dimmed, and the geometry a card takes
//! at a frame too narrow to hold one.

use std::collections::HashSet;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use sindri::composer::Key;
use sindri::ink::{Face, Ink, Look};
use sindri::keys::{Act, Mode};
use sindri::palette::Palette;
use sindri::view::View;
use sindri_core::config::Config;
use sindri_core::provider::served::{Listing, Where};
use sindri_core::provider::transport::{Canned, Replay};
use sindri_core::provider::{Client, Credential};
use sindri_core::records::Entry;
use sindri_core::redact::SecretSet;
use sindri_core::wire::Wire;

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

const ANTHROPIC_MODELS: &str =
    include_str!("../../sindri-core/tests/fixtures/providers/anthropic_models.json");

/// The names a recorded Anthropic listing serves, read through the listing code the screen reads.
fn served() -> Listing {
    let replay = Replay::new([Canned::reply(200, ANTHROPIC_MODELS)]);
    let at = Where::own(Wire::Anthropic);
    Client::over(replay, SecretSet::default())
        .waits(|_| {})
        .served(&at, &Credential::new("sk-ant-recorded"), &at)
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

/// The look the cell at `x`, `y` of the frame reads back as.
fn look(buffer: &Buffer, palette: &Palette, x: u16, y: u16) -> Look {
    Look::of(buffer.cell((x, y)).expect("a cell of the frame"), palette)
        .expect("every cell of a frame is drawn in a look")
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

/// The rows the card's two borders stand on and the columns its corners are drawn at, or none for a
/// card a frame under fifty columns draws with no border at all.
fn corners(buffer: &Buffer) -> Option<(u16, u16, u16, u16)> {
    let rows = buffer.area.y..buffer.area.bottom();
    let top = rows
        .clone()
        .find(|y| column_of(buffer, *y, '╭').is_some())?;
    let bottom = rows.rev().find(|y| column_of(buffer, *y, '╰').is_some())?;
    Some((
        top,
        bottom,
        column_of(buffer, top, '╭').expect("a left corner"),
        column_of(buffer, top, '╮').expect("a right corner"),
    ))
}

/// The card's own columns of one row: the frame a card was opened from is still on the screen
/// either side of it.
fn of_card(buffer: &Buffer, y: u16) -> String {
    let (_, _, left, right) = corners(buffer).expect("a card drawn with its border");
    let (left, right) = (usize::from(left), usize::from(right));
    row(buffer, y)
        .chars()
        .skip(left)
        .take(right + 1 - left)
        .collect()
}

/// The words one row of the card's body draws, between its two sides, with the blanks it pads its
/// end with dropped.
fn said(buffer: &Buffer, y: u16) -> String {
    let (_, _, left, right) = corners(buffer).expect("a card drawn with its border");
    let (left, right) = (usize::from(left), usize::from(right) + 1);
    row(buffer, y)
        .chars()
        .skip(left + 1)
        .take(right - left - 2)
        .collect::<String>()
        .trim_end()
        .to_owned()
}

/// Every row of the card's body, top to bottom, as the words each draws.
fn body_rows(buffer: &Buffer) -> Vec<String> {
    let (top, bottom, ..) = corners(buffer).expect("a card drawn with its border");
    (top + 1..bottom).map(|y| said(buffer, y)).collect()
}

#[test]
fn a_card_is_at_most_seventy_two_columns_wide_and_centred() {
    for (cols, left) in [(120, 24), (100, 14), (73, 0), (72, 0)] {
        let buffer = painted(&mut on_screen(Mode::Keys), cols, 24);
        let (top, _, corner, _) = corners(&buffer).expect("a card drawn with its border");
        assert_eq!(
            corner,
            left,
            "at {cols} columns a 72-column card stands in column {left}, and the border of \
             one that is not centred stands somewhere else: {:?}",
            row(&buffer, top)
        );
        assert_eq!(
            column_of(&buffer, top, '╮'),
            Some(left + 71),
            "and its right corner is seventy-one columns on from its left one: {:?}",
            row(&buffer, top)
        );
    }
}

#[test]
fn a_screen_names_itself_in_the_top_border_and_the_key_that_leaves_in_the_bottom() {
    for mode in SCREENS {
        let buffer = painted(&mut on_screen(mode), 120, 40);
        let (top, bottom, ..) = corners(&buffer).expect("a card drawn with its border");

        let named = of_card(&buffer, top);
        assert!(
            named.starts_with('╭') && named.ends_with('╮'),
            "{} is drawn on a card whose top row is a rounded rule: {named:?}",
            name(mode)
        );
        assert!(
            named.contains(name(mode)),
            "and the row says which screen it is, in {}'s own name: {named:?}",
            name(mode)
        );

        let leaving = of_card(&buffer, bottom);
        assert!(
            leaving.starts_with('╰') && leaving.ends_with('╯'),
            "{} is drawn on a card whose bottom row is a rounded rule: {leaving:?}",
            name(mode)
        );
        assert!(
            leaving.contains(leaves(mode)),
            "and the row says `{}`, the key the screen's own table binds to leaving it, so the \
             reader need not close the screen to find out: {leaving:?}",
            leaves(mode)
        );

        for side in (top + 1..bottom).map(|y| of_card(&buffer, y)) {
            assert!(
                side.starts_with('│') && side.ends_with('│'),
                "and the rows between them are the card's two sides: {side:?}"
            );
        }
    }
}

#[test]
fn every_screen_open_over_the_frame_carries_the_accent_on_the_edge_row_of_its_card() {
    let palette = Palette::TRUECOLOR;
    let faint = Look::new(Ink::Accent, Face::Faint);

    for (cols, rows) in [(120, 40), (40, 24)] {
        for mode in SCREENS {
            let buffer = painted(&mut on_screen(mode), cols, rows);
            // A card too narrow to hold a border is the whole frame with its title row first, and
            // one that holds it is drawn between its corners with the frame it dimmed either side.
            let card = corners(&buffer);
            let (top, bottom, left, right) = match card {
                Some((top, bottom, left, right)) => (top, bottom, left, right),
                None => (buffer.area.y, buffer.area.y, 0, cols - 1),
            };
            let edge = row(&buffer, top);
            assert!(
                edge.contains(name(mode)),
                "{} is drawn on the card's edge row: {edge:?}",
                name(mode)
            );
            for column in left..=right {
                assert_eq!(
                    look(&buffer, &palette, column, top),
                    faint,
                    "column {column} of {}'s edge row at {cols} columns is not the accent drawn \
                     faint, and the card is the surface that takes the keys: {edge:?}",
                    name(mode)
                );
            }
            if card.is_none() {
                continue;
            }

            for column in left..=right {
                assert_ne!(
                    look(&buffer, &palette, column, bottom),
                    faint,
                    "column {column} of the row that says how to leave {} carries the accent too, \
                     and one card lights one row",
                    name(mode)
                );
            }
        }
    }
}

#[test]
fn a_card_drawn_in_no_colour_keeps_the_accent_on_its_name_and_draws_no_accent_edge() {
    let palette = Palette::SLOTS;
    let mut view = on_screen(Mode::Keys);
    view.set_palette(palette);
    let buffer = painted(&mut view, 120, 40);
    let (top, _, left, _) = corners(&buffer).expect("a card drawn with its border");
    let named_at = row(&buffer, top)
        .find(name(Mode::Keys))
        .expect("the screen's own name in the edge row of its card");
    let named_at = sindri::pen::cells(&row(&buffer, top)[..named_at]);

    assert_eq!(
        look(&buffer, &palette, left, top),
        Look::plain(Ink::Dim),
        "the card's edge row is the dim edge it always drew: a reader who asked for no colour is \
         given no accent edge to see, and their focus is carried by the card's border and the \
         scrim"
    );
    assert_eq!(
        look(&buffer, &palette, named_at, top),
        Look::plain(Ink::Accent),
        "and the name that row carries is the accent it always was, so every screen such a \
         terminal draws is the screen it drew before: {:?}",
        row(&buffer, top)
    );
}

#[test]
fn a_card_drawn_in_a_window_nobody_is_reading_keeps_the_dim_edge() {
    let palette = Palette::TRUECOLOR;
    let mut view = on_screen(Mode::Keys);
    view.set_palette(palette);
    view.set_focused(false);
    let buffer = painted(&mut view, 120, 40);
    let (top, _, left, _) = corners(&buffer).expect("a card drawn with its border");

    assert_eq!(
        look(&buffer, &palette, left, top),
        palette.dim_edge(),
        "the card is the surface that takes the keys, and it lights its edge for a reader who is \
         reading it: nobody is, so the edge recedes and the card's border carries it"
    );
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

    let inside = body_rows(&buffer).join("\n");
    assert!(
        !inside.contains("opened over it"),
        "the transcript behind the card is dimmed and covered, and no row of the card reads \
         through it: {inside:?}"
    );
    assert!(
        inside.contains("model"),
        "and the card's own rows are what is on it: {inside:?}"
    );
}

#[test]
fn the_words_of_a_row_are_never_cut_by_the_card_they_are_drawn_in() {
    let buffer = painted(&mut on_screen(Mode::Keys), 120, 40);
    let body = body_rows(&buffer);
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
fn a_description_wider_than_the_card_wraps_under_its_own_column_and_hangs_under_it() {
    let buffer = painted(&mut on_screen(Mode::Keys), 120, 40);
    let body = body_rows(&buffer);
    let does = Mode::Composer
        .table()
        .iter()
        .find(|binding| binding.name == "@")
        .expect("the empty draft binds `@` to the files of the workspace")
        .does;
    let opens: Vec<&str> = does.split_whitespace().take(4).collect();
    let first = body
        .iter()
        .position(|row| row.contains(&opens.join(" ")))
        .unwrap_or_else(|| {
            panic!(
                "no row of the keys screen says `{}`: {body:#?}",
                opens.join(" ")
            )
        });
    let start = body[first]
        .find(&opens.join(" "))
        .expect("the row the line begins on says its first words");
    let column = body[first][..start].chars().count();
    let under = &body[first + 1];
    let hung: Vec<&str> = under.split_whitespace().collect();
    let words: Vec<&str> = does.split_whitespace().collect();
    assert!(
        !hung.is_empty() && words.ends_with(&hung),
        "the line this test is about is wider than the columns the card has left for it, and the \
         row under it says the words it had no room for: {:?} then {under:?}",
        body[first]
    );
    assert!(
        under.chars().take(column).all(|glyph| glyph == ' '),
        "and that row is blank up to the column the description begins in, so a wrapped line \
         hangs under the line above it rather than standing as a name of its own: {under:?}"
    );
}

#[test]
fn the_reason_a_value_was_refused_is_drawn_whole_however_narrow_the_card() {
    let mut view = View::boot();
    view.open_settings(&Config::default());
    view.refuse("max_turns two hundred will not parse: invalid digit found in string");
    let buffer = painted(&mut view, 120, 40);
    let body = body_rows(&buffer);
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
fn a_refusal_is_drawn_as_a_verdict_under_the_last_key_row() {
    let reason = "bad value in max_turns: `two hundred` will not parse: invalid digit found in \
                  string";
    let mut view = View::boot();
    view.open_settings(&Config::default());
    view.refuse(reason);
    let palette = view.palette();
    let buffer = painted(&mut view, 120, 40);
    let (top, bottom, left, _) = corners(&buffer).expect("a card drawn with its border");
    let body = body_rows(&buffer);
    let at = body
        .iter()
        .position(|row| row.trim_start().starts_with("ssh_agent"))
        .expect("the last key of the schema drawn in the card");
    let last = top + 1 + u16::try_from(at).expect("a row of the card");

    assert_eq!(
        said(&buffer, last + 1),
        "",
        "a blank row stands under the last key, so the refusal reads as a verdict on the visit and \
         not as that key's value: {body:#?}"
    );

    let mut y = last + 2;
    let mut drawn = Vec::new();
    while y < bottom && !said(&buffer, y).is_empty() {
        let row = said(&buffer, y);
        let start = u16::try_from(row.len() - row.trim_start().len()).expect("a column");
        let words = row.trim_start();
        let inks: Vec<Ink> = (0..sindri::pen::cells(words))
            .map(|px| look(&buffer, &palette, left + 1 + start + px, y).ink())
            .collect();
        assert!(
            inks.iter().all(|ink| *ink == Ink::Err),
            "every cell of the verdict is drawn in the ink an error is read in, so it is not \
             mistaken for a key or a value: {words:?} in {inks:?}"
        );
        drawn.extend(words.split_whitespace().map(str::to_owned));
        y += 1;
    }
    assert_eq!(
        drawn.join(" "),
        reason,
        "the verdict stands between the blank row and the card's bottom border whole, and not cut \
         at the border with its tail lost: {body:#?}"
    );
}

#[test]
fn a_frame_too_short_for_the_settings_visit_draws_the_head_of_the_list_and_keeps_the_verdict_back()
{
    let mut view = View::boot();
    view.open_settings(&Config::default());
    view.refuse(
        "bad value in max_turns: `two hundred` will not parse: invalid digit found in string",
    );
    let buffer = painted(&mut view, 120, 14);
    let body = body_rows(&buffer);
    let frame = rows(&buffer).join("\n");

    assert!(
        body.first().is_some_and(|row| row.contains("provider")),
        "fourteen rows hold the card's borders and eleven of its body, and the head of the key \
         list is what those eleven rows are for: {body:#?}"
    );
    assert!(
        !body.iter().any(|row| row.contains("ssh_agent")),
        "so the tail of the key list is kept back rather than drawn over the card's last row: \
         {body:#?}"
    );
    assert!(
        body.iter().any(|row| row.ends_with("rows kept back")),
        "the card's own count row says how much of the visit it could not hold, in the last row \
         the body has: {body:#?}"
    );
    for word in ["parse:", "invalid", "digit", "hundred"] {
        assert!(
            !frame.contains(word),
            "a verdict the frame has no room for is not drawn at all rather than cut into `{word}`, \
             which would say a reason the reader cannot read: {frame}"
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
    let edge = Palette::TRUECOLOR.accent_edge();
    sindri::card::paint(
        buffer.area,
        &name,
        "",
        4,
        &mut buffer,
        edge,
        &Palette::TRUECOLOR,
    );

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
    let edge = Palette::TRUECOLOR.accent_edge();
    sindri::card::paint(
        buffer.area,
        "?",
        &leaves,
        4,
        &mut buffer,
        edge,
        &Palette::TRUECOLOR,
    );

    let bottom = row(&buffer, 5).trim().to_owned();
    assert_eq!(
        bottom,
        format!("╰{}╯", sindri::pen::filled(70, '─')),
        "a key no card row holds is refused whole rather than cut, and the row is its rule"
    );
}

#[test]
fn a_key_as_wide_as_the_bottom_row_allows_keeps_the_corners_it_hangs_between() {
    let mut buffer = Buffer::empty(Rect::new(0, 0, 80, 6));
    let leaves = "k".repeat(66);
    let edge = Palette::TRUECOLOR.accent_edge();
    sindri::card::paint(
        buffer.area,
        "?",
        &leaves,
        4,
        &mut buffer,
        edge,
        &Palette::TRUECOLOR,
    );

    let bottom = row(&buffer, 5).trim().to_owned();
    assert_eq!(
        bottom,
        format!("╰─ {} ─╯", "k".repeat(66)),
        "the widest key the bottom row holds hangs between a column of rule and the corners, \
         which it does not cover"
    );
}

/// The ellipsis a card marks a row it keeps back with.
const ELLIPSIS: char = '\u{22EF}';

/// The card a frame of `cols` by `rows` paints when the screen inside it holds `holds` rows, and
/// the band the screen's own rows are drawn in.
fn card_in(cols: u16, rows: u16, holds: u16) -> (Buffer, Rect) {
    let mut buffer = Buffer::empty(Rect::new(0, 0, cols, rows));
    let edge = Palette::TRUECOLOR.accent_edge();
    let body = sindri::card::paint(
        buffer.area,
        "? keys",
        "esc",
        holds,
        &mut buffer,
        edge,
        &Palette::TRUECOLOR,
    );
    (buffer, body)
}

/// The cell at `x`, `y` of the frame, as the glyph drawn in it.
fn glyph_at(buffer: &Buffer, x: u16, y: u16) -> String {
    buffer
        .cell((x, y))
        .expect("a cell of the frame")
        .symbol()
        .to_owned()
}

/// Row `y` of the frame read through the `width` columns of `body`, with the blanks it leaves at
/// its end dropped.
fn words(buffer: &Buffer, body: Rect, y: u16) -> String {
    row(buffer, y)
        .chars()
        .skip(usize::from(body.x))
        .take(usize::from(body.width))
        .collect::<String>()
        .trim_end()
        .to_owned()
}

#[test]
fn a_card_is_as_tall_as_its_body_and_its_two_borders_and_centred_in_the_frame() {
    let (buffer, body) = card_in(80, 20, 4);
    assert_eq!(
        glyph_at(&buffer, 4, 7),
        "╭",
        "a card holding four rows is six rows tall — its body and its two borders — and a frame \
         of twenty has a gap of fourteen to centre it in, whose spare row falls below: {:?}",
        rows(&buffer)
    );
    assert_eq!(
        glyph_at(&buffer, 75, 12),
        "╯",
        "and its bottom border is the last of those six rows, seven rows below the first: {:?}",
        rows(&buffer)
    );
    assert_eq!(
        body,
        Rect::new(6, 8, 68, 4),
        "the body is the four rows the screen holds, under the top border and between the card's \
         two columns of inset: {:?}",
        rows(&buffer)
    );
    assert_eq!(
        glyph_at(&buffer, 4, 6),
        " ",
        "no row above the card is its own, so the frame under it is not covered: {:?}",
        rows(&buffer)
    );
    assert_eq!(
        glyph_at(&buffer, 4, 13),
        " ",
        "and no row under it is either: {:?}",
        rows(&buffer)
    );

    let (buffer, body) = card_in(80, 20, 5);
    assert_eq!(
        glyph_at(&buffer, 4, 6),
        "╭",
        "a card holding five rows is seven rows tall, and the gap of thirteen it is centred in \
         leaves its spare row below it, so its top border is row 6: {:?}",
        rows(&buffer)
    );
    assert_eq!(
        glyph_at(&buffer, 75, 12),
        "╯",
        "and its bottom border is row 12, the same row a card one row shorter ends on: {:?}",
        rows(&buffer)
    );
    assert_eq!(
        body,
        Rect::new(6, 7, 68, 5),
        "the body is the five rows the screen holds, under the top border: {:?}",
        rows(&buffer)
    );
}

#[test]
fn a_body_taller_than_the_frame_is_capped_and_the_card_says_how_many_rows_it_keeps_back() {
    let (buffer, body) = card_in(80, 20, 60);
    assert_eq!(
        glyph_at(&buffer, 4, 0),
        "╭",
        "a body of sixty rows in a frame of twenty is capped at the frame, so its top border is \
         the frame's first row: {:?}",
        rows(&buffer)
    );
    assert_eq!(
        glyph_at(&buffer, 75, 19),
        "╯",
        "and its bottom border the frame's last: {:?}",
        rows(&buffer)
    );
    assert_eq!(
        body,
        Rect::new(6, 1, 68, 17),
        "the eighteen rows between the borders give one up to the row that says how many it keeps \
         back, so the screen draws in seventeen of them: {:?}",
        rows(&buffer)
    );
    assert_eq!(
        words(&buffer, body, 18),
        format!("{ELLIPSIS} 43 rows kept back"),
        "the row above the bottom border says how many of the sixty rows the card could not hold, \
         marked with the ellipsis the fold counts in: {:?}",
        rows(&buffer)
    );
    assert_eq!(
        look(&buffer, &Palette::TRUECOLOR, body.x, 18),
        Look::plain(Ink::Dim),
        "and it is said in the dim ink of the words a card keeps back: {:?}",
        rows(&buffer)
    );
    assert_eq!(
        glyph_at(&buffer, 4, 19),
        "╰",
        "the bottom border is still drawn"
    );
    assert_eq!(glyph_at(&buffer, 75, 19), "╯", "with both its corners");
    assert!(
        row(&buffer, 19).contains("esc"),
        "and it still names the key that leaves: {:?}",
        row(&buffer, 19)
    );
}

#[test]
fn a_card_covers_no_row_of_the_frame_its_body_and_borders_do_not_need() {
    let mut buffer = Buffer::empty(Rect::new(0, 0, 80, 20));
    for y in 0..20 {
        for x in 0..80 {
            if let Some(cell) = buffer.cell_mut((x, y)) {
                cell.set_symbol("#");
            }
        }
    }
    let edge = Palette::TRUECOLOR.accent_edge();
    sindri::card::paint(
        buffer.area,
        "? keys",
        "esc",
        4,
        &mut buffer,
        edge,
        &Palette::TRUECOLOR,
    );

    for y in (0..7).chain(13..20) {
        assert_eq!(
            row(&buffer, y),
            "#".repeat(80),
            "row {y} of the frame is neither a border of the card nor a row of its body, so the \
             glyph the frame drew there is still the one on it: {:?}",
            rows(&buffer)
        );
    }
    let card: Vec<String> = (7..13)
        .map(|y| row(&buffer, y).chars().skip(4).take(72).collect::<String>())
        .collect();
    for (y, row) in (7..13).zip(&card) {
        assert!(
            !row.contains('#'),
            "row {y} is the card's own, and no glyph the frame drew behind it reads through: {row:?}"
        );
    }
}

#[test]
fn a_frame_under_fifty_columns_bleeds_a_body_too_tall_for_it_and_counts_the_rows_it_keeps_back() {
    let (buffer, body) = card_in(40, 24, 40);
    let first = row(&buffer, 0);
    assert!(
        first.starts_with('─') && first.contains("? keys"),
        "a frame under fifty columns has no border to spend columns on, so its first row is the \
         card's title row: {first:?}"
    );
    assert_eq!(
        body,
        Rect::new(2, 1, 36, 22),
        "the body keeps all but the last row of the frame, less the one it gives up to the row \
         that says how many it keeps back: {:?}",
        rows(&buffer)
    );
    assert_eq!(
        words(&buffer, body, 23),
        format!("{ELLIPSIS} 18 rows kept back"),
        "the frame's last row says how many of the forty rows it could not hold: {:?}",
        rows(&buffer)
    );
    let screen = rows(&buffer).join("\n");
    for border in ['╭', '╮', '╰', '╯', '│'] {
        assert!(
            !screen.contains(border),
            "and no border is drawn, because the columns one costs are the reader's: {screen}"
        );
    }
}

#[test]
fn a_frame_too_short_for_a_body_draws_its_borders_and_counts_no_rows() {
    let (buffer, body) = card_in(120, 2, 60);
    assert_eq!(
        body,
        Rect::new(26, 1, 68, 0),
        "two rows hold the card's top border and its bottom one, and nothing between them for a \
         body to be drawn in: {:?}",
        rows(&buffer)
    );
    assert!(
        row(&buffer, 0).contains("? keys"),
        "so the top border names the screen: {:?}",
        row(&buffer, 0)
    );
    let last = row(&buffer, 1);
    assert_eq!(
        glyph_at(&buffer, 24, 1),
        "╰",
        "and the bottom border is drawn under it: {last:?}"
    );
    assert!(last.contains("esc"), "naming the key that leaves: {last:?}");
    let screen = rows(&buffer).join("\n");
    assert!(
        !screen.contains("kept back"),
        "and no row of a card with none to draw the screen in says what it keeps back: {screen:?}"
    );
}

#[test]
fn every_screen_is_drawn_on_a_card_as_tall_as_the_rows_it_holds() {
    for (cols, tall) in [(120, 40), (80, 24)] {
        for mode in SCREENS {
            let buffer = painted(&mut on_screen(mode), cols, tall);
            let (top, bottom, ..) = corners(&buffer).unwrap_or_else(|| {
                panic!(
                    "{} is drawn on a card with a border at {cols}x{tall}: {:?}",
                    name(mode),
                    rows(&buffer)
                )
            });
            let above = top - buffer.area.y;
            let below = buffer.area.bottom() - 1 - bottom;
            let held = bottom - top - 1;

            assert!(
                held == 0 || !said(&buffer, bottom - 1).trim().is_empty(),
                "{}'s card at {cols}x{tall} ends where its body ends, so the last of the {held} \
                 rows it holds is the one above its bottom border and not a blank row it did not \
                 need: {:?}",
                name(mode),
                rows(&buffer)
            );
            assert!(
                above == below || above + 1 == below,
                "{}'s card at {cols}x{tall} is centred: {above} rows of frame above it and \
                 {below} below, and a card with no spare row carries the one it cannot share \
                 under it: {:?}",
                name(mode),
                rows(&buffer)
            );
            assert!(
                above + below > 0 || said(&buffer, bottom - 1).contains("kept back"),
                "{}'s card at {cols}x{tall} is the whole frame, and a card is only as tall as the \
                 frame when its body is taller than the room it has: the row above its bottom \
                 border says how many rows it keeps back, where a card that is the frame's height \
                 and says nothing is the card this screen was drawn on before: {:?}",
                name(mode),
                rows(&buffer)
            );
        }
    }
}

#[test]
fn the_model_list_is_a_card_as_tall_as_the_names_it_holds() {
    let mut view = opened(conversation(), Mode::Models);
    view.set_served(served());
    let buffer = painted(&mut view, 120, 40);

    let (top, bottom, ..) = corners(&buffer).expect("a card drawn with its border");
    assert_eq!(
        (top, bottom),
        (17, 21),
        "the card holding the query row and the two names a wire served is five rows — those \
         three and its two borders — and a frame of forty has a gap of thirty-five to centre it \
         in, whose spare row falls below: {:?}",
        rows(&buffer)
    );
    let named = of_card(&buffer, top);
    assert!(
        named.starts_with("╭─ /model") && named.ends_with('╮'),
        "the top border wears the screen's own name: {named:?}"
    );
    assert_eq!(
        said(&buffer, 18).trim_start(),
        "query:",
        "the query is the first row of the body: {:?}",
        row(&buffer, 18)
    );
    assert!(
        said(&buffer, 19).contains("claude-sonnet-4-6"),
        "and the name the wire served first is the row under it: {:?}",
        row(&buffer, 19)
    );
    assert!(
        said(&buffer, 20).contains("claude-haiku-4-5"),
        "the second name the wire served is the last row the card holds: {:?}",
        row(&buffer, 20)
    );
    let leaving = of_card(&buffer, bottom);
    assert!(
        leaving.starts_with('╰') && leaving.contains(leaves(Mode::Models)),
        "and the card ends on its bottom border, five rows under its top one: {leaving:?}"
    );
    assert!(
        corners(&buffer).is_some_and(|(_, bottom, ..)| bottom == 21),
        "so the frame the card was opened from is on the screen again under it: {:?}",
        row(&buffer, 22)
    );
}

#[test]
fn a_card_the_frame_cannot_hold_keeps_back_the_rows_it_does_not_draw() {
    let whole = painted(&mut on_screen(Mode::Keys), 120, 40);
    let capped = painted(&mut on_screen(Mode::Keys), 80, 24);

    let held = body_rows(&whole);
    let (top, bottom, ..) = corners(&capped).expect("a card drawn with its border");
    let drawn: Vec<String> = (top + 1..bottom - 1).map(|y| said(&capped, y)).collect();
    let count = said(&capped, bottom - 1);
    assert!(
        drawn.len() < held.len(),
        "a frame of twenty-four rows cannot hold the keys list a frame of forty draws: {} rows \\
         against {}",
        drawn.len(),
        held.len()
    );
    assert_eq!(
        drawn[..],
        held[..drawn.len()],
        "the card too short for the list draws the first rows of it, in the same order and with \
         the same words: {:?}",
        rows(&capped)
    );
    let kept = held.len() - drawn.len();
    let told: usize = count
        .split_whitespace()
        .nth(1)
        .and_then(|said| said.parse().ok())
        .unwrap_or_else(|| {
            panic!(
                "the card's own count row says how many rows it keeps back: {count:?}, {:?}",
                rows(&capped)
            )
        });
    assert_eq!(
        told, kept,
        "and the rows it kept back are the difference between the rows the list holds and the rows \
         the card draws: {count:?}"
    );
}
