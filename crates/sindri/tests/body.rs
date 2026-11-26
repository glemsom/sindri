mod caps;
mod frame;

use ratatui::layout::Rect;

use sindri::capability::Capabilities;
use sindri::composer::{Key, Outcome};
use sindri::keys::{Body, Mode};
use sindri::list::ScreenRow;
use sindri::view::{ScreenBody, Sheet, Transcript, View};
use sindri_core::config::Config;
use sindri_core::provider::endpoint::Endpoint;
use sindri_core::provider::served::Listing;
use sindri_core::provider::transport::{Canned, Replay};
use sindri_core::provider::{Client, Credential};
use sindri_core::redact::SecretSet;
use sindri_core::wire::Wire;

const EMPTY_CATALOGUE: &str = "{\"data\":[]}";

#[allow(dead_code)] // this binary reads the fixture's record, and paints no frame
mod fixture;

use fixture::{document, facts};

fn rows(body: &ScreenBody) -> &[ScreenRow] {
    let ScreenBody::Rows(rows) = body else {
        panic!("a card of rows is what a screen says, and this body is a ledger of facts");
    };
    rows
}

fn ledger(body: &ScreenBody) -> &Sheet {
    let ScreenBody::Ledger(sheet) = body else {
        panic!("a ledger is a screen of its own sheet, and this body is a card of rows");
    };
    sheet
}

fn said(body: &ScreenBody) -> Vec<String> {
    rows(body).iter().map(words).collect()
}

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
        View::of(
            Capabilities::default(),
            sindri::facts::Facts::default().asked(),
            Transcript::none()
        )
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
    let mut view = View::of(
        Capabilities::default(),
        sindri::facts::Facts::default()
            .asked()
            .working(root.path().to_path_buf(), None),
        Transcript::none(),
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
    let at = Endpoint::own(Wire::Anthropic);
    Client::over(
        Replay::new([Canned::reply(200, payload)]),
        SecretSet::default(),
    )
    .waits(|_| {})
    .served(&at, &Credential::new("sk-ant-recorded"), &at)
}

#[test]
fn the_served_names_body_asks_while_the_wire_is_unasked_and_says_so_when_it_served_nothing() {
    let mut view = View::of(
        Capabilities::default(),
        sindri::facts::Facts::default().asked(),
        Transcript::none(),
    );
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
fn the_settings_body_focuses_the_refused_field_and_keeps_actions_below_the_reason() {
    let mut view = View::of(
        Capabilities::default(),
        sindri::facts::Facts::default().asked(),
        Transcript::none(),
    );
    view.open_settings(&Config::default());
    let before: Vec<_> = view
        .settings()
        .rows()
        .map(|(key, value)| (key, value.to_owned()))
        .collect();
    let reason = "the schema refused a max_turns it cannot read";
    view.refuse(reason);
    let body = view.body(Mode::Settings, 60).expect("settings body");
    let words = said(&body);
    let refusal = words
        .iter()
        .position(|row| row == reason)
        .expect("visible refusal");
    let apply = words
        .iter()
        .position(|row| row.contains("Apply 0 changes"))
        .expect("apply action");
    assert!(refusal < apply);
    assert!(words.iter().any(|row| row.starts_with("› Maximum turns")));
    assert_eq!(
        view.settings()
            .rows()
            .map(|(key, value)| (key, value.to_owned()))
            .collect::<Vec<_>>(),
        before
    );
}

#[test]
fn the_ledger_body_holds_the_rows_it_laid_and_the_card_it_is_drawn_on_is_that_tall() {
    let root = tempfile::tempdir().expect("a temp workspace");
    let mut view = View::of(
        Capabilities::default(),
        sindri::facts::Facts::default()
            .asked()
            .working(root.path().to_path_buf(), None)
            .railed(facts())
            .recorded(&document()),
        Transcript::none(),
    );
    view.paste("/stats");
    assert_eq!(
        view.key(Key::Enter),
        Outcome::Edited,
        "a `/stats` line opens the screen it names, which is a frame to paint"
    );

    let tall = Rect::new(0, 0, 120, 60);
    let body = view
        .body(Mode::Stats, sindri::card::body_width(tall))
        .expect("the ledger is a screen, so it answers with a body");
    let laid = usize::from(ledger(&body).height());

    let card = frame::painted(&mut view, tall.width, tall.height)
        .told()
        .layout()
        .expect("a pass that drew a frame hands back the frame it laid out")
        .card
        .expect("the screen is drawn on a card, so the pass hands back the body it gave it");
    assert_eq!(
        usize::from(card.height),
        laid,
        "the card is as tall as the rows its body carries, because a frame taller than the sheet \
         caps nothing: the pass draws every row the body laid and no row it did not"
    );

    let small = Rect::new(0, 0, 80, 24);
    let card = frame::painted(&mut view, small.width, small.height)
        .told()
        .layout()
        .expect("a pass that drew a frame hands back the frame it laid out")
        .card
        .expect("a screen too short for the card still draws one");
    assert!(
        laid > usize::from(card.height),
        "a card the frame cannot hold draws the rows that fit and counts the rest back, so the \
         pass hands back fewer rows than the body carries and never a body trimmed to the frame: \
         {laid} rows laid, {} drawn",
        card.height
    );
}

#[test]
fn the_keys_body_hangs_no_choice_off_any_of_its_rows() {
    let mut view = View::of(
        Capabilities::default(),
        sindri::facts::Facts::default().asked(),
        Transcript::none(),
    );
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

#[test]
fn settings_groups_readable_fields_and_states_the_units_and_unset_values() {
    let mut view = View::of(
        Capabilities::default(),
        sindri::facts::Facts::default().asked(),
        Transcript::none(),
    );
    view.open_settings(&Config {
        timeout: Some(120),
        context_window: Some(200_000),
        thinking: Some(true),
        ..Config::default()
    });
    let body = view.body(Mode::Settings, 68).expect("settings body");
    let words = said(&body);
    for expected in ["CONNECTION", "RUN & LIMITS", "ACCESS & APPEARANCE"] {
        assert!(words.iter().any(|row| row == expected), "{words:#?}");
    }
    for expected in ["2 min", "200,000 tokens", "[on]", "Not set"] {
        assert!(words.iter().any(|row| row.contains(expected)), "{words:#?}");
    }
    let fields: Vec<usize> = rows(&body).iter().filter_map(|row| row.choice).collect();
    assert_eq!(fields, (0..18).collect::<Vec<_>>());
}
