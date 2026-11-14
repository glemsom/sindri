//! What a screen keeps: the list a cursor movement walks, the query a typed glyph reaches and the row
//! a press chooses, each answered by the description of the screen rather than by matching on it.

mod caps; // the terminals these frames are drawn on

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use sindri::capability::Capabilities;
use sindri::composer::{Key, Outcome};
use sindri::keys::{Act, Mode, Stack};
use sindri::mark::Cell;
use sindri::pointer::Pointer;
use sindri::view::View;

use sindri_core::config::Config;
use sindri_core::provider::served::{Listing, Where};
use sindri_core::provider::transport::{Canned, Replay};
use sindri_core::provider::{Client, Credential};
use sindri_core::redact::SecretSet;
use sindri_core::wire::Wire;

const ANTHROPIC_MODELS: &str =
    include_str!("../../sindri-core/tests/fixtures/providers/anthropic_models.json");

/// The listing the model screen is handed: names a wire really served, read through the listing code.
fn listed() -> Listing {
    let at = Where::own(Wire::Anthropic);
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
    let mut view = View::boot(capabilities);
    view.set_workspace(root.path().to_path_buf());
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
    view.set_served(listed());
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
