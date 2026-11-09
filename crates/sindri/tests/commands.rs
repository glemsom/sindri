//! The `/` commands from the outside: what a line names, and what each does to the frame — a screen opened, a
//! history cleared, a set emptied, a name handed on, the launch ended. Every test types the line a reader types
//! into a frame built the way the terminal builds one; the wire and the save behind `/model` are proved elsewhere.

use std::path::Path;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;

use sindri::commands::{self, Command};
use sindri::composer::{Key, Outcome};
use sindri::keys::Mode;
use sindri::view::View;
use sindri_core::config::Config;
use sindri_core::records::Entry;
use sindri_core::redact::SecretSet;

const SCREEN: (u16, u16) = (80, 24);

fn say(view: &mut View, line: &str) -> Outcome {
    view.paste(line);
    view.key(Key::Enter)
}

fn frame(workspace: &Path) -> View {
    let mut view = View::boot();
    view.set_workspace(workspace.to_path_buf());
    view
}

fn workspace_with(name: &str, frontmatter: &str) -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("a temp workspace");
    let directory = root.path().join(".agents/skills").join(name);
    std::fs::create_dir_all(&directory).expect("the skill's directory");
    std::fs::write(
        directory.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: the {name} skill\n{frontmatter}---\n\nBody.\n"),
    )
    .expect("a SKILL.md");
    root
}

fn painted(view: &mut View) -> String {
    let (cols, rows) = SCREEN;
    let mut terminal =
        Terminal::new(TestBackend::new(cols, rows)).expect("a headless screen of that size");
    let drawn = terminal
        .draw(|frame| view.paint(frame.area(), frame.buffer_mut()))
        .expect("the frame is painted")
        .buffer
        .clone();
    rows_of(&drawn)
}

fn rows_of(buffer: &Buffer) -> String {
    let area = buffer.area;
    let mut lines: Vec<String> = Vec::new();
    for y in area.y..area.bottom() {
        let mut line = String::new();
        for x in area.x..area.right() {
            let cell = buffer
                .cell((x, y))
                .unwrap_or_else(|| panic!("row {y}, column {x} is outside the frame it is in"));
            line.push_str(cell.symbol());
        }
        lines.push(line);
    }
    lines.join("\n")
}

#[test]
fn every_command_in_the_list_reads_as_the_command_it_names() {
    assert_eq!(commands::read("/new"), Some(Command::New));
    assert_eq!(commands::read("/model"), Some(Command::Model));
    assert_eq!(commands::read("/settings"), Some(Command::Settings));
    assert_eq!(commands::read("/help"), Some(Command::Help));
    assert_eq!(commands::read("/quit"), Some(Command::Quit));
    assert_eq!(
        commands::read("/quit "),
        Some(Command::Quit),
        "the whitespace around a line is not part of the command it names"
    );
}

#[test]
fn the_two_skill_spellings_are_one_line() {
    assert_eq!(
        commands::read("/skill:commit-message"),
        commands::read("/skill commit-message"),
        "§5 says `/skill:name` and `/skill name` are the same line, so they are one value \
         here and neither is a second place a load is decided"
    );
    assert_eq!(
        commands::read("/skill:commit-message"),
        Some(Command::Skill(String::from("commit-message")))
    );
}

#[test]
fn a_line_that_names_no_command_is_a_prompt_like_any_other() {
    for line in ["/skillet", "/skill", "/skill:", "/", "and/or"] {
        assert_eq!(
            commands::read(line),
            None,
            "{line} names no command, so the model is asked it rather than the frame \
             swallowing the reader's words"
        );
    }
}

#[test]
fn a_model_named_by_hand_is_the_rest_of_the_line() {
    assert_eq!(
        commands::read("/model some-name-the-wire-never-served"),
        Some(Command::ModelNamed(String::from(
            "some-name-the-wire-never-served"
        ))),
        "a name the wire never offered is still a name, and §11's key takes it whole"
    );
}

#[test]
fn quit_ends_the_launch() {
    let root = workspace_with("commit-message", "");
    let mut view = frame(root.path());

    assert_eq!(
        say(&mut view, "/quit"),
        Outcome::Quit,
        "`/quit` is `Ctrl-Q` as a line rather than a key"
    );
}

#[test]
fn help_opens_the_keys_in_force() {
    let root = workspace_with("commit-message", "");
    let mut view = frame(root.path());

    assert_eq!(say(&mut view, "/help"), Outcome::Edited);
    assert_eq!(
        view.mode(),
        Mode::Keys,
        "the screen that lists the keys in force is the help there is, and `/help` opens it"
    );
}

#[test]
fn settings_opens_the_screen_the_modal_fills_in() {
    let root = workspace_with("commit-message", "");
    let mut view = frame(root.path());

    assert_eq!(
        say(&mut view, "/settings"),
        Outcome::Settings,
        "the screen's rows are the config's and a frame holds no config, so the terminal — \
         which holds the one it launched with — is what seeds it"
    );
    assert_eq!(
        view.mode(),
        Mode::Composer,
        "and nothing is opened before the seeding: an empty form is not the screen"
    );

    view.open_settings(&Config::default());

    assert_eq!(
        view.mode(),
        Mode::Settings,
        "seeded, the screen is up and the draft is behind it"
    );
    assert!(
        painted(&mut view).contains("settings"),
        "and it says what a reader opened: {}",
        painted(&mut view)
    );
}

#[test]
fn model_opens_the_names_the_wire_serves_and_asks_the_wire_for_them() {
    let root = workspace_with("commit-message", "");
    let mut view = frame(root.path());

    assert_eq!(
        say(&mut view, "/model"),
        Outcome::Served,
        "a frame makes no request, so the screen going up is the frame's and the asking is \
         the terminal's (§10, ADR-0005)"
    );
    assert_eq!(view.mode(), Mode::Models);
    assert!(
        view.models().waiting(),
        "the screen is up before the names are, and says so"
    );
    assert!(
        painted(&mut view).contains("asking the wire what it serves"),
        "and it paints that rather than an empty list: {}",
        painted(&mut view)
    );
}

#[test]
fn model_named_by_hand_sets_a_name_without_opening_a_screen() {
    let root = workspace_with("commit-message", "");
    let mut view = frame(root.path());

    assert_eq!(
        say(&mut view, "/model some-name-the-wire-never-served"),
        Outcome::Model(String::from("some-name-the-wire-never-served")),
        "the name is handed to the terminal, which is the half that owns the file the one \
         save writes (ADR-0009)"
    );
    assert_eq!(
        view.mode(),
        Mode::Composer,
        "and no screen is opened for a name already given"
    );
}

#[test]
fn new_clears_the_transcript_the_frame_held() {
    let root = workspace_with("commit-message", "");
    let entries = vec![
        Entry::Turn { turn: 1 },
        Entry::Assistant {
            turn: 1,
            text: SecretSet::default().redact("the answer the conversation was about"),
        },
    ];
    let mut view = View::of(entries, sindri::rows::Scroll::Tail);
    view.set_workspace(root.path().to_path_buf());
    assert!(
        painted(&mut view).contains("the answer the conversation was about"),
        "the frame a run left holds its answer"
    );

    assert_eq!(say(&mut view, "/new"), Outcome::Edited);

    assert!(
        !painted(&mut view).contains("the answer the conversation was about"),
        "`/new` starts the conversation again, so nothing of the last one is on the frame: \
         {}",
        painted(&mut view)
    );
}

#[test]
fn new_empties_the_loaded_skill_set() {
    let root = workspace_with("commit-message", "");
    let mut view = frame(root.path());
    say(&mut view, "/skill:commit-message");
    assert_eq!(
        view.loaded().names(),
        ["commit-message"],
        "the line names a skill the catalog offers a person, so the set holds it"
    );

    say(&mut view, "/new");

    assert!(
        view.loaded().names().is_empty(),
        "a load is once per conversation and the conversation is the history, so clearing \
         the history empties the set (§4): {:?}",
        view.loaded().names()
    );
}

#[test]
fn a_skill_a_person_may_not_ask_for_is_not_loaded() {
    let root = workspace_with("model-only", "user-invocable: false\n");
    let mut view = frame(root.path());

    assert_eq!(
        say(&mut view, "/skill:model-only"),
        Outcome::Unchanged,
        "which names a person may ask for is the catalog's decision, and this one is the \
         model's door alone (§5)"
    );
    assert!(view.loaded().names().is_empty());
}

#[test]
fn a_skill_the_catalog_does_not_hold_loads_nothing() {
    let root = workspace_with("commit-message", "");
    let mut view = frame(root.path());

    assert_eq!(say(&mut view, "/skill:no-such-skill"), Outcome::Unchanged);
    assert!(
        view.loaded().names().is_empty(),
        "a name in the set would answer a later model load with `already_loaded` for a body \
         nothing ever handed over (§5)"
    );
}

#[test]
fn loading_one_skill_twice_is_one_load() {
    let root = workspace_with("commit-message", "");
    let mut view = frame(root.path());

    assert_eq!(say(&mut view, "/skill commit-message"), Outcome::Edited);
    assert_eq!(
        say(&mut view, "/skill:commit-message"),
        Outcome::Unchanged,
        "the two spellings are one line, and two loads of one skill in one conversation are \
         one load — the second carries no body, so it changes nothing"
    );
    assert_eq!(view.loaded().names(), ["commit-message"]);
}

#[test]
fn a_screen_answers_no_command_line() {
    let root = workspace_with("commit-message", "");
    let mut view = frame(root.path());
    view.key(Key::Char('?'));

    assert_eq!(
        view.mode(),
        Mode::Keys,
        "the keys screen is up, and it answers its own keys and nothing else"
    );
    assert_eq!(
        view.key(Key::Enter),
        Outcome::Unchanged,
        "Enter at a screen is not a line sent, so there is no command for it to name"
    );
}

#[test]
fn the_commands_are_read_off_the_draft_the_reader_typed() {
    let root = workspace_with("commit-message", "");
    let mut view = frame(root.path());
    view.paste("/model");

    assert_eq!(
        view.mode(),
        Mode::Composer,
        "a line typed is a draft, and nothing has been sent yet"
    );
    assert_eq!(view.composer().text(), "/model");
}

#[test]
fn a_frame_with_no_workspace_loads_no_skill() {
    // A frame told no workspace has no catalog to ask, and offering a skill from a directory no run
    // could read would name a name nothing can load.
    let mut view = View::boot();

    assert_eq!(say(&mut view, "/skill:commit-message"), Outcome::Unchanged);
    assert!(view.loaded().names().is_empty());
}
