//! The launch's own two contracts, each driving one key's outcome through a pseudo-terminal of
//! this process's own: a copy written down the wire, and a settings visit whose value is saved.

use std::os::fd::AsFd;
use std::path::PathBuf;

use clap::Parser;
use sindri::composer::Key;
use sindri::graphics::CellSize;
use sindri::ledger::Boot;
use sindri::pictures::{PLACEHOLDER, Pictures};
use sindri::rows::Scroll;
use sindri_core::policy::Enforcement;
use sindri_core::records::Entry;
use sindri_core::redact::SecretSet;
use sindri_core::wire::Wire;

use crate::term::pty::Pty;
use crate::term::{Terminal, canvas_at, painted_at};

use super::*;

#[test]
fn a_copy_leaves_the_terminal_by_osc_52_and_asks_no_clipboard_utility() {
    let mut pty = Pty::open();
    let terminal =
        Terminal::pair(pty.slave.as_fd(), pty.slave.as_fd()).expect("a terminal to write to");
    let mut canvas = canvas_at(&terminal, painted_at()).expect("a canvas over it");
    let mut view = View::of(
        vec![Entry::Assistant {
            turn: 1,
            text: SecretSet::default().redact("read the transcript"),
        }],
        Scroll::Tail,
    );

    // No mark, so `Ctrl-Y` copies the last answer: the words go down the wire the frame does,
    // which is what makes a copy work over SSH.
    let cli = Cli::parse_from(["sindri"]);
    let mut config = sindri_core::config::Config::default();
    let environment = std::collections::BTreeMap::new();
    let wake = Arc::new(Wake::open().expect("a wake"));
    let mut telling = Window::new(
        pty.slave
            .try_clone()
            .expect("a second handle on the terminal"),
        true,
        std::path::Path::new("/work/sindri"),
        false,
    );
    let mut running = None;
    let mut asking = None;
    let mut held = None;
    let ended = answer(
        &terminal,
        &mut canvas,
        &mut view,
        Event::Key(Key::Copy),
        &mut running,
        &mut asking,
        &cli,
        &mut config,
        &environment,
        &wake,
        &mut telling,
        &mut held,
    )
    .expect("the copy is written");

    assert!(ended.is_none(), "a copy is not the key that quits");
    let drawn = pty.drawn();
    let sequence = crate::osc::copy("read the transcript");
    assert!(
        drawn.contains(std::str::from_utf8(&sequence).expect("a printable sequence")),
        "the words left as OSC 52 and as nothing else: {drawn:?}"
    );
}

fn config_home(
    text: &str,
) -> (
    tempfile::TempDir,
    std::collections::BTreeMap<String, String>,
) {
    let home = tempfile::tempdir().expect("a temp dir");
    let directory = home.path().join("sindri");
    std::fs::create_dir_all(&directory).expect("the sindri directory");
    std::fs::write(directory.join("config.toml"), text).expect("the config file");
    let environment = [(
        String::from("XDG_CONFIG_HOME"),
        home.path().display().to_string(),
    )]
    .into_iter()
    .collect();
    (home, environment)
}

fn walk_to(view: &mut View, key: &str) {
    for _ in 0..sindri_core::config::Config::KEYS.len() {
        let row = view
            .settings()
            .rows()
            .nth(view.settings().current())
            .map(|(name, _)| name);
        if row == Some(key) {
            return;
        }
        view.key(Key::Down);
    }
    panic!("{key} is a key of the schema, so the screen has a row for it");
}

#[test]
fn enter_at_the_settings_modal_writes_the_file_and_the_next_prompt_runs_on_it() {
    let (home, environment) = config_home("# mine\nmodel = \"old-model\"\n");
    let path = home.path().join("sindri").join("config.toml");
    let cli = Cli::parse_from(["sindri"]);
    let mut config = Config::resolve(&environment, cli.flags()).expect("a config");
    let pty = Pty::open();
    let terminal =
        Terminal::pair(pty.slave.as_fd(), pty.slave.as_fd()).expect("a terminal to write to");
    let mut canvas = canvas_at(&terminal, painted_at()).expect("a canvas over it");
    let mut view = View::boot();
    view.open_settings(&config);
    let wake = Arc::new(Wake::open().expect("a wake"));
    let mut telling = Window::new(
        pty.slave
            .try_clone()
            .expect("a second handle on the terminal"),
        true,
        std::path::Path::new("/work/sindri"),
        false,
    );
    let mut running = None;
    let mut asking = None;
    let mut held = None;

    walk_to(&mut view, "model");
    view.key(Key::Clear);
    for glyph in "new-model".chars() {
        view.key(Key::Char(glyph));
    }
    let ended = answer(
        &terminal,
        &mut canvas,
        &mut view,
        Event::Key(Key::Enter),
        &mut running,
        &mut asking,
        &cli,
        &mut config,
        &environment,
        &wake,
        &mut telling,
        &mut held,
    )
    .expect("the write is made");

    assert!(ended.is_none(), "a write is not the key that quits");
    assert_eq!(
        std::fs::read_to_string(&path).expect("the file the write landed in"),
        "# mine\nmodel = \"new-model\"\n",
        "Enter writes at once, and only the row the reader touched: the comment is the \
         byte it was"
    );
    assert_eq!(
        view.mode(),
        sindri::keys::Mode::Composer,
        "and the visit is over, so the draft has the keys again"
    );
    assert_eq!(
        config.model.as_deref(),
        Some("new-model"),
        "the launch's config is resolved again from what the visit wrote, so the next \
         prompt — the run `send` starts from this very value — runs on it"
    );
}

const FRAME: (u16, u16) = (120, 40);

fn cell() -> CellSize {
    CellSize::new(10, 20)
}

/// The frame a launch opens on, with a rail of facts and a terminal that places pictures: the two
/// things the face is drawn over and in.
fn railed(config: &Config) -> View {
    let mut view = opened(config);
    view.set_cell_size(cell());
    view.set_pictures(Pictures::kitty(cell()));
    view.door(Boot {
        wire: Some(Wire::Anthropic),
        model: Some(String::from("claude-sonnet-4-6")),
        workspace: PathBuf::from("/work/sindri"),
        branch: None,
        enforcement: Enforcement::Bubblewrap,
    });
    view
}

/// Whether this frame paints the face, read off the cells a placement covers.
fn draws_the_face(view: &mut View) -> bool {
    let (cols, rows) = FRAME;
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(cols, rows))
        .expect("a headless screen of that size");
    let painted = terminal
        .draw(|frame| view.paint(frame.area(), frame.buffer_mut()))
        .expect("the frame is painted")
        .buffer
        .clone();
    let area = painted.area;
    (area.y..area.bottom()).any(|y| {
        (area.x..area.right()).any(|x| {
            painted
                .cell((x, y))
                .is_some_and(|cell| cell.symbol().contains(PLACEHOLDER))
        })
    })
}

#[test]
fn a_launch_opens_on_the_face_unless_its_config_turns_it_off() {
    for (face, drawn) in [(None, true), (Some(true), true), (Some(false), false)] {
        let config = Config {
            face,
            ..Config::default()
        };

        assert_eq!(
            draws_the_face(&mut railed(&config)),
            drawn,
            "a config holding face = {face:?} is drawn {drawn} on the frame a launch opens on"
        );
    }
}

#[test]
fn a_settings_visit_that_turns_the_face_off_repaints_the_frame_it_leaves_without_one() {
    let (home, environment) = config_home("face = true\n");
    let path = home.path().join("sindri").join("config.toml");
    let cli = Cli::parse_from(["sindri"]);
    let mut config = Config::resolve(&environment, cli.flags()).expect("a config");
    let mut view = railed(&config);
    assert!(
        draws_the_face(&mut view),
        "the frame the visit is opened from draws the face the file speaks for"
    );
    let pty = Pty::open();
    let terminal =
        Terminal::pair(pty.slave.as_fd(), pty.slave.as_fd()).expect("a terminal to write to");
    let mut canvas = canvas_at(&terminal, painted_at()).expect("a canvas over it");
    let wake = Arc::new(Wake::open().expect("a wake"));
    let mut telling = Window::new(
        pty.slave
            .try_clone()
            .expect("a second handle on the terminal"),
        true,
        std::path::Path::new("/work/sindri"),
        false,
    );
    let mut running = None;
    let mut asking = None;
    let mut held = None;

    view.open_settings(&config);
    walk_to(&mut view, "face");
    view.key(Key::Char(' '));
    let ended = answer(
        &terminal,
        &mut canvas,
        &mut view,
        Event::Key(Key::Enter),
        &mut running,
        &mut asking,
        &cli,
        &mut config,
        &environment,
        &wake,
        &mut telling,
        &mut held,
    )
    .expect("the write is made");

    assert!(ended.is_none(), "a write is not the key that quits");
    assert_eq!(
        std::fs::read_to_string(&path).expect("the file the write landed in"),
        "face = false\n",
        "the row the reader flipped is the only line the write touches"
    );
    assert_eq!(
        view.mode(),
        sindri::keys::Mode::Composer,
        "and the visit is over, so the draft has the keys again"
    );
    assert!(
        !draws_the_face(&mut view),
        "the frame the visit leaves reserves no band for a face it will not draw"
    );
}
