//! What a screen keeps: the list a cursor movement walks, the query a typed glyph reaches and the row
//! a press chooses, each answered by the description of the screen rather than by matching on it.

mod caps; // the terminals these frames are drawn on

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use sindri::capability::Capabilities;
use sindri::commands;
use sindri::composer::{Key, Outcome};
use sindri::facts::Facts;
use sindri::ink::Ink;
use sindri::kept::{Kept, Move};
use sindri::keys::{Act, Mode, Stack};
use sindri::list;
use sindri::mark::Cell;
use sindri::pointer::Pointer;
use sindri::view::{Transcript, View};

use sindri_core::config::Config;
use sindri_core::provider::endpoint::Endpoint;
use sindri_core::provider::served::Listing;
use sindri_core::provider::transport::{Canned, Replay};
use sindri_core::provider::{Client, Credential};
use sindri_core::redact::SecretSet;
use sindri_core::wire::Wire;

const ANTHROPIC_MODELS: &str =
    include_str!("../../sindri-core/tests/fixtures/providers/anthropic_models.json");

/// The listing the model screen is handed: names a wire really served, read through the listing code.
fn listed() -> Listing {
    let at = Endpoint::own(Wire::Anthropic);
    Client::over(
        Replay::new([Canned::reply(200, ANTHROPIC_MODELS)]),
        SecretSet::default(),
    )
    .waits(|_| {})
    .served(&at, &Credential::new("sk-ant-recorded"), &at)
}

/// A workspace of `files`, standing where the picker walks it.
fn workspace(files: &[&str]) -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("a temp workspace");
    for file in files {
        let path = root.path().join(file);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("the directories a file sits in");
        }
        std::fs::write(&path, "").expect("a file");
    }
    root
}

/// A frame holding every list there is — a workspace the picker walks, the names the wire served and
/// the keys the config holds — with `mode` open on top of it.
fn screen(mode: Mode, capabilities: Capabilities) -> (tempfile::TempDir, View) {
    let root = workspace(&["src/rows.rs", "src/view.rs"]);
    let mut view = View::of(
        capabilities,
        Facts::default()
            .asked()
            .working(root.path().to_path_buf(), None),
        Transcript::none(),
    );
    match mode {
        Mode::Files => {
            view.key(Key::Char('@'));
        }
        Mode::Models => {
            view.paste("/model");
            view.key(Key::Enter);
        }
        Mode::Settings => view.open_settings(&Config::default()),
        other => panic!("{other:?} keeps no list, so no screen of it is opened here"),
    }
    // The model screen opens empty, so the wire's answer is handed to it once it stands.
    view.handed(view.facts().clone().served_by(listed()));
    assert_eq!(
        view.mode(),
        mode,
        "{:?} is the screen standing over it",
        mode
    );
    (root, view)
}

/// The three lists of one frame, in the order a test names them: the picker's, the served names' and
/// the visit's.
const LISTS: [Mode; 3] = [Mode::Files, Mode::Models, Mode::Settings];

/// The mark of each of the three lists: the picker's row, the model screen's and the visit's.
fn marks(view: &View) -> [usize; 3] {
    [
        view.picker().selected(),
        view.models().selected(),
        view.settings().current(),
    ]
}

/// What each of the three lists has typed into it: two queries and the visit's value on its row.
fn typed(view: &View) -> [String; 3] {
    let value = view
        .settings()
        .rows()
        .nth(view.settings().current())
        .map(|(_, text)| text.to_owned())
        .unwrap_or_default();
    [
        view.picker().query().to_owned(),
        view.models().query().to_owned(),
        value,
    ]
}

#[test]
fn one_act_types_into_the_list_a_screen_keeps() {
    let acts: Vec<Option<Act>> = LISTS
        .iter()
        .map(|mode| {
            let mut stack = Stack::new();
            stack.open(*mode);
            let glyph = stack.act(Key::Char('x'));
            assert_eq!(
                stack.act(Key::Backspace),
                glyph,
                "the glyph that erases and the glyph that types are one act at the {} screen, \
                 because one list is what receives both",
                mode.name()
            );
            glyph
        })
        .collect();

    assert!(
        acts.iter().all(|act| *act == acts[0] && act.is_some()),
        "the {} screens that keep a list type with one act, and answer {:?}: a screen that gains \
         a list has no act of its own to remember",
        LISTS.len(),
        acts
    );
}

#[test]
fn a_movement_walks_the_list_the_screen_keeps() {
    for (at, mode) in LISTS.into_iter().enumerate() {
        let (_root, mut view) = screen(mode, Capabilities::default());
        assert_eq!(
            marks(&view),
            [0; 3],
            "every list opens on its first row, so a mark that moved is a mark a key moved"
        );

        assert_eq!(
            view.key(Key::Down),
            Outcome::Edited,
            "`↓` is a mark moving at the {} screen, which is a frame to repaint",
            mode.name()
        );

        let mut want = [0; 3];
        want[at] = 1;
        assert_eq!(
            marks(&view),
            want,
            "`↓` moves the mark of the list the {} screen keeps and leaves the other two where \
             they were",
            mode.name()
        );
    }
}

#[test]
fn a_glyph_lands_in_the_list_the_screen_keeps() {
    for (at, mode) in LISTS.into_iter().enumerate() {
        let (_root, mut view) = screen(mode, Capabilities::default());

        assert_eq!(
            view.key(Key::Char('x')),
            Outcome::Edited,
            "a glyph typed at the {} screen changes a list, which is a frame to repaint",
            mode.name()
        );

        let mut want = [String::new(), String::new(), String::new()];
        want[at] = String::from("x");
        assert_eq!(
            typed(&view),
            want,
            "the glyph lands in the list the {} screen keeps and in no other",
            mode.name()
        );
    }
}

#[test]
fn a_press_on_a_row_of_the_visit_is_no_act() {
    let (_root, mut view) = screen(Mode::Settings, caps::pointing());
    let area = Rect::new(0, 0, 120, 40);
    let mut buffer = Buffer::empty(area);
    let told = view.paint(area, &mut buffer);
    let row = (area.y..area.bottom())
        .find(|y| {
            (area.x..area.right())
                .map(|x| buffer.cell((x, *y)).expect("a cell of the row").symbol())
                .collect::<String>()
                .contains("thinking")
        })
        .expect("the settings card draws the keys of the schema on the rows of its list");
    let before = marks(&view);

    assert_eq!(
        view.pointer(&told, Pointer::Press(Cell::new(row, 40))),
        Outcome::Unchanged,
        "a row of the visit hangs no choice, so a press on it writes no value and sends no key"
    );
    assert_eq!(
        marks(&view),
        before,
        "and the mark stands on the row the reader walked to rather than on the one that was \
         pressed: a visit's rows are no list a press moves through"
    );
    assert_eq!(
        view.mode(),
        Mode::Settings,
        "and the screen the visit is drawn on is still open"
    );
}

#[test]
fn the_commands_card_walks_the_rows_its_word_leaves() {
    let mut card = commands::Card::of("/s").expect("a card holds this word");

    assert_eq!(
        card.rows().iter().map(|row| row.name).collect::<Vec<_>>(),
        ["/settings", "/skill", "/stats"],
        "`/s` leaves the three commands a name of which holds it"
    );
    assert_eq!(
        card.selected(),
        0,
        "the mark opens on the row the words name rather than on a row the frame picked"
    );

    assert!(card.walked(Move::Next), "`↓` walks the card down a row");
    assert_eq!(card.selected(), 1, "the second row the word leaves");
    assert!(card.walked(Move::Next));
    assert_eq!(card.selected(), 2, "and the third");
    assert!(
        !card.walked(Move::Next),
        "a movement past the last row of the card moves nothing"
    );
    assert_eq!(card.selected(), 2, "so the mark stands where it stood");

    assert!(card.walked(Move::Previous), "`↑` walks it back");
    assert_eq!(card.selected(), 1);
}

#[test]
fn a_glyph_narrows_the_card_and_the_row_its_words_name_is_marked() {
    let mut card = commands::Card::of("/n").expect("a card holds this word");
    assert!(
        card.walked(Move::Next),
        "the card is walked off the row it opened on"
    );
    assert_eq!(card.selected(), 1, "which is `/settings`");

    assert!(
        card.typed(Key::Char('e')),
        "`e` changes the rows the card hands, which is a frame to paint"
    );

    assert_eq!(
        card.rows().iter().map(|row| row.name).collect::<Vec<_>>(),
        ["/new"],
        "`/ne` leaves the one command whose name starts the word"
    );
    assert_eq!(
        card.selected(),
        0,
        "and the row the words name is the row marked, not the row the mark was walked to"
    );

    for _ in 0.."/ne".len() {
        card.typed(Key::Backspace);
    }
    assert!(
        card.rows().is_empty(),
        "a word erased past the slash leaves no row at all"
    );
}

#[test]
fn a_press_marks_the_row_of_the_card_it_landed_on() {
    let mut card = commands::Card::of("/s").expect("a card holds this word");

    assert!(
        card.pressed(2),
        "a press on the third row of the card chooses that row"
    );
    assert_eq!(
        card.selected(),
        2,
        "and the mark stands where the press landed"
    );
    assert_eq!(
        card.spelling(),
        "/stats",
        "which is the line `Enter` then sends"
    );

    assert!(
        !card.pressed(3),
        "a press on a row the card does not hold chooses nothing"
    );
    assert_eq!(card.selected(), 2, "and marks nothing");

    let rows = card.body();
    assert_eq!(
        rows[2].spans[0].0,
        list::MARK,
        "the row the press landed on opens with the mark"
    );
    assert_eq!(
        rows[2].spans[0].1.ink(),
        Ink::Accent,
        "and is drawn in the accent, so the reader's eye lands on the row `Enter` takes"
    );
    for (at, row) in rows.iter().enumerate().filter(|(at, _)| *at != 2) {
        assert_ne!(row.spans[0].0, list::MARK, "row {at} wears no mark");
        assert_eq!(
            row.spans[0].1.ink(),
            Ink::Plain,
            "and the blank of the mark's columns stands in the ink of the row it opens"
        );
    }
}

#[test]
fn a_card_with_no_row_above_gives_the_up_arrow_back_to_the_draft() {
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::none(),
    );
    view.paste("queued");
    let Outcome::Send(prompt) = view.key(Key::Enter) else {
        panic!("a draft is a prompt once it is sent");
    };
    view.enqueue(prompt);
    view.paste("/s");

    assert_eq!(
        view.card().map(commands::Card::selected),
        Some(0),
        "the card opens on its first row, so it has no row above that one"
    );
    assert_eq!(
        view.key(Key::Up),
        Outcome::Edited,
        "`↑` is the draft's own where the card has nothing above the mark, so a queued prompt is \
         what answers it"
    );
    assert!(
        view.queue().is_empty(),
        "the prompt the queue held is the draft's again"
    );
}

#[test]
fn the_arrow_keys_walk_the_commands_card_over_the_draft() {
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::none(),
    );
    view.paste("/s");

    assert_eq!(
        view.card().map(commands::Card::selected),
        Some(0),
        "the draft opens the card on the row its word names"
    );
    assert_eq!(
        view.key(Key::Down),
        Outcome::Edited,
        "`↓` walks the card, which is a frame to paint"
    );
    assert_eq!(
        view.card().map(commands::Card::selected),
        Some(1),
        "and the mark stands on the second row the word leaves"
    );
    assert_eq!(view.key(Key::Up), Outcome::Edited, "`↑` walks it back");
    assert_eq!(view.card().map(commands::Card::selected), Some(0));
    assert_eq!(
        view.key(Key::Up),
        Outcome::Unchanged,
        "a movement off the first row of the card moves nothing"
    );

    assert_eq!(view.key(Key::Down), Outcome::Edited);
    assert_eq!(view.card().map(commands::Card::selected), Some(1));
    assert_eq!(
        view.key(Key::Char('e')),
        Outcome::Edited,
        "a glyph narrows the card the draft holds"
    );
    assert_eq!(
        view.card().map(commands::Card::selected),
        Some(0),
        "and typing after a walk puts the mark back on the row the words name"
    );
}
