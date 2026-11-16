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
use sindri::models::Models;
use sindri::picker::Picker;
use sindri::pointer::Pointer;
use sindri::settings::Settings;
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

/// The four lists there are, stood on a bench: the picker's files, the names the wire served, the
/// config's keys and the card a draft opens, each answering the seam as the frame asks it.
enum List {
    Files(Picker),
    Models(Models),
    Settings(Settings),
    Card(commands::Card),
}

/// A workspace of four files, one of which a query can take away from under the mark.
fn files() -> List {
    let root = workspace(&["a1.rs", "b1.rs", "m.rs", "zz.rs"]);
    List::Files(Picker::walk(root.path()))
}

/// The names a recorded wire served, which are two.
fn served() -> List {
    List::Models(Models::new().served_by(Some(listed())))
}

/// The config's keys, which are the rows a visit holds and the one list with no query.
fn visit() -> List {
    List::Settings(Settings::of(&Config::default()))
}

/// The card a bare slash opens, which holds every command in one order.
fn card() -> List {
    List::Card(commands::Card::of("/").expect("a bare slash holds every command"))
}

impl List {
    fn name(&self) -> &'static str {
        match self {
            List::Files(_) => "picker",
            List::Models(_) => "served names",
            List::Settings(_) => "visit",
            List::Card(_) => "card",
        }
    }

    /// The rows the list holds, each as the word it spells.
    fn rows(&self) -> Vec<String> {
        match self {
            List::Files(picker) => picker
                .rows()
                .into_iter()
                .map(|row| match row.directory {
                    "" => row.name.to_owned(),
                    directory => format!("{directory}/{}", row.name),
                })
                .collect(),
            List::Models(models) => models
                .rows()
                .into_iter()
                .map(|(id, _)| id.to_owned())
                .collect(),
            List::Settings(settings) => settings.rows().map(|(key, _)| key.to_owned()).collect(),
            List::Card(card) => card
                .rows()
                .into_iter()
                .map(|row| row.name.to_owned())
                .collect(),
        }
    }

    /// The mark, which is the row the reader stands on.
    fn mark(&self) -> usize {
        match self {
            List::Files(picker) => picker.selected(),
            List::Models(models) => models.selected(),
            List::Settings(settings) => settings.current(),
            List::Card(card) => card.selected(),
        }
    }

    /// The word the row the mark stands on spells.
    fn marked(&self) -> Option<String> {
        self.rows().into_iter().nth(self.mark())
    }

    /// The glyphs of `asked`, typed at the list one at a time.
    fn ask(&mut self, asked: &str) {
        for glyph in asked.chars() {
            assert!(
                self.typed(Key::Char(glyph)),
                "{glyph:?} changes the rows the {} list hands, which is a frame to paint",
                self.name()
            );
        }
    }
}

impl Kept for List {
    fn walked(&mut self, way: Move) -> bool {
        match self {
            List::Files(picker) => picker.walked(way),
            List::Models(models) => models.walked(way),
            List::Settings(settings) => settings.walked(way),
            List::Card(card) => card.walked(way),
        }
    }

    fn typed(&mut self, key: Key) -> bool {
        match self {
            List::Files(picker) => picker.typed(key),
            List::Models(models) => models.typed(key),
            List::Settings(settings) => settings.typed(key),
            List::Card(card) => card.typed(key),
        }
    }

    fn pressed(&mut self, at: usize) -> bool {
        match self {
            List::Files(picker) => picker.pressed(at),
            List::Models(models) => models.pressed(at),
            List::Settings(settings) => settings.pressed(at),
            List::Card(card) => card.pressed(at),
        }
    }
}

#[test]
fn the_mark_stops_at_the_first_row_and_at_the_last_of_every_list() {
    for mut list in [files(), served(), visit(), card()] {
        let name = list.name();
        let last = list.rows().len() - 1;
        assert!(
            last > 0,
            "the {name} list holds a row under the first, so the ends are two ends"
        );
        assert_eq!(
            list.mark(),
            0,
            "the {name} list opens its mark on the first row"
        );
        assert!(
            !list.walked(Move::Previous),
            "no row of the {name} list sits above the first, so `↑` moves nothing and paints nothing"
        );
        assert_eq!(
            list.mark(),
            0,
            "and the mark of the {name} list stands where it stood"
        );

        for at in 0..last {
            assert!(
                list.walked(Move::Next),
                "the {name} list holds a row under the mark at {at}"
            );
            assert_eq!(
                list.mark(),
                at + 1,
                "and the mark of the {name} list walks to it"
            );
        }
        assert!(
            !list.walked(Move::Next),
            "nor is there a row of the {name} list under the last, so `↓` moves nothing"
        );
        assert_eq!(
            list.mark(),
            last,
            "so the mark of the {name} list stops on its last row"
        );
    }
}

#[test]
fn an_edit_that_still_reaches_the_mark_leaves_it_where_the_reader_put_it() {
    let mut picker = files();
    assert!(picker.walked(Move::Next));
    assert!(picker.walked(Move::Next));
    assert_eq!(
        picker.marked().as_deref(),
        Some("m.rs"),
        "the mark of the picker stands on the third of the four files"
    );
    picker.ask("rs");
    assert_eq!(
        picker.rows().len(),
        4,
        "every path of the workspace holds `rs`, so the query leaves them all"
    );
    assert_eq!(
        picker.marked().as_deref(),
        Some("m.rs"),
        "and the mark of the picker stands on the file the reader pointed at"
    );

    let mut models = served();
    assert!(models.walked(Move::Next));
    assert_eq!(models.marked().as_deref(), Some("claude-haiku-4-5"));
    models.ask("claude-");
    assert_eq!(
        models.rows().len(),
        2,
        "both served names hold `claude-`, in the wire's own order"
    );
    assert_eq!(
        models.marked().as_deref(),
        Some("claude-haiku-4-5"),
        "so the mark of the served names stands on the name the reader pointed at"
    );

    let mut card = card();
    assert!(card.walked(Move::Next));
    assert_eq!(
        card.marked().as_deref(),
        Some("/model"),
        "the mark of the card stands on the second row a bare slash leaves"
    );
    card.ask(":");
    assert_eq!(
        card.rows().len(),
        commands::TABLE.len(),
        "a colon names a skill's word without narrowing the rows, so every command is left"
    );
    assert_eq!(
        card.marked().as_deref(),
        Some("/model"),
        "and the mark of the card stands on the row the reader pointed at"
    );
}

#[test]
fn an_edit_that_re_ranks_the_rows_leaves_the_mark_at_the_index_it_stood_at() {
    let root = workspace(&["abe.rs", "bob.rs"]);
    let mut picker = List::Files(Picker::walk(root.path()));
    assert!(picker.walked(Move::Next));
    assert_eq!(
        picker.marked().as_deref(),
        Some("bob.rs"),
        "the mark stands on the second of the two files"
    );

    picker.ask("b");

    assert_eq!(
        picker.rows(),
        [String::from("bob.rs"), String::from("abe.rs")],
        "`b` is the letter `bob.rs` starts and `abe.rs` merely holds, so the query turns the \
         two rows around"
    );
    assert_eq!(
        picker.mark(),
        1,
        "and the mark keeps the place the reader put it in: the eye stays where it was, and \
         the row underneath it is the query's to change"
    );
}

#[test]
fn an_edit_that_takes_the_marked_row_away_leaves_the_mark_on_the_nearest_row() {
    let mut picker = files();
    for _ in 0..3 {
        assert!(picker.walked(Move::Next));
    }
    assert_eq!(picker.marked().as_deref(), Some("zz.rs"));
    picker.ask("1");
    assert_eq!(
        picker.rows(),
        [String::from("a1.rs"), String::from("b1.rs")],
        "`1` takes the marked file away and leaves two above it"
    );
    assert_eq!(
        picker.marked().as_deref(),
        Some("b1.rs"),
        "so the mark of the picker stands on the nearest row kept rather than on the first"
    );

    let mut models = served();
    assert!(models.walked(Move::Next));
    models.ask("so");
    assert_eq!(
        models.rows(),
        [String::from("claude-sonnet-4-6")],
        "`so` takes the served name the mark stands on away and leaves the other"
    );
    assert_eq!(
        models.marked().as_deref(),
        Some("claude-sonnet-4-6"),
        "so the mark of the served names stands on the nearest row kept, which is the last"
    );

    let mut card = card();
    assert!(card.walked(Move::Next));
    assert!(card.walked(Move::Next));
    card.ask("t");
    assert_eq!(
        card.rows(),
        [
            String::from("/quit"),
            String::from("/settings"),
            String::from("/stats")
        ],
        "`/t` takes `/new` away and leaves the three names that hold it"
    );
    assert_eq!(
        card.marked().as_deref(),
        Some("/stats"),
        "so the mark of the card stands on the nearest row kept, which is the last row left"
    );
}

#[test]
fn a_press_marks_the_row_it_landed_on_and_a_press_past_the_last_marks_nothing() {
    for mut list in [files(), served(), card()] {
        let name = list.name();
        assert!(
            list.pressed(1),
            "the second row of the {name} list is a row a press lands on"
        );
        assert_eq!(
            list.mark(),
            1,
            "and the mark of the {name} list stands where the press landed"
        );

        let rows = list.rows().len();
        assert!(
            !list.pressed(rows),
            "a press under the last row of the {name} list lands on no row"
        );
        assert_eq!(
            list.mark(),
            1,
            "so the mark of the {name} list stands where it stood"
        );
    }
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
