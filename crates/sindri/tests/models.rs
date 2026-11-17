//! The `/model` screen: the names the wire serves, ranked as a query is typed, and the one `Enter` takes.
//! Every listing is read through the real listing code from a recorded catalogue, so no test passes on a name
//! this tree made up. What happens to a chosen name afterwards is `tests/tui.rs`'s.

mod caps; // the terminals these frames are drawn on

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use sindri::capability::Capabilities;
use sindri::composer::{Key, Outcome};
use sindri::facts::Facts;
use sindri::kept::{CAP, Kept, Move};
use sindri::keys::Mode;
use sindri::mark::Cell;
use sindri::models::Models;
use sindri::pointer::Pointer;
use sindri::view::{Painted, Transcript, View};

use sindri_core::provider::endpoint::Endpoint;
use sindri_core::provider::served::Listing;
use sindri_core::provider::transport::{Canned, Replay};
use sindri_core::provider::{Client, Credential};
use sindri_core::redact::SecretSet;

const ANTHROPIC_MODELS: &str =
    include_str!("../../sindri-core/tests/fixtures/providers/anthropic_models.json");

const FIRST: &str = "claude-sonnet-4-6";

const SECOND: &str = "claude-haiku-4-5";

fn listed() -> Listing {
    let replay = Replay::new([Canned::reply(200, ANTHROPIC_MODELS)]);
    let at = Endpoint::own(sindri_core::wire::Wire::Anthropic);
    Client::over(replay, SecretSet::default())
        .waits(|_| {})
        .served(&at, &Credential::new("sk-ant-recorded"), &at)
}

fn silent() -> Listing {
    let at = Endpoint::own(sindri_core::wire::Wire::Anthropic);
    Client::over(Replay::new([]), SecretSet::default())
        .waits(|_| {})
        .served(&at, &Credential::none(), &at)
}

fn offered(models: &Models) -> Vec<String> {
    models
        .rows()
        .into_iter()
        .map(|(id, _)| id.to_owned())
        .collect()
}

fn narrowed(query: &str) -> Models {
    let mut models = Models::new().served_by(Some(listed()));
    for glyph in query.chars() {
        models.key(Key::Char(glyph));
    }
    models
}

#[test]
fn the_rows_are_the_names_the_wire_served_in_the_order_it_served_them() {
    let mut models = Models::new();
    assert!(models.waiting(), "a screen opens before the wire answers");
    assert!(offered(&models).is_empty(), "and offers nothing yet");

    models = models.served_by(Some(listed()));

    assert!(!models.waiting());
    assert_eq!(
        offered(&models),
        [String::from(FIRST), String::from(SECOND)],
        "the wire's own names, in its own order: the screen offers what it was served and \
         nothing it invented (§10)"
    );
    assert_eq!(
        models.chosen(),
        Some(FIRST),
        "and a screen opens on the first name the wire listed"
    );
}

#[test]
fn a_query_narrows_the_rows_as_it_is_typed() {
    assert_eq!(
        offered(&narrowed("ha")),
        [String::from(SECOND)],
        "a query is read as part of a name, in either case, and only one served name holds \
         `ha`"
    );
}

#[test]
fn a_query_that_reaches_one_name_leaves_that_name_to_choose() {
    let models = narrowed(SECOND);

    assert_eq!(
        models.chosen(),
        Some(SECOND),
        "one name answers the query and the card draws it first, so `Enter` takes it"
    );
}

#[test]
fn a_query_that_matches_no_served_name_takes_no_name() {
    let models = narrowed("a-name-the-wire-never-served");

    assert!(offered(&models).is_empty());
    assert_eq!(
        models.chosen(),
        None,
        "the screen offers what the wire served, so a name it never offered is one the \
         reader writes with `/model <name>` instead"
    );
}

#[test]
fn the_screen_offers_eight_rows_and_no_more() {
    let names: Vec<String> = (0..20).map(|at| format!("a-served-name-{at}")).collect();
    let catalogue = format!(
        "{{\"data\":[{}],\"has_more\":false}}",
        names
            .iter()
            .map(|name| format!(
                "{{\"type\":\"model\",\"id\":\"{name}\",\"display_name\":\"{name}\"}}"
            ))
            .collect::<Vec<String>>()
            .join(",")
    );
    // Leaked so it is the `'static` text a canned answer needs: this catalogue is one a test wrote, and
    // nothing reads it afterwards.
    let replay = Replay::new([Canned::reply(200, Box::leak(catalogue.into_boxed_str()))]);
    let at = Endpoint::own(sindri_core::wire::Wire::Anthropic);
    let listing = Client::over(replay, SecretSet::default())
        .waits(|_| {})
        .served(&at, &Credential::new("sk-ant-recorded"), &at);
    let models = Models::new().served_by(Some(listing));

    assert_eq!(models.rows().len(), CAP);
}

#[test]
fn a_wire_that_could_not_answer_offers_its_hint_and_no_names() {
    let models = Models::new().served_by(Some(silent()));

    assert!(offered(&models).is_empty(), "an unasked wire has no names");
    assert!(
        models
            .hint()
            .is_some_and(|hint| hint.contains("ANTHROPIC_API_KEY")),
        "and §10's hint says where a credential comes from: {:?}",
        models.hint()
    );
    assert_eq!(models.chosen(), None);
}

#[test]
fn a_listing_the_screen_has_not_held_puts_the_mark_back_on_the_first_row() {
    let mut models = Models::new().served_by(Some(listed()));
    assert!(models.walked(Move::Next));
    assert_eq!(models.selected(), 1, "the reader walked down one row");

    let held = models.served_by(Some(listed()));
    assert_eq!(
        held.selected(),
        1,
        "the listing the screen already holds leaves the reader where they were"
    );

    let fresh = held.served_by(Some(silent()));
    assert_eq!(
        fresh.selected(),
        0,
        "and a listing that is not the one it holds puts the mark back on the first row"
    );
}

fn screen(capabilities: Capabilities) -> View {
    let mut view = View::of(capabilities, Facts::default().asked(), Transcript::none());
    view.paste("/model");
    view.key(Key::Enter);
    view.handed(view.facts().clone().served_by(listed()));
    assert_eq!(view.mode(), Mode::Models);
    view
}

#[test]
fn a_screen_opened_again_asks_the_wire_rather_than_serving_the_names_it_held() {
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::none(),
    );
    view.handed(view.facts().clone().served_by(listed()));
    view.paste("/model");
    view.key(Key::Enter);

    let rows = frame(&mut view, 120, 40);
    assert!(
        rows.iter()
            .any(|row| row.contains("asking the wire what it serves")),
        "the names a screen held are the ones it was asked for, and asking again says so: {rows:#?}"
    );
}

#[test]
fn enter_takes_the_chosen_name_and_closes_the_screen() {
    let mut view = screen(Capabilities::default());

    assert_eq!(
        view.key(Key::Enter),
        Outcome::Model(String::from(FIRST)),
        "the name under the selection is the model, and the terminal is what writes it"
    );
    assert_eq!(
        view.mode(),
        Mode::Composer,
        "and the screen that was up is closed: the name is taken and there is nothing left \
         to choose"
    );
}

/// Every row of a frame `cols` by `rows`, as the glyphs it draws, painted on the way.
fn frame(view: &mut View, cols: u16, rows: u16) -> Vec<String> {
    let area = Rect::new(0, 0, cols, rows);
    let mut buffer = Buffer::empty(area);
    view.paint(area, &mut buffer);
    (area.y..area.bottom())
        .map(|y| {
            (area.x..area.right())
                .map(|x| buffer.cell((x, y)).expect("a cell of the row").symbol())
                .collect()
        })
        .collect()
}

/// The rows a card's two borders hold, each as the words it says.
fn body(frame: &[String]) -> Vec<String> {
    let top = frame
        .iter()
        .position(|row| row.contains('\u{256d}'))
        .expect("a card drawn with its top border");
    let bottom = frame
        .iter()
        .rposition(|row| row.contains('\u{2570}'))
        .expect("a card drawn with its bottom border");
    let border: Vec<char> = frame[top].chars().collect();
    let sides = (
        border.iter().position(|glyph| *glyph == '\u{256d}'),
        border.iter().rposition(|glyph| *glyph == '\u{256e}'),
    );
    frame[top + 1..bottom]
        .iter()
        .map(|said| {
            let glyphs: Vec<char> = said.chars().collect();
            match sides {
                (Some(left), Some(right))
                    if glyphs.get(left) == Some(&'\u{2502}')
                        && glyphs.get(right) == Some(&'\u{2502}') =>
                {
                    glyphs[left + 1..right].iter().collect::<String>()
                }
                _ => said.clone(),
            }
            .trim()
            .to_owned()
        })
        .collect()
}

/// The row of a 120 by 40 frame the screen draws `said` on, and what that pass told the terminal:
/// a press lands on the cells a row was drawn in, so the cells are what a test reads.
fn row_holding(view: &mut View, said: &str) -> (u16, Painted) {
    let area = Rect::new(0, 0, 120, 40);
    let mut buffer = Buffer::empty(area);
    let told = view.paint(area, &mut buffer);
    let row = (area.y..area.bottom())
        .find(|y| {
            (area.x..area.right())
                .map(|x| buffer.cell((x, *y)).expect("a cell of the row").symbol())
                .collect::<String>()
                .contains(said)
        })
        .unwrap_or_else(|| panic!("the screen draws {said:?} on no row of it"));
    (row, told)
}

#[test]
fn a_press_on_a_served_row_names_that_name_and_closes_the_screen() {
    let mut view = screen(caps::pointing());
    let (row, told) = row_holding(&mut view, SECOND);

    assert_eq!(
        view.pointer(&told, Pointer::Press(Cell::new(row, 40))),
        Outcome::Model(String::from(SECOND)),
        "a press on the second row names the name that row hangs from, as `Enter` on it does"
    );
    assert_eq!(
        view.mode(),
        Mode::Composer,
        "and the screen it was chosen from is closed, as the key closes it"
    );
}

#[test]
fn a_press_on_no_row_of_the_model_screen_names_nothing() {
    let mut view = screen(caps::pointing());
    let (query, _) = row_holding(&mut view, "query:");
    let (name, told) = row_holding(&mut view, SECOND);

    for (at, why) in [
        (Cell::new(query, 40), "the row the query is typed on"),
        (Cell::new(name, 5), "a cell outside the card"),
    ] {
        assert_eq!(
            view.pointer(&told, Pointer::Press(at)),
            Outcome::Unchanged,
            "a press on {why} chooses no row, so no model is named"
        );
        assert_eq!(view.mode(), Mode::Models, "and the screen stays open");
    }
}

#[test]
fn a_key_at_the_model_screen_reaches_no_draft() {
    let mut view = screen(Capabilities::default());

    for key in [Key::Char('x'), Key::Enter, Key::Down] {
        view.key(key);
    }

    assert_eq!(
        view.composer().text(),
        "",
        "a screen is the whole frame, and a glyph typed at it narrows a list rather than \
         composing a prompt nobody can see (§12, ADR-0017)"
    );
}

#[test]
fn escape_closes_the_screen_and_names_no_model() {
    let mut view = screen(Capabilities::default());

    assert_eq!(view.key(Key::Escape), Outcome::Edited);
    assert_eq!(view.mode(), Mode::Composer);
}

#[test]
fn a_query_that_leaves_no_row_leaves_the_screen_open() {
    let mut view = screen(Capabilities::default());
    for glyph in "a-name-the-wire-never-served".chars() {
        view.key(Key::Char(glyph));
    }

    assert_eq!(
        view.key(Key::Enter),
        Outcome::Unchanged,
        "a key that takes nothing is not a frame that changed, and a screen closing on it \
         would be a screen that swallowed the query"
    );
    assert_eq!(view.mode(), Mode::Models);
}

#[test]
fn a_band_too_short_for_the_served_names_draws_the_head_of_the_list_and_counts_the_rest() {
    let mut view = screen(Capabilities::default());
    let held = body(&frame(&mut view, 120, 40));
    let drawn = body(&frame(&mut view, 120, 4));

    assert_eq!(
        held.len(),
        3,
        "the query and the two names the wire served are the whole list: {held:?}"
    );
    assert_eq!(
        drawn.len(),
        2,
        "a frame of four rows holds the card's two borders and two rows of the body, and a card \
         that cannot hold the list spends one of them counting the rows it keeps back: {drawn:?}"
    );
    assert_eq!(
        drawn[0], held[0],
        "so the row a reader is still shown is the head of the list, in the words it says in a \
         frame that holds all of it"
    );
    assert_eq!(
        drawn[1], "\u{22ef} 2 rows kept back",
        "and the row under it says how many of the three the card could not hold: {drawn:?}"
    );
    assert!(
        !frame(&mut view, 120, 4)
            .iter()
            .any(|row| row.contains(FIRST)),
        "and no name is drawn past the band the card gave the list: {drawn:?}"
    );
}
