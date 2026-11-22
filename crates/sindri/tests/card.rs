//! The card every screen is drawn on: the rounded border and the name in its top row, the key that
//! leaves in its bottom row, the frame behind it painted and dimmed, and the geometry a card takes
//! at a frame too narrow to hold one.

mod caps; // the terminals these frames are drawn on
mod frame; // one paint-and-read for every test

use std::collections::HashSet;

use frame::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use sindri::capability::Capabilities;
use sindri::composer::Key;
use sindri::facts::Facts;
use sindri::ink::{Face, Ink, Look};
use sindri::keys::{Act, Mode};
use sindri::palette::Palette;
use sindri::view::{Transcript, View};
use sindri_core::config::Config;
use sindri_core::provider::endpoint::Endpoint;
use sindri_core::provider::served::Listing;
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
            view.handed(
                view.facts()
                    .clone()
                    .working(root.path().to_path_buf(), None),
            );
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
    let at = Endpoint::own(Wire::Anthropic);
    Client::over(replay, SecretSet::default())
        .waits(|_| {})
        .served(&at, &Credential::new("sk-ant-recorded"), &at)
}

fn on_screen(mode: Mode) -> View {
    opened(conversation(), mode)
}

/// A frame with a run in it and no draft, so every screen opens on its own key.
fn conversation() -> View {
    conversation_in(Capabilities::default())
}

/// The same conversation on `capabilities`: the frame a card is opened over, drawn in other colours.
fn conversation_in(capabilities: Capabilities) -> View {
    let entries = [
        Entry::Turn { turn: 1 },
        Entry::Assistant {
            turn: 1,
            text: SecretSet::default().redact(
                "The band the transcript is drawn in holds the run\nand the \ncard is opened over it",
            ),
        },
    ];
    View::of(
        capabilities,
        Facts::default().asked(),
        Transcript::of(entries).scrolled(sindri::rows::Scroll::Tail),
    )
}

/// The same frame with a draft typed into it, so the composer band is behind the card too.
fn drafted() -> View {
    let mut view = conversation();
    view.paste("a draft behind the card");
    view
}

/// The rows the card's two borders stand on and the columns its corners are drawn at, or none for a
/// card a frame under fifty columns draws with no border at all.
fn corners(drawn: &Frame) -> Option<(u16, u16, u16, u16)> {
    let area = drawn.area();
    let rows = area.y..area.bottom();
    let top = rows.clone().find(|y| drawn.column_of(*y, '╭').is_some())?;
    let bottom = rows.rev().find(|y| drawn.column_of(*y, '╰').is_some())?;
    Some((
        top,
        bottom,
        drawn.column_of(top, '╭').expect("a left corner"),
        drawn.column_of(top, '╮').expect("a right corner"),
    ))
}

/// The card's own columns of one row: the frame a card was opened from is still on the screen
/// either side of it.
fn of_card(drawn: &Frame, y: u16) -> String {
    let (_, _, left, right) = corners(drawn).expect("a card drawn with its border");
    let (left, right) = (usize::from(left), usize::from(right));
    drawn
        .row(y)
        .chars()
        .skip(left)
        .take(right + 1 - left)
        .collect()
}

/// The words one row of the card's body draws, between its two sides, with the blanks it pads its
/// end with dropped.
fn said(drawn: &Frame, y: u16) -> String {
    let (_, _, left, right) = corners(drawn).expect("a card drawn with its border");
    let (left, right) = (usize::from(left), usize::from(right) + 1);
    drawn
        .row(y)
        .chars()
        .skip(left + 1)
        .take(right - left - 2)
        .collect::<String>()
        .trim_end()
        .to_owned()
}

/// Every row of the card's body, top to bottom, as the words each draws.
fn body_rows(drawn: &Frame) -> Vec<String> {
    let (top, bottom, ..) = corners(drawn).expect("a card drawn with its border");
    (top + 1..bottom).map(|y| said(drawn, y)).collect()
}

#[test]
fn a_card_is_at_most_seventy_two_columns_wide_and_centred() {
    for (cols, left) in [(120, 24), (100, 14), (73, 0), (72, 0)] {
        let frame = frame::painted(&mut on_screen(Mode::Keys), cols, 24);
        let (top, _, corner, _) = corners(&frame).expect("a card drawn with its border");
        assert_eq!(
            corner,
            left,
            "at {cols} columns a 72-column card stands in column {left}, and the border of \
             one that is not centred stands somewhere else: {:?}",
            frame.row(top)
        );
        assert_eq!(
            frame.column_of(top, '╮'),
            Some(left + 71),
            "and its right corner is seventy-one columns on from its left one: {:?}",
            frame.row(top)
        );
    }
}

#[test]
fn a_screen_names_itself_in_the_top_border_and_the_key_that_leaves_in_the_bottom() {
    for mode in SCREENS {
        let frame = frame::painted(&mut on_screen(mode), 120, 40);
        let (top, bottom, ..) = corners(&frame).expect("a card drawn with its border");

        let named = of_card(&frame, top);
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

        let leaving = of_card(&frame, bottom);
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

        for side in (top + 1..bottom).map(|y| of_card(&frame, y)) {
            assert!(
                side.starts_with('│') && side.ends_with('│'),
                "and the rows between them are the card's two sides: {side:?}"
            );
        }
    }
}

#[test]
fn every_screen_open_over_the_frame_carries_the_accent_on_the_edge_row_of_its_card() {
    let faint = Look::new(Ink::Accent, Face::Faint);

    for (cols, rows) in [(120, 40), (40, 24)] {
        for mode in SCREENS {
            let frame = frame::painted(&mut on_screen(mode), cols, rows);
            // A card too narrow to hold a border is the whole frame with its title row first, and
            // one that holds it is drawn between its corners with the frame it dimmed either side.
            let card = corners(&frame);
            let (top, bottom, left, right) = match card {
                Some((top, bottom, left, right)) => (top, bottom, left, right),
                None => (frame.area().y, frame.area().y, 0, cols - 1),
            };
            let edge = frame.row(top);
            assert!(
                edge.contains(name(mode)),
                "{} is drawn on the card's edge row: {edge:?}",
                name(mode)
            );
            for column in left..=right {
                assert_eq!(
                    frame.look(column, top),
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
                    frame.look(column, bottom),
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
    let mut view = opened(conversation_in(caps::inked(palette)), Mode::Keys);
    let frame = frame::painted(&mut view, 120, 40);
    let (top, _, left, _) = corners(&frame).expect("a card drawn with its border");
    let named_at = frame
        .row(top)
        .find(name(Mode::Keys))
        .expect("the screen's own name in the edge row of its card");
    let named_at = sindri::pen::cells(&frame.row(top)[..named_at]);

    assert_eq!(
        frame.look(left, top),
        Look::plain(Ink::Dim),
        "the card's edge row is the dim edge it always drew: a reader who asked for no colour is \
         given no accent edge to see, and their focus is carried by the card's border and the \
         scrim"
    );
    assert_eq!(
        frame.look(named_at, top),
        Look::plain(Ink::Accent),
        "and the name that row carries is the accent it always was, so every screen such a \
         terminal draws is the screen it drew before: {:?}",
        frame.row(top)
    );
}

#[test]
fn a_card_drawn_in_a_window_nobody_is_reading_keeps_the_dim_edge() {
    let palette = Palette::TRUECOLOR;
    let mut view = on_screen(Mode::Keys);
    view.handed(view.facts().clone().focused(false));
    let frame = frame::painted(&mut view, 120, 40);
    let (top, _, left, _) = corners(&frame).expect("a card drawn with its border");

    assert_eq!(
        frame.look(left, top),
        palette.dim_edge(),
        "the card is the surface that takes the keys, and it lights its edge for a reader who is \
         reading it: nobody is, so the edge recedes and the card's border carries it"
    );
}

#[test]
fn the_frame_behind_a_card_is_painted_and_dimmed() {
    let mut view = drafted();
    let before = frame::painted(&mut view, 120, 40);

    view.open_settings(&Config::default());
    let after = frame::painted(&mut view, 120, 40);

    // The card is seventy-two columns wide in a hundred and twenty, so its two gutters are all
    // that is left of the frame it was opened from.
    let words = (0..40)
        .flat_map(|y| (0..24).map(move |x| (x, y)))
        .filter(|(x, y)| before.cell(*x, *y).symbol() != " ")
        .count();
    assert!(
        words > 0,
        "the frame behind the card drew {words} glyphs in the gutter the card leaves it"
    );
    let band = after
        .told()
        .layout()
        .expect("a frame under a card is laid out whole")
        .composer
        .y;
    for y in 0..40 {
        for x in (0..24).chain(96..120) {
            let held = before.cell(x, y);
            let drawn = after.cell(x, y);
            assert_eq!(
                after.ink(x, y),
                Some(Ink::Dim),
                "row {y}, column {x} of the frame behind the card is not dimmed: the reader sees \
                 a frame they cannot reach, and this cell is on {:?}",
                drawn.bg
            );
            match y == band {
                true => {
                    assert_eq!(
                        drawn.symbol(),
                        " ",
                        "row {y}, column {x} of the band's head carries a glyph, and the head is \
                         the blank row of the one edge ink the card's scrim dims: {:?}",
                        after.row(y)
                    );
                    assert_eq!(
                        drawn.bg,
                        Palette::TRUECOLOR.edge_ink(),
                        "and the head the card leaves in its gutters is still the row of the one \
                         edge ink, at row {y}, column {x}"
                    );
                }
                false => {
                    assert_eq!(
                        drawn.symbol(),
                        held.symbol(),
                        "row {y}, column {x} is the frame the card was opened from, and it is \
                         still drawn: {:?}",
                        after.row(y)
                    );
                    assert_eq!(
                        drawn.bg, held.bg,
                        "and a cell behind the card keeps the band it sits on, at row {y}, \
                         column {x}"
                    );
                }
            }
        }
    }
}

#[test]
fn a_frame_under_fifty_columns_draws_the_card_full_bleed_with_its_title_row_and_nothing_else() {
    let frame = frame::painted(&mut on_screen(Mode::Keys), 49, 24);
    let screen = frame.rows().join("\n");

    let first = &frame.rows()[0];
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
        frame.rows()[1].starts_with("  keys"),
        "and the screen's rows are drawn under the title, inset by the card's own two columns: \
         {screen}"
    );
}

#[test]
fn the_card_covers_the_frame_behind_it_rather_than_letting_it_read_through() {
    let mut view = drafted();
    view.open_settings(&Config::default());
    let frame = frame::painted(&mut view, 120, 40);

    let inside = body_rows(&frame).join("\n");
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
    let frame = frame::painted(&mut on_screen(Mode::Keys), 120, 40);
    let body = body_rows(&frame);
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
    let frame = frame::painted(&mut on_screen(Mode::Keys), 120, 40);
    let body = body_rows(&frame);
    let does = Mode::Composer
        .table()
        .iter()
        .find(|binding| binding.name == "@")
        .expect("the empty draft binds `@` to the files of the workspace")
        .does;
    let first_words: Vec<&str> = does.split_whitespace().take(3).collect();
    let begins = body
        .iter()
        .position(|row| row.contains(&first_words.join(" ")))
        .unwrap_or_else(|| {
            panic!(
                "no row of the keys screen says `{}`: {body:#?}",
                first_words.join(" ")
            )
        });
    let column = body[begins]
        .find(&first_words.join(" "))
        .expect("the row the line begins on says its first words");
    let mut under = begins + 1;
    while under < body.len() && body[under].len() >= column {
        let lead: String = body[under].chars().take(column).collect();
        if !lead.trim().is_empty() || body[under].trim().is_empty() {
            break;
        }
        under += 1;
    }
    assert!(
        under > begins + 1,
        "the line `{}` is wider than the columns the card has left for it, so it takes more than \
         one row: {body:#?}",
        does
    );
    let wrapped: String = body[begins..under]
        .iter()
        .map(|row| row.chars().skip(column).collect::<String>())
        .collect::<Vec<String>>()
        .join(" ");
    assert_eq!(
        wrapped.split_whitespace().collect::<Vec<&str>>().join(" "),
        does,
        "and the rows under the first say the words it had no room for, in order and without \
         losing one: {body:#?}"
    );
    for row in &body[begins + 1..under] {
        assert!(
            row.chars().take(column).all(|glyph| glyph == ' '),
            "every row a description wraps onto is blank up to the column it begins in, so a \
             wrapped line hangs under the line above it rather than standing as a name of its own: \
             {row:?}"
        );
    }
}

#[test]
fn the_reason_a_value_was_refused_is_drawn_whole_however_narrow_the_card() {
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::none(),
    );
    view.open_settings(&Config::default());
    view.refuse("max_turns two hundred will not parse: invalid digit found in string");
    let frame = frame::painted(&mut view, 120, 40);
    let body = body_rows(&frame);
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
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::none(),
    );
    view.open_settings(&Config::default());
    view.refuse(reason);
    let frame = frame::painted(&mut view, 120, 40);
    let (top, bottom, left, _) = corners(&frame).expect("a card drawn with its border");
    let body = body_rows(&frame);
    let at = body
        .iter()
        .position(|row| row.trim_start().starts_with("ssh_agent"))
        .expect("the last key of the schema drawn in the card");
    let last = top + 1 + u16::try_from(at).expect("a row of the card");

    assert_eq!(
        said(&frame, last + 1),
        "",
        "a blank row stands under the last key, so the refusal reads as a verdict on the visit and \
         not as that key's value: {body:#?}"
    );

    let mut y = last + 2;
    let mut drawn = Vec::new();
    while y < bottom && !said(&frame, y).is_empty() {
        let row = said(&frame, y);
        let start = u16::try_from(row.len() - row.trim_start().len()).expect("a column");
        let words = row.trim_start();
        let inks: Vec<Ink> = (0..sindri::pen::cells(words))
            .map(|px| frame.look(left + 1 + start + px, y).ink())
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
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::none(),
    );
    view.open_settings(&Config::default());
    view.refuse(
        "bad value in max_turns: `two hundred` will not parse: invalid digit found in string",
    );
    let frame = frame::painted(&mut view, 120, 14);
    let body = body_rows(&frame);
    let drawn = frame.rows().join("\n");

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
            !drawn.contains(word),
            "a verdict the frame has no room for is not drawn at all rather than cut into `{word}`, \
             which would say a reason the reader cannot read: {drawn}"
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
    let frame = frame::painted(&mut on_screen(Mode::Keys), 120, 2);

    assert!(
        of_card(&frame, 0).contains("? keys"),
        "two rows hold the card's top border and its bottom one: {:?}",
        frame.row(0)
    );
    assert!(
        of_card(&frame, 1).contains(leaves(Mode::Keys)),
        "and the bottom border of a card with no body is still the row that says how to leave: \
         {:?}",
        frame.row(1)
    );
}

#[test]
fn a_name_too_wide_for_its_row_is_cut_at_the_rows_right_edge() {
    let name = "n".repeat(200);
    let (frame, _) = card_on(80, 6, &name, "", 4);

    let top = frame.row(0).trim().to_owned();
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
    let leaves = "k".repeat(100);
    let (frame, _) = card_on(80, 6, "?", &leaves, 4);

    let bottom = frame.row(5).trim().to_owned();
    assert_eq!(
        bottom,
        format!("╰{}╯", sindri::pen::filled(70, '─')),
        "a key no card row holds is refused whole rather than cut, and the row is its rule"
    );
}

#[test]
fn a_key_as_wide_as_the_bottom_row_allows_keeps_the_corners_it_hangs_between() {
    let leaves = "k".repeat(66);
    let (frame, _) = card_on(80, 6, "?", &leaves, 4);

    let bottom = frame.row(5).trim().to_owned();
    assert_eq!(
        bottom,
        format!("╰─ {} ─╯", "k".repeat(66)),
        "the widest key the bottom row holds hangs between a column of rule and the corners, \
         which it does not cover"
    );
}

/// The ellipsis a card marks a row it keeps back with.
const ELLIPSIS: char = '\u{22EF}';

/// A blank screen `cols` by `rows` with the card `name` and `leaves` painted into it, and the body
/// the paint hands the screen inside it.
fn card_on(cols: u16, rows: u16, name: &str, leaves: &str, holds: u16) -> (Frame, Rect) {
    let mut cells = Buffer::empty(Rect::new(0, 0, cols, rows));
    let edge = Palette::TRUECOLOR.accent_edge();
    let body = sindri::card::paint(
        cells.area,
        name,
        leaves,
        holds,
        &mut cells,
        edge,
        &Palette::TRUECOLOR,
    );
    (frame::holding(cells, Palette::TRUECOLOR), body)
}

/// The card a frame of `cols` by `rows` paints when the screen inside it holds `holds` rows, and
/// the band the screen's own rows are drawn in.
fn card_in(cols: u16, rows: u16, holds: u16) -> (Frame, Rect) {
    card_on(cols, rows, "? keys", "esc", holds)
}

/// Row `y` of the frame read through the `width` columns of `body`, with the blanks it leaves at
/// its end dropped.
fn words(drawn: &Frame, body: Rect, y: u16) -> String {
    drawn
        .row(y)
        .chars()
        .skip(usize::from(body.x))
        .take(usize::from(body.width))
        .collect::<String>()
        .trim_end()
        .to_owned()
}

#[test]
fn a_card_is_as_tall_as_its_body_and_its_two_borders_and_centred_in_the_frame() {
    let (frame, body) = card_in(80, 20, 4);
    assert_eq!(
        frame.glyph(4, 7),
        '╭',
        "a card holding four rows is six rows tall — its body and its two borders — and a frame \
         of twenty has a gap of fourteen to centre it in, whose spare row falls below: {:?}",
        frame.rows()
    );
    assert_eq!(
        frame.glyph(75, 12),
        '╯',
        "and its bottom border is the last of those six rows, seven rows below the first: {:?}",
        frame.rows()
    );
    assert_eq!(
        body,
        Rect::new(6, 8, 68, 4),
        "the body is the four rows the screen holds, under the top border and between the card's \
         two columns of inset: {:?}",
        frame.rows()
    );
    assert_eq!(
        frame.glyph(4, 6),
        ' ',
        "no row above the card is its own, so the frame under it is not covered: {:?}",
        frame.rows()
    );
    assert_eq!(
        frame.glyph(4, 13),
        ' ',
        "and no row under it is either: {:?}",
        frame.rows()
    );

    let (frame, body) = card_in(80, 20, 5);
    assert_eq!(
        frame.glyph(4, 6),
        '╭',
        "a card holding five rows is seven rows tall, and the gap of thirteen it is centred in \
         leaves its spare row below it, so its top border is row 6: {:?}",
        frame.rows()
    );
    assert_eq!(
        frame.glyph(75, 12),
        '╯',
        "and its bottom border is row 12, the same row a card one row shorter ends on: {:?}",
        frame.rows()
    );
    assert_eq!(
        body,
        Rect::new(6, 7, 68, 5),
        "the body is the five rows the screen holds, under the top border: {:?}",
        frame.rows()
    );
}

#[test]
fn a_body_taller_than_the_frame_is_capped_and_the_card_says_how_many_rows_it_keeps_back() {
    let (frame, body) = card_in(80, 20, 60);
    assert_eq!(
        frame.glyph(4, 0),
        '╭',
        "a body of sixty rows in a frame of twenty is capped at the frame, so its top border is \
         the frame's first row: {:?}",
        frame.rows()
    );
    assert_eq!(
        frame.glyph(75, 19),
        '╯',
        "and its bottom border the frame's last: {:?}",
        frame.rows()
    );
    assert_eq!(
        body,
        Rect::new(6, 1, 68, 17),
        "the eighteen rows between the borders give one up to the row that says how many it keeps \
         back, so the screen draws in seventeen of them: {:?}",
        frame.rows()
    );
    assert_eq!(
        words(&frame, body, 18),
        format!("{ELLIPSIS} 43 rows kept back"),
        "the row above the bottom border says how many of the sixty rows the card could not hold, \
         marked with the ellipsis the fold counts in: {:?}",
        frame.rows()
    );
    assert_eq!(
        frame.look(body.x, 18),
        Look::plain(Ink::Dim),
        "and it is said in the dim ink of the words a card keeps back: {:?}",
        frame.rows()
    );
    assert_eq!(frame.glyph(4, 19), '╰', "the bottom border is still drawn");
    assert_eq!(frame.glyph(75, 19), '╯', "with both its corners");
    assert!(
        frame.row(19).contains("esc"),
        "and it still names the key that leaves: {:?}",
        frame.row(19)
    );
}

#[test]
fn a_card_covers_no_row_of_the_frame_its_body_and_borders_do_not_need() {
    let mut cells = Buffer::empty(Rect::new(0, 0, 80, 20));
    for y in 0..20 {
        for x in 0..80 {
            if let Some(cell) = cells.cell_mut((x, y)) {
                cell.set_symbol("#");
            }
        }
    }
    let edge = Palette::TRUECOLOR.accent_edge();
    sindri::card::paint(
        cells.area,
        "? keys",
        "esc",
        4,
        &mut cells,
        edge,
        &Palette::TRUECOLOR,
    );
    let frame = frame::holding(cells, Palette::TRUECOLOR);

    for y in (0..7).chain(13..20) {
        assert_eq!(
            frame.row(y),
            "#".repeat(80),
            "row {y} of the frame is neither a border of the card nor a row of its body, so the \
             glyph the frame drew there is still the one on it: {:?}",
            frame.rows()
        );
    }
    let card: Vec<String> = (7..13)
        .map(|y| frame.row(y).chars().skip(4).take(72).collect::<String>())
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
    let (frame, body) = card_in(40, 24, 40);
    let first = frame.row(0);
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
        frame.rows()
    );
    assert_eq!(
        words(&frame, body, 23),
        format!("{ELLIPSIS} 18 rows kept back"),
        "the frame's last row says how many of the forty rows it could not hold: {:?}",
        frame.rows()
    );
    let screen = frame.rows().join("\n");
    for border in ['╭', '╮', '╰', '╯', '│'] {
        assert!(
            !screen.contains(border),
            "and no border is drawn, because the columns one costs are the reader's: {screen}"
        );
    }
}

#[test]
fn a_frame_too_short_for_a_body_draws_its_borders_and_counts_no_rows() {
    let (frame, body) = card_in(120, 2, 60);
    assert_eq!(
        body,
        Rect::new(26, 1, 68, 0),
        "two rows hold the card's top border and its bottom one, and nothing between them for a \
         body to be drawn in: {:?}",
        frame.rows()
    );
    assert!(
        frame.row(0).contains("? keys"),
        "so the top border names the screen: {:?}",
        frame.row(0)
    );
    let last = frame.row(1);
    assert_eq!(
        frame.glyph(24, 1),
        '╰',
        "and the bottom border is drawn under it: {last:?}"
    );
    assert!(last.contains("esc"), "naming the key that leaves: {last:?}");
    let screen = frame.rows().join("\n");
    assert!(
        !screen.contains("kept back"),
        "and no row of a card with none to draw the screen in says what it keeps back: {screen:?}"
    );
}

#[test]
fn every_screen_is_drawn_on_a_card_as_tall_as_the_rows_it_holds() {
    for (cols, tall) in [(120, 40), (80, 24)] {
        for mode in SCREENS {
            let frame = frame::painted(&mut on_screen(mode), cols, tall);
            let (top, bottom, ..) = corners(&frame).unwrap_or_else(|| {
                panic!(
                    "{} is drawn on a card with a border at {cols}x{tall}: {:?}",
                    name(mode),
                    frame.rows()
                )
            });
            let above = top - frame.area().y;
            let below = frame.area().bottom() - 1 - bottom;
            let held = bottom - top - 1;

            assert!(
                held == 0 || !said(&frame, bottom - 1).trim().is_empty(),
                "{}'s card at {cols}x{tall} ends where its body ends, so the last of the {held} \
                 rows it holds is the one above its bottom border and not a blank row it did not \
                 need: {:?}",
                name(mode),
                frame.rows()
            );
            assert!(
                above == below || above + 1 == below,
                "{}'s card at {cols}x{tall} is centred: {above} rows of frame above it and \
                 {below} below, and a card with no spare row carries the one it cannot share \
                 under it: {:?}",
                name(mode),
                frame.rows()
            );
            assert!(
                above + below > 0 || said(&frame, bottom - 1).contains("kept back"),
                "{}'s card at {cols}x{tall} is the whole frame, and a card is only as tall as the \
                 frame when its body is taller than the room it has: the row above its bottom \
                 border says how many rows it keeps back, where a card that is the frame's height \
                 and says nothing is the card this screen was drawn on before: {:?}",
                name(mode),
                frame.rows()
            );
        }
    }
}

#[test]
fn the_model_list_is_a_card_as_tall_as_the_names_it_holds() {
    let mut view = opened(conversation(), Mode::Models);
    view.handed(view.facts().clone().served_by(served()));
    let frame = frame::painted(&mut view, 120, 40);

    let (top, bottom, ..) = corners(&frame).expect("a card drawn with its border");
    assert_eq!(
        (top, bottom),
        (17, 21),
        "the card holding the query row and the two names a wire served is five rows — those \
         three and its two borders — and a frame of forty has a gap of thirty-five to centre it \
         in, whose spare row falls below: {:?}",
        frame.rows()
    );
    let named = of_card(&frame, top);
    assert!(
        named.starts_with("╭─ /model") && named.ends_with('╮'),
        "the top border wears the screen's own name: {named:?}"
    );
    assert_eq!(
        said(&frame, 18).trim_start(),
        "query:",
        "the query is the first row of the body: {:?}",
        frame.row(18)
    );
    assert!(
        said(&frame, 19).contains("claude-sonnet-4-6"),
        "and the name the wire served first is the row under it: {:?}",
        frame.row(19)
    );
    assert!(
        said(&frame, 20).contains("claude-haiku-4-5"),
        "the second name the wire served is the last row the card holds: {:?}",
        frame.row(20)
    );
    let leaving = of_card(&frame, bottom);
    assert!(
        leaving.starts_with('╰') && leaving.contains(leaves(Mode::Models)),
        "and the card ends on its bottom border, five rows under its top one: {leaving:?}"
    );
    assert!(
        corners(&frame).is_some_and(|(_, bottom, ..)| bottom == 21),
        "so the frame the card was opened from is on the screen again under it: {:?}",
        frame.row(22)
    );
}

#[test]
fn a_card_the_frame_cannot_hold_keeps_back_the_rows_it_does_not_draw() {
    let whole = frame::painted(&mut on_screen(Mode::Keys), 120, 40);
    let capped = frame::painted(&mut on_screen(Mode::Keys), 80, 24);

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
        capped.rows()
    );
    let kept = held.len() - drawn.len();
    let told: usize = count
        .split_whitespace()
        .nth(1)
        .and_then(|said| said.parse().ok())
        .unwrap_or_else(|| {
            panic!(
                "the card's own count row says how many rows it keeps back: {count:?}, {:?}",
                capped.rows()
            )
        });
    assert_eq!(
        told, kept,
        "and the rows it kept back are the difference between the rows the list holds and the rows \
         the card draws: {count:?}"
    );
}
