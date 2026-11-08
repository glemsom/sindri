//! The `/model` screen: the names the wire serves, ranked as a query is typed into the
//! screen, and the one `Enter` takes (§10, §12).
//!
//! The names here are the wire's own: every listing is read through the real listing code
//! from a recorded catalogue, so a test cannot pass on a model name this tree made up —
//! §10's one rule, and the one the no_hardcoded_models scan enforces over the source.
//!
//! What happens to a chosen name after the frame hands it on — the owner-only atomic save —
//! is `tests/tui.rs`'s, where a real file is written and read back.

use sindri::composer::{Key, Outcome};
use sindri::keys::Mode;
use sindri::models::{CAP, Models};
use sindri::view::View;

use sindri_core::provider::served::{Listing, Where};
use sindri_core::provider::transport::{Canned, Replay};
use sindri_core::provider::{Client, Credential};
use sindri_core::redact::SecretSet;

/// A recorded Anthropic `GET /v1/models` catalogue: two served names, as the wire listed
/// them, in its own order.
const ANTHROPIC_MODELS: &str =
    include_str!("../../sindri-core/tests/fixtures/providers/anthropic_models.json");

/// The first identifier the wire listed, named once so an assertion reads as the wire's
/// list rather than as a name this test invented.
const FIRST: &str = "claude-sonnet-4-6";

/// The second of the two, and the one a query narrows to.
const SECOND: &str = "claude-haiku-4-5";

/// The listing a wire that answered hands back, read through the listing code itself from
/// the recorded catalogue: the names, keeps and drops a live run would get, and no socket.
fn listed() -> Listing {
    let replay = Replay::new([Canned::reply(200, ANTHROPIC_MODELS)]);
    let at = Where::own(sindri_core::wire::Wire::Anthropic);
    Client::over(replay, SecretSet::default())
        .waits(|_| {})
        .served(&at, &Credential::new("sk-ant-recorded"), &at)
}

/// The listing a wire that could not be asked hands back: no names, and one line saying
/// where a credential comes from (§10's hint, never a refusal).
fn silent() -> Listing {
    let at = Where::own(sindri_core::wire::Wire::Anthropic);
    Client::over(Replay::new([]), SecretSet::default())
        .waits(|_| {})
        .served(&at, &Credential::none(), &at)
}

/// The identifiers a screen offers, in the order it offers them.
fn offered(models: &Models) -> Vec<String> {
    models
        .rows()
        .into_iter()
        .map(|(id, _)| id.to_owned())
        .collect()
}

/// A screen with `query` typed into it, a glyph at a time: what a reader sees after typing.
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
    // The wire's order puts `claude-sonnet-4-6` first, so a ranking that only filtered would
    // keep it first; `4` starts neither name and both hold it.
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
    // The recorded reply a wire would have sent, leaked so it is the `'static` text a canned
    // answer is: this catalogue is one a test wrote, and nothing reads it afterwards.
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

/// A frame with the `/model` screen up and the wire's names on it: the whole screen arrived
/// at the way a reader and a terminal arrive at it, one key and one listing at a time.
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
