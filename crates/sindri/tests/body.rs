//! What a screen's body holds, asked of the screen rather than read off a painted frame: each
//! description names the body its screen builds, and each screen builds it from the rows it keeps —
//! the line it says in place of rows, its count row and its refusal row among them.

mod caps; // the terminals these frames are drawn on

use sindri::capability::Capabilities;
use sindri::composer::{Key, Outcome};
use sindri::keys::{Body, Mode};
use sindri::ledger::Facts;
use sindri::list::ScreenRow;
use sindri::view::{ScreenBody, View};
use sindri_core::config::Config;
use sindri_core::provider::Route;
use sindri_core::provider::served::{Listing, Where};
use sindri_core::provider::transport::{Canned, Replay};
use sindri_core::provider::{Client, Credential};
use sindri_core::redact::SecretSet;
use sindri_core::run::{State as RunState, Stop};
use sindri_core::wire::Wire;

const EMPTY_CATALOGUE: &str = "{\"data\":[]}";

#[allow(dead_code)] // this binary reads the fixture's record, and paints no frame
mod fixture;

use fixture::{GUID, document, facts};

/// Every row of a body of rows.
fn rows(body: &ScreenBody) -> &[ScreenRow] {
    let ScreenBody::Rows(rows) = body else {
        panic!("a card of rows is what a screen says, and this body is a ledger of facts");
    };
    rows
}

/// The facts a ledger body lays out.
fn ledger(body: &ScreenBody) -> &Facts {
    let ScreenBody::Ledger(facts) = body else {
        panic!("a ledger is a screen of facts, and this body is a card of rows");
    };
    facts
}

/// The words of a body of rows, one string per row.
fn said(body: &ScreenBody) -> Vec<String> {
    rows(body).iter().map(words).collect()
}

/// The words one row says, its spans joined in the order the card draws them.
fn words(row: &ScreenRow) -> String {
    row.spans.iter().map(|(said, _)| said.as_str()).collect()
}

#[test]
fn every_screen_names_the_body_it_holds_and_the_draft_names_none() {
    let named: [(Mode, Option<Body>); 6] = [
        (Mode::Composer, None),
        (Mode::Keys, Some(Body::Keys)),
        (Mode::Files, Some(Body::Files)),
        (Mode::Models, Some(Body::Models)),
        (Mode::Settings, Some(Body::Settings)),
        (Mode::Stats, Some(Body::Ledger)),
    ];

    for mode in Mode::ALL {
        let body = named
            .iter()
            .find(|(screen, _)| *screen == mode)
            .map(|(_, body)| *body)
            .unwrap_or_else(|| {
                panic!(
                    "{mode:?} is a mode, so a test naming the bodies a frame draws has to say \
                     which one it holds: a mode named by none of them has no body to draw"
                )
            });
        assert_eq!(
            mode.body(),
            body,
            "the description of {mode:?} names the body its screen holds: the draft names none, \
             because it is no screen and takes no card"
        );
    }

    assert!(
        View::blank(Capabilities::default())
            .body(Mode::Composer, 60)
            .is_none(),
        "and the frame draws the body the screen names and no other: a draft asked for one is \
         answered with none, because there is no card to hand it"
    );
}

#[test]
fn the_path_picker_body_holds_the_query_row_first_and_the_count_of_matches_last() {
    let root = tempfile::tempdir().expect("a temp workspace");
    for file in ["src/rows.rs", "src/view.rs"] {
        let path = root.path().join(file);
        std::fs::create_dir_all(path.parent().expect("a file inside a directory"))
            .expect("a folder");
        std::fs::write(&path, "").expect("a file");
    }
    let mut view = View::blank(Capabilities::default());
    view.handed(
        view.facts()
            .clone()
            .working(root.path().to_path_buf(), None),
    );
    assert_eq!(
        view.key(Key::Char('@')),
        Outcome::Edited,
        "`@` on an empty draft opens the picker, which walks the workspace the frame stands in"
    );

    let body = view
        .body(Mode::Files, 60)
        .expect("the picker is a screen, so it answers with a body");
    let words = said(&body);

    assert_eq!(
        words.first().map(String::as_str),
        Some("query: "),
        "the row the query is typed into stands at the head of the body: {words:#?}"
    );
    assert_eq!(
        words.last().map(String::as_str),
        Some("1\u{2013}2 of 2"),
        "and the row under the files counts the matches the query left and the ones it shows, \
         both of them the two the workspace holds: {words:#?}"
    );
    let places: Vec<Option<usize>> = rows(&body).iter().map(|row| row.choice).collect();
    assert_eq!(
        places,
        vec![None, Some(0), Some(1), None],
        "each row a file is drawn on hangs the place a press chooses in its screen's list, and \
         the query row above them and the count row under them hang none: {words:#?}"
    );
}

fn served(payload: &'static str) -> Listing {
    let at = Where::own(Wire::Anthropic);
    Client::over(
        Replay::new([Canned::reply(200, payload)]),
        SecretSet::default(),
    )
    .waits(|_| {})
    .served(&at, &Credential::new("sk-ant-recorded"), &at)
}

#[test]
fn the_served_names_body_asks_while_the_wire_is_unasked_and_says_so_when_it_served_nothing() {
    let mut view = View::blank(Capabilities::default());
    let body = view
        .body(Mode::Models, 60)
        .expect("the served names are a screen and hold a body");
    assert_eq!(
        said(&body).last().map(String::as_str),
        Some("asking the wire what it serves"),
        "a wire that has yet to answer is one line saying it was asked, rather than a list with \
         nothing in it: {:#?}",
        said(&body)
    );

    view.handed(view.facts().clone().served_by(served(EMPTY_CATALOGUE)));
    let body = view
        .body(Mode::Models, 60)
        .expect("a screen handed an empty catalogue still holds a body");
    assert_eq!(
        said(&body).last().map(String::as_str),
        Some("no served name answers"),
        "a wire that answered serving nothing leaves the screen one line saying no name answers, \
         which is a different fact from a wire still to be asked: {:#?}",
        said(&body)
    );
}

#[test]
fn the_settings_body_ends_in_the_refusal_that_turned_the_visit_back() {
    let mut view = View::blank(Capabilities::default());
    view.open_settings(&Config::default());
    let body = view
        .body(Mode::Settings, 60)
        .expect("the settings screen holds a body of keys");
    let kept = said(&body);

    view.refuse("the schema refused a max_turns it cannot read");
    let body = view
        .body(Mode::Settings, 60)
        .expect("a refused visit is still the screen it was opened as");
    let words = said(&body);

    assert!(
        words.len() > kept.len() && words[..kept.len()] == kept[..],
        "a refusal adds a blank row and its own words under the keys, and changes no row above it, \
         so a visit that was turned back keeps every value it held: {words:#?} against {kept:#?}"
    );
    assert_eq!(
        words.last().map(String::as_str),
        Some("the schema refused a max_turns it cannot read"),
        "the refusal the frame was handed is the last words of the body, under the keys whose \
         visit it turned back: {words:#?}"
    );
    assert_eq!(
        words[words.len() - 2],
        "",
        "and a blank row sets it apart from the keys above it: {words:#?}"
    );
}

#[test]
fn the_ledger_body_holds_the_facts_the_record_wrote() {
    let mut view = View::blank(Capabilities::default());
    view.handed(view.facts().clone().railed(facts()).recorded(&document()));
    let body = view
        .body(Mode::Stats, 120)
        .expect("the ledger is a screen, so it answers with a body");
    let held = ledger(&body);

    assert_eq!(
        held.route,
        Some(Route::Chat),
        "the route the record wrote is the one the screen lays out, and the frame does not read it \
         off the document again"
    );
    assert_eq!(
        held.spend.total,
        Some(51_300),
        "so is what the run spent: every ask summed, which is the record's own total and not a \
         number the frame holds"
    );
    assert_eq!(
        held.state,
        Some(RunState::Stopped(Stop::Final)),
        "and the state the run ended on is the record's word for its stop"
    );
    assert_eq!(
        held.session.as_deref(),
        Some(GUID),
        "the session the frame was handed stands beside the record's facts, because the record \
         does not name it"
    );
}

#[test]
fn the_keys_body_hangs_no_choice_off_any_of_its_rows() {
    let mut view = View::blank(Capabilities::default());
    view.key(Key::Char('?'));
    let body = view
        .body(Mode::Keys, 60)
        .expect("the keys are a screen, so they answer with a body");

    assert!(
        rows(&body).len() > 1,
        "the keys screen names every open mode over the keys it binds, so it holds a row per key: \
         {:#?}",
        said(&body)
    );
    for row in rows(&body) {
        assert!(
            row.choice.is_none(),
            "a key is not a path or a name, so a press on its row chooses nothing: {:?}",
            words(row)
        );
    }
}
