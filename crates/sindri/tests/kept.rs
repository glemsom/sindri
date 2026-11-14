//! What a screen keeps: the list a cursor movement walks, the query a typed glyph reaches and the row
//! a press chooses, each answered by the description of the screen rather than by matching on it.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use sindri::composer::{Key, Outcome};
use sindri::kept::List;
use sindri::keys::{Body, Mode, Stack};
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
fn screen(mode: Mode) -> (tempfile::TempDir, View) {
    let root = workspace(&["src/rows.rs", "src/view.rs"]);
    let mut view = View::boot();
    view.set_workspace(root.path().to_path_buf());
    view.open_settings(&Config::default());
    view.key(Key::Escape);
    match mode {
        Mode::Files => view.key(Key::Char('@')),
        Mode::Models => {
            view.paste("/model");
            view.key(Key::Enter)
        }
        Mode::Settings => {
            view.open_settings(&Config::default());
            Outcome::Edited
        }
        other => panic!("{other:?} keeps no list, so no screen of it is opened here"),
    };
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

/// Where each list's mark stands: the picker's row, the model screen's and the settings visit's.
fn marks(view: &View) -> (usize, usize, usize) {
    (
        view.picker().selected(),
        view.models().selected(),
        view.settings().current(),
    )
}

#[test]
fn a_screen_names_the_list_a_movement_typing_and_a_press_reach() {
    let named: [(Mode, Option<List>); 6] = [
        (Mode::Composer, None),
        (Mode::Keys, None),
        (Mode::Files, Some(List::Files)),
        (Mode::Models, Some(List::Models)),
        (Mode::Settings, Some(List::Settings)),
        (Mode::Stats, None),
    ];

    for mode in Mode::ALL {
        let kept = named
            .iter()
            .find(|(screen, _)| *screen == mode)
            .map(|(_, list)| *list)
            .unwrap_or_else(|| {
                panic!(
                    "{mode:?} is a mode, so a test naming the lists a frame keeps has to say \
                     which one it names"
                )
            });
        assert_eq!(
            mode.body().and_then(Body::kept),
            kept,
            "the description of {mode:?} names the list its screen keeps: the draft keeps none \
             because its rows are its own card, the keys screen's rows are keys nothing moves \
             through, and the ledger's are facts laid whole"
        );
    }
}

#[test]
fn one_act_types_into_the_list_a_screen_keeps() {
    let mut typed = Vec::new();
    for mode in Mode::ALL {
        if mode.body().and_then(Body::kept).is_none() {
            continue;
        }
        let mut stack = Stack::new();
        stack.open(mode);
        let erase = stack
            .act(Key::Backspace)
            .unwrap_or_else(|| panic!("{} binds no backspace on its list", mode.name()));
        let glyph = stack
            .act(Key::Char('x'))
            .unwrap_or_else(|| panic!("a glyph is no act at the {} screen", mode.name()));
        typed.push((mode, erase, glyph));
    }

    assert_eq!(
        typed.len(),
        3,
        "three screens keep a list, so three of them answer a typed glyph: {:?}",
        typed
            .iter()
            .map(|(mode, ..)| mode.name())
            .collect::<Vec<_>>()
    );
    for (mode, erase, glyph) in &typed {
        assert_eq!(
            erase,
            glyph,
            "the glyph that erases and the glyph that types are one act at the {} screen, because \
             one box is what receives both",
            mode.name()
        );
        assert_eq!(
            *glyph,
            typed[0].2,
            "and it is the same act at every screen that keeps a list: the {} screen types what \
             the {} screen types, so a screen that gains a list has no act of its own to remember",
            mode.name(),
            typed[0].0.name()
        );
    }
}

#[test]
fn a_movement_walks_the_list_the_screen_keeps() {
    for mode in [Mode::Files, Mode::Models, Mode::Settings] {
        let (_root, mut view) = screen(mode);
        assert_eq!(
            marks(&view),
            (0, 0, 0),
            "every list opens on its first row, so a mark that moved is a mark a key moved"
        );

        assert_eq!(
            view.key(Key::Down),
            Outcome::Edited,
            "`↓` is a mark moving at the {} screen, which is a frame to repaint",
            mode.name()
        );

        let walked = marks(&view);
        let reached = match mode {
            Mode::Files => walked == (1, 0, 0),
            Mode::Models => walked == (0, 1, 0),
            Mode::Settings => walked == (0, 0, 1),
            other => panic!("{other:?} keeps no list to walk"),
        };
        assert!(
            reached,
            "`↓` moves the mark of the list the {} screen keeps and leaves the other two where \
             they were: {walked:?}",
            mode.name()
        );
    }
}

/// The value the settings visit holds on the row it is on.
fn value(view: &View) -> String {
    view.settings()
        .rows()
        .nth(view.settings().current())
        .map(|(_, text)| text.to_owned())
        .expect("a visit opened on a config with keys in it stands on one of them")
}

#[test]
fn a_glyph_lands_in_the_list_the_screen_keeps() {
    for mode in [Mode::Files, Mode::Models, Mode::Settings] {
        let (_root, mut view) = screen(mode);

        assert_eq!(
            view.key(Key::Char('x')),
            Outcome::Edited,
            "a glyph typed at the {} screen changes a list, which is a frame to repaint",
            mode.name()
        );

        let typed = (
            view.picker().query().to_owned(),
            view.models().query().to_owned(),
            value(&view),
        );
        let landed = match mode {
            Mode::Files => typed == (String::from("x"), String::new(), String::new()),
            Mode::Models => typed == (String::new(), String::from("x"), String::new()),
            Mode::Settings => typed == (String::new(), String::new(), String::from("x")),
            other => panic!("{other:?} keeps no list to type into"),
        };
        assert!(
            landed,
            "the glyph lands in the list the {} screen keeps and in no other: {typed:?}",
            mode.name()
        );
    }
}

#[test]
fn a_press_on_a_row_of_the_visit_is_no_act() {
    let (_root, mut view) = screen(Mode::Settings);
    view.set_pointer(true);
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
