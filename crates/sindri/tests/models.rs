//! The `/model` screen: the names the wire serves, ranked as a query is typed, and the one `Enter` takes.
//! Every listing is read through the real listing code from a recorded catalogue, so no test passes on a name
//! this tree made up. What happens to a chosen name afterwards is `tests/tui.rs`'s.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use sindri::composer::{Key, Outcome};
use sindri::keys::Mode;
use sindri::mark::Cell;
use sindri::models::{CAP, Models};
use sindri::pointer::Pointer;
use sindri::view::View;

use sindri_core::provider::served::{Listing, Where};
use sindri_core::provider::transport::{Canned, Replay};
use sindri_core::provider::{Client, Credential};
use sindri_core::redact::SecretSet;

const ANTHROPIC_MODELS: &str =
    include_str!("../../sindri-core/tests/fixtures/providers/anthropic_models.json");

const FIRST: &str = "claude-sonnet-4-6";

const SECOND: &str = "claude-haiku-4-5";

fn listed() -> Listing {
    let replay = Replay::new([Canned::reply(200, ANTHROPIC_MODELS)]);
    let at = Where::own(sindri_core::wire::Wire::Anthropic);
    Client::over(replay, SecretSet::default())
        .waits(|_| {})
        .served(&at, &Credential::new("sk-ant-recorded"), &at)
}

fn silent() -> Listing {
    let at = Where::own(sindri_core::wire::Wire::Anthropic);
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
    let mut models = Models::new();
    models.served(listed());
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

    models.served(listed());

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
fn a_name_the_query_starts_ranks_above_one_that_merely_holds_it() {
    // The wire's order puts `claude-sonnet-4-6` first, so a ranking that only filtered would keep it
    // first; `4` starts neither name and both hold it.
    let mut models = narrowed("4");
    assert_eq!(
        offered(&models),
        [String::from(FIRST), String::from(SECOND)],
        "both names hold the query and neither starts with it, so the wire's own order \
         settles what the rank cannot"
    );

    models.key(Key::Backspace);
    for glyph in "claude-h".chars() {
        models.key(Key::Char(glyph));
    }

    assert_eq!(
        offered(&models),
        [String::from(SECOND)],
        "a query that starts one name and not the other leaves that one"
    );
}

#[test]
fn a_query_that_starts_a_later_name_puts_it_first() {
    let mut models = Models::new();
    models.served(listed());
    for glyph in SECOND.chars() {
        models.key(Key::Char(glyph));
    }

    assert_eq!(models.chosen(), Some(SECOND));
}

#[test]
fn a_row_the_list_holds_is_chosen_by_a_press_on_it_and_one_it_does_not_holds_nothing() {
    let mut models = Models::new();
    models.served(listed());
    let rows = models.rows().len();

    assert!(
        models.select(1),
        "the second row of the list is a press the screen takes"
    );
    assert_eq!(
        models.chosen(),
        Some(SECOND),
        "and the selection moves to the row the press landed on"
    );
    assert!(
        !models.select(rows + 7),
        "a press past the last row is a row no list holds"
    );
    assert_eq!(
        models.selected(),
        1,
        "so the selection stays on the row it was"
    );
}

#[test]
fn the_selection_walks_the_rows_and_stops_at_the_ends() {
    let mut models = Models::new();
    models.served(listed());

    assert!(!models.up(), "there is no row above the first");
    assert!(models.down());
    assert_eq!(models.selected(), 1);
    assert_eq!(
        models.chosen(),
        Some(SECOND),
        "and the row the selection is on is the name a key would write"
    );
    assert!(!models.down(), "nor below the last");
}

#[test]
fn a_narrower_query_puts_the_selection_back_on_the_best_match() {
    let mut models = Models::new();
    models.served(listed());
    models.down();

    for glyph in "haiku".chars() {
        models.key(Key::Char(glyph));
    }

    assert_eq!(
        models.selected(),
        0,
        "a query that leaves a shorter list cannot keep a row that may be gone, and the best \
         match the query leaves is the one a reader means"
    );
}

#[test]
fn a_backspace_widens_the_list_again() {
    let mut models = narrowed("haiku");
    assert_eq!(offered(&models), [String::from(SECOND)]);

    for _ in 0.."haiku".len() {
        assert!(models.key(Key::Backspace));
    }

    assert_eq!(
        offered(&models),
        [String::from(FIRST), String::from(SECOND)],
        "erasing the query a glyph at a time widens the list back to the names the wire \
         served, in the order it served them"
    );
    assert_eq!(models.query(), "");
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
    let at = Where::own(sindri_core::wire::Wire::Anthropic);
    let listing = Client::over(replay, SecretSet::default())
        .waits(|_| {})
        .served(&at, &Credential::new("sk-ant-recorded"), &at);
    let mut models = Models::new();
    models.served(listing);

    assert_eq!(models.rows().len(), CAP);
}

#[test]
fn a_wire_that_could_not_answer_offers_its_hint_and_no_names() {
    let mut models = Models::new();
    models.served(silent());

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
fn a_key_a_filter_box_has_no_use_for_moves_nothing() {
    let mut models = Models::new();
    models.served(listed());

    for key in [Key::Left, Key::Home, Key::Enter, Key::Quit, Key::Cancel] {
        assert!(
            !models.key(key),
            "{key:?} is not a glyph and not an erasure, so the filter box is not the key's \
             to answer"
        );
    }
    assert_eq!(models.query(), "");
}

fn screen() -> View {
    let mut view = View::boot();
    view.paste("/model");
    view.key(Key::Enter);
    view.set_served(listed());
    assert_eq!(view.mode(), Mode::Models);
    view
}

#[test]
fn enter_takes_the_chosen_name_and_closes_the_screen() {
    let mut view = screen();

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

/// The row of a 120 by 40 frame the screen draws `said` on, painted on the way: a press lands on the
/// cells a row was drawn in, so the cells are what a test reads.
fn row_holding(view: &mut View, said: &str) -> u16 {
    let area = Rect::new(0, 0, 120, 40);
    let mut buffer = Buffer::empty(area);
    view.paint(area, &mut buffer);
    (area.y..area.bottom())
        .find(|y| {
            (area.x..area.right())
                .map(|x| buffer.cell((x, *y)).expect("a cell of the row").symbol())
                .collect::<String>()
                .contains(said)
        })
        .unwrap_or_else(|| panic!("the screen draws {said:?} on no row of it"))
}

#[test]
fn a_press_on_a_served_row_names_that_name_and_closes_the_screen() {
    let mut view = screen();
    view.set_pointer(true);
    let row = row_holding(&mut view, SECOND);

    assert_eq!(
        view.pointer(Pointer::Press(Cell::new(row, 40))),
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
    let mut view = screen();
    view.set_pointer(true);
    let query = row_holding(&mut view, "query:");
    let name = row_holding(&mut view, SECOND);

    for (at, why) in [
        (Cell::new(query, 40), "the row the query is typed on"),
        (Cell::new(name, 5), "a cell outside the card"),
    ] {
        assert_eq!(
            view.pointer(Pointer::Press(at)),
            Outcome::Unchanged,
            "a press on {why} chooses no row, so no model is named"
        );
        assert_eq!(view.mode(), Mode::Models, "and the screen stays open");
    }
}

#[test]
fn a_key_at_the_model_screen_reaches_no_draft() {
    let mut view = screen();

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
    let mut view = screen();

    assert_eq!(view.key(Key::Escape), Outcome::Edited);
    assert_eq!(view.mode(), Mode::Composer);
}

#[test]
fn a_query_that_leaves_no_row_leaves_the_screen_open() {
    let mut view = screen();
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
