//! The launch's own contracts, each driving one event through a pseudo-terminal of this process's
//! own: a copy written down the wire, a settings visit whose value is saved or refused, a run that
//! cannot start leaving its row, a focus report, and a key typed before the first frame.

use std::os::fd::AsFd;
use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;

use sindri::composer::{Composer, Key, Outcome, Prompt};
use sindri::ink::Look;
use sindri::ledger::Boot;
use sindri::palette::Palette;
use sindri::pictures::PLACEHOLDER;
use sindri_core::first_run::FirstRun;
use sindri_core::policy::Enforcement;
use sindri_core::records::Entry;
use sindri_core::redact::SecretSet;
use sindri_core::run::Stop;
use sindri_core::wire::Wire;

use crate::term::Terminal;
use crate::term::pty::Pty;

use super::*;

#[test]
fn a_copy_leaves_the_terminal_by_osc_52_and_asks_no_clipboard_utility() {
    let stage = Stage::open();
    let view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::of(vec![Entry::Assistant {
            turn: 1,
            text: SecretSet::default().redact("read the transcript"),
        }]),
    );

    // No mark, so `Ctrl-Y` copies the last answer: the words go down the wire the frame does,
    // which is what makes a copy work over SSH.
    let environment = std::collections::BTreeMap::new();
    let mut live = stage.launch(view, Config::default(), &environment);

    let ended = live
        .answer(Event::Key(Key::Copy))
        .expect("the copy is written");

    assert!(ended.is_none(), "a copy is not the key that quits");
    let drawn = stage.drawn();
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

#[test]
fn the_harness_resolves_the_config_a_launch_runs_on() {
    let (_home, mut environment) =
        config_home("provider = \"anthropic\"\nmodel = \"claude-sonnet-4-5\"\n");
    environment.insert(
        String::from("ANTHROPIC_API_KEY"),
        String::from("sk-ant-the-wires-own"),
    );
    let stage = Stage::open();

    assert_eq!(
        stage.config(&environment).api_key.as_deref(),
        Some("sk-ant-the-wires-own"),
        "a screen test resolves through the door a launch uses, so it cannot pass on a config \
         production never builds"
    );
}

/// A pseudo-terminal of this process's own, with the launch standing on it: the keyboard and the
/// screen it was handed, and the reader that says what the screen has been given.
struct Stage {
    pty: Pty,
    terminal: Terminal,
    cli: Cli,
}

impl Stage {
    fn open() -> Stage {
        let pty = Pty::open();
        let terminal =
            Terminal::pair(pty.slave.as_fd(), pty.slave.as_fd()).expect("a terminal to write to");
        Stage {
            pty,
            terminal,
            cli: Cli::parse_from(["sindri"]),
        }
    }

    /// The config the launch resolves from `environment`, resolved as `main` resolves one.
    fn config(&self, environment: &impl Environment) -> Config {
        FirstRun::resolve(environment, self.cli.flags())
            .expect("a config")
            .config()
            .clone()
    }

    /// The launch this terminal draws, with `view` as the frame it draws and `config` as the run a
    /// prompt will ask for.
    fn launch<'a>(
        &'a self,
        view: View,
        config: Config,
        environment: &'a dyn Environment,
    ) -> Loop<'a> {
        self.told_in(view, config, environment, false)
    }

    /// The same launch where the stream carries a window, which is the only thing that reports the
    /// reader's focus and raises a notification.
    fn launch_in_a_window<'a>(
        &'a self,
        view: View,
        config: Config,
        environment: &'a dyn Environment,
    ) -> Loop<'a> {
        self.told_in(view, config, environment, true)
    }

    /// Every byte this terminal has been handed, and nothing it says next.
    fn drawn(&self) -> String {
        self.pty.drawn()
    }

    fn told_in<'a>(
        &'a self,
        view: View,
        config: Config,
        environment: &'a dyn Environment,
        carries: bool,
    ) -> Loop<'a> {
        let telling = Window::new(
            self.pty
                .slave
                .try_clone()
                .expect("a second handle on the terminal"),
            config.tells_status(),
            std::path::Path::new("/work/sindri"),
            carries,
        );
        Loop::new(
            &self.terminal,
            &self.cli,
            config,
            environment,
            view,
            telling,
            Arc::new(Wake::open().expect("a wake")),
        )
        .expect("a launch")
    }
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
    let stage = Stage::open();
    let config = stage.config(&environment);
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::none(),
    );
    view.open_settings(&config);

    walk_to(&mut view, "model");
    view.key(Key::Clear);
    for glyph in "new-model".chars() {
        view.key(Key::Char(glyph));
    }
    let mut live = stage.launch(view, config, &environment);

    let ended = live
        .answer(Event::Key(Key::Enter))
        .expect("the write is made");

    assert!(ended.is_none(), "a write is not the key that quits");
    assert_eq!(
        std::fs::read_to_string(&path).expect("the file the write landed in"),
        "# mine\nmodel = \"new-model\"\n",
        "Enter writes at once, and only the row the reader touched: the comment is the \
         byte it was"
    );
    assert_eq!(
        live.mode(),
        sindri::keys::Mode::Composer,
        "and the visit is over, so the draft has the keys again"
    );
    assert_eq!(
        live.config().model.as_deref(),
        Some("new-model"),
        "the launch's config is resolved again from what the visit wrote, so the next \
         prompt — the run `send` starts from this very value — runs on it"
    );
}

#[test]
fn a_settings_value_the_schema_refuses_names_its_row_and_writes_nothing() {
    let (home, environment) = config_home("model = \"old-model\"\n");
    let path = home.path().join("sindri").join("config.toml");
    let stage = Stage::open();
    let config = stage.config(&environment);
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::none(),
    );
    view.open_settings(&config);

    walk_to(&mut view, "context_window");
    view.key(Key::Char('0'));
    let mut live = stage.launch(view, config, &environment);

    let ended = live
        .answer(Event::Key(Key::Enter))
        .expect("the refusal is drawn");

    assert!(
        ended.is_none(),
        "a value the schema refuses is a row on the frame, and not the end of the launch"
    );
    let refusal = live
        .refusal()
        .unwrap_or_else(|| panic!("the settings screen says why the value will not do"));
    assert!(
        refusal.contains("context_window"),
        "the refusal names the row a reader has to go and fix, and it says: {refusal}"
    );
    assert_eq!(
        std::fs::read_to_string(&path).expect("the file the write landed in"),
        "model = \"old-model\"\n",
        "and nothing is written at all"
    );
}

const FRAME: (u16, u16) = (120, 40);

/// The frame a launch opens on, with a rail of facts and a terminal that places pictures: the rail
/// the face is drawn over, beside the band no row of it is taken from.
fn railed(config: &Config) -> View {
    let boot = Boot {
        wire: Some(Wire::Anthropic),
        model: Some(String::from("claude-sonnet-4-6")),
        workspace: PathBuf::from("/work/sindri"),
        branch: None,
        enforcement: Enforcement::Bubblewrap,
    };
    View::of(
        crate::caps::pictured().faced(config.draws_face()),
        Facts::boot(boot, None),
        Transcript::none(),
    )
}

fn painted(view: &mut View) -> ratatui::buffer::Buffer {
    let (cols, rows) = FRAME;
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(cols, rows))
        .expect("a headless screen of that size");
    terminal
        .draw(|frame| {
            view.paint(frame.area(), frame.buffer_mut());
        })
        .expect("the frame is painted")
        .buffer
        .clone()
}

/// Whether this frame paints the face, read off the cells a placement covers.
fn draws_the_face(view: &mut View) -> bool {
    let frame = painted(view);
    let area = frame.area;
    (area.y..area.bottom()).any(|y| {
        (area.x..area.right()).any(|x| {
            frame
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
    let stage = Stage::open();
    let config = stage.config(&environment);
    let mut view = railed(&config);
    assert!(
        draws_the_face(&mut view),
        "the frame the visit is opened from draws the face the file speaks for"
    );

    view.open_settings(&config);
    walk_to(&mut view, "face");
    view.key(Key::Char(' '));
    let mut live = stage.launch(view, config, &environment);

    let ended = live
        .answer(Event::Key(Key::Enter))
        .expect("the write is made");

    assert!(ended.is_none(), "a write is not the key that quits");
    assert_eq!(
        std::fs::read_to_string(&path).expect("the file the write landed in"),
        "face = false\n",
        "the row the reader flipped is the only line the write touches"
    );
    assert_eq!(
        live.mode(),
        sindri::keys::Mode::Composer,
        "and the visit is over, so the draft has the keys again"
    );
    assert!(
        !live.draws_the_face(),
        "the frame the visit leaves reserves no band for a face it will not draw"
    );
}

/// The face a visit turns off is the one picture the launch's first frame has already placed over
/// the rail's top rows: a value handed again is the frame's own, so no placement is made into pixels
/// a second time.
#[test]
fn a_settings_visit_that_turns_the_face_off_makes_no_picture_into_pixels_again() {
    let (_home, environment) = config_home("face = true\n");
    let stage = Stage::open();
    let config = stage.config(&environment);
    let mut view = railed(&config);
    view.open_settings(&config);
    walk_to(&mut view, "face");
    view.key(Key::Char(' '));
    let mut live = stage.launch(view, config, &environment);

    assert_eq!(
        live.rasterises("face.png"),
        0,
        "no frame has drawn the face yet, so none was made into pixels"
    );
    assert!(live.draws_the_face(), "the launch's first frame draws it");

    let ended = live
        .answer(Event::Key(Key::Enter))
        .expect("the write is made");

    assert!(ended.is_none(), "a write is not the key that quits");
    assert!(!live.draws_the_face(), "the face is turned off");
    assert_eq!(
        live.rasterises("face.png"),
        1,
        "and the rail's one face is made into pixels once: nothing was transmitted twice"
    );
}

fn cells_wearing(view: &mut View, palette: &Palette, look: Look) -> Vec<(u16, u16)> {
    let frame = painted(view);
    let area = frame.area;
    (area.y..area.bottom())
        .flat_map(|y| (area.x..area.right()).map(move |x| (x, y)))
        .filter(|at| {
            frame
                .cell(*at)
                .and_then(|cell| Look::of(cell, palette))
                .is_some_and(|worn| worn == look)
        })
        .collect()
}

/// What a test asks of a launch: the frame it holds, the config the run will ask by, and the words it
/// has told. A test never reads a field of it.
impl Loop<'_> {
    fn mode(&self) -> sindri::keys::Mode {
        self.view.mode()
    }

    fn config(&self) -> &Config {
        &self.config
    }

    fn draft(&self) -> &str {
        self.view.composer().text()
    }

    fn queued(&self) -> usize {
        self.view.queue().len()
    }

    fn dequeue(&mut self) -> Option<Prompt> {
        self.view.dequeue()
    }

    fn refusal(&self) -> Option<&str> {
        self.view.settings().refusal()
    }

    fn draws_the_face(&mut self) -> bool {
        draws_the_face(&mut self.view)
    }

    /// How many times this frame has made `source` into pixels: a picture placed twice is a
    /// transmission the reader waited for twice.
    fn rasterises(&self, source: &str) -> usize {
        self.view.capabilities().pictures().rasterises(source)
    }

    fn cells(&mut self, palette: &Palette, look: Look) -> Vec<(u16, u16)> {
        cells_wearing(&mut self.view, palette, look)
    }
}

fn sequences(written: &str) -> Vec<&str> {
    written
        .split("\x1b]")
        .filter_map(|rest| rest.split_once("\x1b\\").map(|(payload, _)| payload))
        .collect()
}

/// A prompt as the composer hands one back: typed, then sent.
fn prompt_of(text: &str) -> Prompt {
    let mut composer = Composer::new();
    composer.paste(text);
    match composer.key(Key::Enter) {
        Outcome::Send(prompt) => prompt,
        other => panic!("a draft should have been sent, and {other:?} came instead"),
    }
}

#[test]
fn a_run_that_cannot_start_leaves_its_row_and_the_draft_and_never_ends_the_launch() {
    let stage = Stage::open();
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::none(),
    );
    view.paste("why does the retry test fail");
    let environment = std::collections::BTreeMap::new();
    let mut live = stage.launch(view, Config::default(), &environment);

    let ended = live
        .answer(Event::Key(Key::Enter))
        .expect("the refusal is drawn");

    assert!(
        ended.is_none(),
        "a run that could not start is a row in the frame, and not the end of the launch"
    );
    let drawn = stage.drawn();
    let refusal = drawn
        .find("a run needs a provider and a model")
        .unwrap_or_else(|| panic!("the frame carries the refusal's own words: {drawn:?}"));
    let recalled = drawn
        .rfind("why does the retry test fail")
        .unwrap_or_else(|| panic!("the draft that asked is the reader's again: {drawn:?}"));
    assert!(
        refusal < recalled,
        "the draft stands below the refusal it asked with, in the composer it came from: {drawn:?}"
    );
}

#[test]
fn a_pass_with_no_run_in_flight_lands_nothing_and_ends_nothing() {
    let stage = Stage::open();
    let environment = std::collections::BTreeMap::new();
    let mut live = stage.launch(
        View::of(
            Capabilities::default(),
            Facts::default().asked(),
            Transcript::none(),
        ),
        Config::default(),
        &environment,
    );

    let ended = live
        .take()
        .expect("a pass with nothing to land is no failure");

    assert!(
        !ended,
        "a pass with no run in flight ends no run and resumes"
    );
}

#[test]
fn a_refused_prompt_out_of_the_queue_is_the_draft_again_and_the_launch_stands() {
    let stage = Stage::open();
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::none(),
    );
    view.enqueue(prompt_of("run the test that failed and read it"));
    view.enqueue(prompt_of("then the fmt check"));
    // No provider and no model: `Launch::open` refuses before it touches a sandbox, a session or a
    // wire.
    let environment = std::collections::BTreeMap::new();
    let mut live = stage.launch(view, Config::default(), &environment);

    live.resume().expect("the refusal is drawn");

    assert_eq!(
        live.queued(),
        1,
        "the prompt behind the one that could not run waits there still"
    );
    assert_eq!(
        live.draft(),
        "run the test that failed and read it",
        "and the words it was asked with are the reader's draft again, to send or to change"
    );
    let drawn = stage.drawn();
    assert!(
        drawn.contains("a run needs a provider and a model"),
        "the frame carries the refusal's own words: {drawn:?}"
    );
}

#[test]
fn a_refused_prompt_waits_in_the_queue_again_while_the_reader_has_a_draft_of_their_own() {
    let stage = Stage::open();
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::none(),
    );
    view.enqueue(prompt_of("run the test that failed and read it"));
    view.enqueue(prompt_of("then the fmt check"));
    view.paste("and then the release check");
    // No provider and no model: `Launch::open` refuses before it touches a sandbox, a session or a
    // wire.
    let environment = std::collections::BTreeMap::new();
    let mut live = stage.launch(view, Config::default(), &environment);

    live.resume().expect("the refusal is drawn");

    assert_eq!(
        live.draft(),
        "and then the release check",
        "a draft the reader has typed since is theirs, and no refusal takes it"
    );
    assert_eq!(
        live.queued(),
        2,
        "so the prompt that could not run waits in the queue again rather than nowhere"
    );
    assert_eq!(
        live.dequeue().as_ref().map(Prompt::text),
        Some("then the fmt check"),
        "behind the prompt it was queued behind"
    );
    assert_eq!(
        live.dequeue().as_ref().map(Prompt::text),
        Some("run the test that failed and read it"),
        "and ahead of nothing it has lost"
    );
    let drawn = stage.drawn();
    assert!(
        drawn.contains("a run needs a provider and a model"),
        "the frame carries the refusal's own words: {drawn:?}"
    );
}

#[test]
fn a_quit_typed_before_the_first_frame_is_answered_by_the_loop() {
    let stage = Stage::open();
    let environment = std::collections::BTreeMap::new();
    let signals = Signals::install().expect("a signal pipe");
    let mut live = stage.launch(
        View::of(
            Capabilities::default(),
            Facts::default().asked(),
            Transcript::none(),
        ),
        Config::default(),
        &environment,
    );

    let ended = live.keys(&signals, b"\x11").expect("the quit is answered");

    assert!(
        matches!(ended, Ending::Quit),
        "a quit typed before the first frame is the ending the loop hands back"
    );
}

#[test]
fn a_focus_report_recedes_the_frame_and_the_stop_nobody_watched_raises_the_notification() {
    let stage = Stage::open();
    let environment = std::collections::BTreeMap::new();
    let palette = Palette::TRUECOLOR;
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::none(),
    );
    view.paste("the draft the composer band is drawn from");
    // A stream that carries a window: nothing else raises a notification at all.
    let mut live = stage.launch_in_a_window(view, Config::default(), &environment);
    let lit = live.cells(&palette, palette.accent_edge());
    assert!(
        !lit.is_empty(),
        "the surface that takes the keys wears the accent edge while the reader is read: {lit:?}"
    );

    let ended = live
        .answer(Event::Focus(false))
        .expect("the report is answered");

    assert!(ended.is_none(), "a focus report never ends the launch");
    let away = live.cells(&palette, palette.dim_edge());
    assert!(
        lit.iter().all(|at| away.contains(at)),
        "every cell that wore the accent recedes to the dim edge while nobody is looking: {away:?}"
    );
    live.told(Told::Stopped {
        turn: 1,
        stop: Stop::Final,
    })
    .expect("the run's ending is told");
    let drawn = stage.drawn();
    let said = sequences(&drawn);
    assert!(
        said.iter().any(|payload| payload.starts_with("9;sindri: ")),
        "a run that ends while the report says nobody is reading raises the notification: {said:?}"
    );
    live.answer(Event::Focus(true)).expect("the reader is back");
    assert_eq!(
        live.cells(&palette, palette.accent_edge()),
        lit,
        "and the edge returns to the cells it left once the terminal says the reader is back"
    );
}
