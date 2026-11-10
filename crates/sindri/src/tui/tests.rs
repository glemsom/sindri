//! The launch's own two contracts, each driving one key's outcome through a pseudo-terminal of
//! this process's own: a copy written down the wire, and a settings visit whose value is saved.

use std::os::fd::AsFd;

use clap::Parser;
use sindri::composer::Key;
use sindri::rows::Scroll;
use sindri_core::records::Entry;
use sindri_core::redact::SecretSet;

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
    let mut telling = Teller::new(
        pty.slave
            .try_clone()
            .expect("a second handle on the terminal"),
        true,
        std::path::Path::new("/work/sindri"),
    );
    let mut running = None;
    let mut asking = None;
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
    let mut telling = Teller::new(
        pty.slave
            .try_clone()
            .expect("a second handle on the terminal"),
        true,
        std::path::Path::new("/work/sindri"),
    );
    let mut running = None;
    let mut asking = None;

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
