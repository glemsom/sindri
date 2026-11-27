mod caps;
mod frame;

use std::path::Path;

use frame::Frame;
use sindri::capability::Capabilities;
use sindri::commands::{self, Command};
use sindri::composer::{Key, Outcome};
use sindri::facts::Facts;
use sindri::keys::Mode;
use sindri::ledger;
use sindri::mark::Cell;
use sindri::pointer::Pointer;
use sindri::view::{Transcript, View};
use sindri_core::config::Config;
use sindri_core::records::Entry;
use sindri_core::redact::SecretSet;
use sindri_core::run::Loaded;

const SCREEN: (u16, u16) = (80, 24);

fn say(view: &mut View, line: &str) -> Outcome {
    view.paste(line);
    view.key(Key::Enter)
}

fn frame(workspace: &Path) -> View {
    frame_holding(workspace, &Loaded::default())
}

fn frame_holding(workspace: &Path, loaded: &Loaded) -> View {
    View::of(
        Capabilities::default(),
        Facts::default()
            .asked()
            .working(workspace.to_path_buf(), None)
            .railed(ledger::Facts::default().loading(loaded.clone())),
        Transcript::none(),
    )
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

fn card_open(drawn: &Frame) -> bool {
    drawn
        .rows()
        .iter()
        .any(|row| row.trim_start().starts_with('\u{256d}') && row.contains("/ commands"))
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
fn legacy_skill_spellings_are_unknown_commands() {
    for draft in ["/skill:commit-message", "/skill commit-message"] {
        assert_eq!(commands::read(draft), None);
        assert!(commands::Card::of(draft).is_none());
    }
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
        "a name the wire never offered is still a name, and the key takes it whole"
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
    let drawn = frame::painted(&mut view, SCREEN.0, SCREEN.1)
        .rows()
        .join("\n");
    assert!(
        drawn.contains("settings"),
        "and it says what a reader opened: {drawn}"
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
         the terminal's"
    );
    assert_eq!(view.mode(), Mode::Models);
    assert!(
        view.models().waiting(),
        "the screen is up before the names are, and says so"
    );
    let drawn = frame::painted(&mut view, SCREEN.0, SCREEN.1)
        .rows()
        .join("\n");
    assert!(
        drawn.contains("asking the wire what it serves"),
        "and it paints that rather than an empty list: {drawn}"
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
         save writes"
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
    let mut view = View::of(
        Capabilities::default(),
        Facts::default()
            .asked()
            .working(root.path().to_path_buf(), None),
        Transcript::of(entries),
    );
    let held = frame::painted(&mut view, SCREEN.0, SCREEN.1)
        .rows()
        .join("\n");
    assert!(
        held.contains("the answer the conversation was about"),
        "the frame a run left holds its answer"
    );

    assert_eq!(say(&mut view, "/new"), Outcome::Edited);

    let drawn = frame::painted(&mut view, SCREEN.0, SCREEN.1)
        .rows()
        .join("\n");
    assert!(
        !drawn.contains("the answer the conversation was about"),
        "`/new` starts the conversation again, so nothing of the last one is on the frame: \
         {drawn}"
    );
}

#[test]
fn new_empties_the_loaded_skill_set() {
    let root = workspace_with("commit-message", "");
    let loaded = Loaded::default();
    let mut view = frame_holding(root.path(), &loaded);
    say(&mut view, "/commit-message");
    assert_eq!(
        loaded.names(),
        ["commit-message"],
        "the line names a skill the catalog offers a person, so the set holds it"
    );

    say(&mut view, "/new");

    assert!(
        loaded.names().is_empty(),
        "a load is once per conversation and the conversation is the history, so clearing \
         the history empties the one set: {:?}",
        loaded.names()
    );
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
    let loaded = Loaded::default();
    let mut view = View::of(
        Capabilities::default(),
        Facts::default()
            .asked()
            .railed(ledger::Facts::default().loading(loaded.clone())),
        Transcript::none(),
    );

    assert_eq!(say(&mut view, "/commit-message"), Outcome::Unchanged);
    assert!(loaded.names().is_empty());
}

#[test]
fn a_draft_whose_first_glyph_is_a_slash_draws_the_commands_card() {
    let root = workspace_with("commit-message", "");
    let mut view = frame(root.path());
    view.paste("/");

    let frame = frame::painted(&mut view, SCREEN.0, SCREEN.1);
    let drawn = frame.rows().join("\n");
    assert!(
        card_open(&frame),
        "a draft that begins with a slash opens the commands card: {drawn}"
    );
    for said in ["/model", "/quit", "end the launch"] {
        assert!(
            drawn.contains(said),
            "the card names the command and the one line that says what it does, and the frame \
             draws no {said:?}: {drawn}"
        );
    }
}

#[test]
fn the_commands_card_narrows_as_the_reader_types() {
    let root = workspace_with("commit-message", "");
    let mut view = frame(root.path());
    view.paste("/s");

    let drawn = frame::painted(&mut view, SCREEN.0, SCREEN.1)
        .rows()
        .join("\n");
    assert!(
        drawn.contains("/settings") && drawn.contains("/stats"),
        "`/s` leaves the commands that hold it: {drawn}"
    );
    assert!(
        !drawn.contains("/new"),
        "and the commands that hold nothing of it go: {drawn}"
    );
}

#[test]
fn enter_on_a_narrowed_draft_does_what_the_row_names() {
    let root = workspace_with("commit-message", "");
    let mut view = frame(root.path());
    view.paste("/qu");

    assert_eq!(
        view.key(Key::Enter),
        Outcome::Quit,
        "the row the draft names is the command `Enter` sends, so `/qu` ends the launch as \
         `/quit` does"
    );
}

#[test]
fn enter_on_a_draft_naming_no_command_still_sends_the_draft() {
    let root = workspace_with("commit-message", "");
    let mut view = frame(root.path());
    view.paste("/skillet");

    let sent = view.key(Key::Enter);

    match sent {
        Outcome::Send(prompt) => assert_eq!(
            prompt.text(),
            "/skillet",
            "a word no command holds is the reader's own words, and the model is asked them"
        ),
        other => panic!("a draft no command holds is sent, and this one was answered {other:?}"),
    }
    let drawn = frame::painted(&mut view, SCREEN.0, SCREEN.1);
    assert!(
        !card_open(&drawn),
        "and the card that held no row is closed: {}",
        drawn.rows().join("\n")
    );
}

#[test]
fn choosing_a_command_closes_the_commands_card() {
    let root = workspace_with("commit-message", "");
    let mut view = frame(root.path());
    view.paste("/set");
    assert!(card_open(&frame::painted(&mut view, SCREEN.0, SCREEN.1)));

    assert_eq!(view.key(Key::Enter), Outcome::Settings);
    let drawn = frame::painted(&mut view, SCREEN.0, SCREEN.1);
    assert!(
        !card_open(&drawn),
        "the row was chosen, so the card that offered it has done its work: {}",
        drawn.rows().join("\n")
    );
}

#[test]
fn enter_keeps_the_argument_typed_after_the_narrowed_word() {
    let root = workspace_with("commit-message", "");
    let mut view = frame(root.path());
    view.paste("/model a-name");

    assert_eq!(
        view.key(Key::Enter),
        Outcome::Model(String::from("a-name")),
        "the words after the name are the argument the command takes, and choosing the row \
         does not throw them away"
    );
}

#[test]
fn escape_closes_the_commands_card_and_leaves_the_draft_as_it_was() {
    let root = workspace_with("commit-message", "");
    let mut view = frame(root.path());
    view.paste("/s");

    assert_eq!(view.key(Key::Escape), Outcome::Edited);
    let drawn = frame::painted(&mut view, SCREEN.0, SCREEN.1);
    assert!(
        !card_open(&drawn),
        "the reader waved the card away: {}",
        drawn.rows().join("\n")
    );
    assert_eq!(view.composer().text(), "/s", "and the draft is untouched");

    match view.key(Key::Enter) {
        Outcome::Send(prompt) => assert_eq!(
            prompt.text(),
            "/s",
            "a card waved away stays away for that draft, so the draft is a prompt again"
        ),
        other => panic!("a draft whose card was closed is sent, and this one was {other:?}"),
    }
}

#[test]
fn a_press_on_a_command_row_chooses_it_the_way_a_file_row_is_chosen() {
    let root = workspace_with("deploy", "");
    let mut clicked = View::of(
        caps::pointing(),
        Facts::default()
            .asked()
            .working(root.path().to_path_buf(), None),
        Transcript::none(),
    );
    clicked.paste("/s");
    let drawn = frame::painted(&mut clicked, 120, 40);
    let row = drawn.area().y + drawn.row_holding(drawn.area(), "/stats");

    let mut typed = View::of(
        Capabilities::default(),
        Facts::default()
            .asked()
            .working(root.path().to_path_buf(), None),
        Transcript::none(),
    );
    typed.paste("/stats");

    assert_eq!(
        clicked.pointer(drawn.told(), Pointer::Press(Cell::new(row, 40))),
        typed.key(Key::Enter),
        "a press on the row that draws `/stats` sends what `Enter` on the typed line `/stats` \
         sends"
    );
    assert_eq!(
        clicked.composer().text(),
        typed.composer().text(),
        "which leaves the frame exactly as that typed line leaves it"
    );
    assert_eq!(
        clicked.mode(),
        typed.mode(),
        "with the same screen standing over it"
    );
}

#[test]
fn skill_rows_rank_names_and_show_only_human_invocable_descriptions() {
    let root = workspace_with("redeploy", "");
    for (name, flags) in [
        ("deploy", ""),
        ("deploy-staging", ""),
        ("deploy-model", "user-invocable: false\n"),
    ] {
        let directory = root.path().join(".agents/skills").join(name);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            directory.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: the {name} skill\n{flags}---\nBody.\n"),
        )
        .unwrap();
    }
    let catalog = sindri_core::skills::Catalog::discover_with_builtin(root.path(), None, &[]);
    let card = commands::Card::with_catalog("/dep", &catalog).unwrap();
    let rows = card.rows();
    assert_eq!(
        rows.iter().map(|row| row.name.as_ref()).collect::<Vec<_>>(),
        ["/deploy", "/deploy-staging", "/redeploy"]
    );
    assert_eq!(rows[0].says, "the deploy skill");
    assert_eq!(card.spelling(), "/deploy");
}

#[test]
fn skill_rows_keep_catalog_order_and_cap_and_count_only_long_lists() {
    let root = workspace_with("deploy-9", "");
    for number in 0..9 {
        let name = format!("deploy-{number}");
        let directory = root.path().join(".agents/skills").join(&name);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            directory.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: {name}\n---\nBody.\n"),
        )
        .unwrap();
    }
    let catalog = sindri_core::skills::Catalog::discover_with_builtin(root.path(), None, &[]);
    let card = commands::Card::with_catalog("/deploy-", &catalog).unwrap();
    assert_eq!(
        card.rows()
            .iter()
            .map(|row| row.name.to_string())
            .collect::<Vec<_>>(),
        (0..8)
            .map(|number| format!("/deploy-{number}"))
            .collect::<Vec<_>>()
    );
    assert_eq!(card.body().len(), 9);
    assert_eq!(
        card.body()[8]
            .spans
            .iter()
            .map(|(text, _)| text.as_str())
            .collect::<String>(),
        "1–8 of 10"
    );
    let short = commands::Card::with_catalog("/deploy-9", &catalog).unwrap();
    assert_eq!(short.body().len(), 1);
}

#[test]
fn skill_naming_displays_descriptions_and_refreshes_after_leaving() {
    let root = workspace_with("deploy", "");
    let mut view = frame(root.path());
    view.paste("/dep");
    let drawn = frame::painted(&mut view, SCREEN.0, SCREEN.1);
    assert!(
        drawn
            .rows()
            .iter()
            .any(|row| row.contains("/deploy") && row.contains("the deploy skill"))
    );
    let directory = root.path().join(".agents/skills/deploy-new");
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join("SKILL.md"),
        "---\nname: deploy-new\ndescription: freshly written\n---\nBody.\n",
    )
    .unwrap();
    view.key(Key::Char('l'));
    let drawn = frame::painted(&mut view, SCREEN.0, SCREEN.1);
    assert!(
        !drawn
            .rows()
            .iter()
            .any(|row| row.contains("freshly written"))
    );
    for _ in 0..view.composer().text().len() {
        view.key(Key::Backspace);
    }
    view.paste("/mod");
    for _ in 0..view.composer().text().len() {
        view.key(Key::Backspace);
    }
    view.paste("/dep");
    assert!(
        frame::painted(&mut view, SCREEN.0, SCREEN.1)
            .rows()
            .iter()
            .any(|row| row.contains("/deploy-new") && row.contains("freshly written"))
    );
}

#[test]
fn skill_rows_include_skills_written_before_the_card_is_prepared() {
    let root = workspace_with("deploy", "");
    let mut view = frame(root.path());
    view.paste("/deploy");
    let directory = root.path().join(".agents/skills/deploy-new");
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join("SKILL.md"),
        "---\nname: deploy-new\ndescription: freshly written\n---\nBody.\n",
    )
    .unwrap();

    let drawn = frame::painted(&mut view, SCREEN.0, SCREEN.1);
    assert!(drawn.rows().iter().any(|row| row.contains("/deploy-new")));
}

#[test]
fn loaded_skill_rows_lead_their_description_with_loaded() {
    let root = workspace_with("deploy", "");
    let catalog = sindri_core::skills::Catalog::discover_with_builtin(root.path(), None, &[]);
    let loaded = Loaded::from_iter(["deploy"]);
    let mut card = commands::Card::with_catalog("/dep", &catalog).unwrap();
    card.loading(&loaded);
    assert_eq!(card.rows()[0].says, "loaded · the deploy skill");
}

#[test]
fn the_view_draws_loaded_ahead_of_the_skill_description() {
    let root = workspace_with("deploy", "");
    let loaded = Loaded::from_iter(["deploy"]);
    let mut view = frame_holding(root.path(), &loaded);
    view.paste("/dep");
    let drawn = frame::painted(&mut view, 120, 40);
    assert!(
        drawn
            .rows()
            .iter()
            .any(|row| row.contains("loaded · the deploy skill"))
    );
    drawn.snapshot("commands-skill-loaded", "commands-skill-loaded-120x40");
}

#[test]
fn a_row_replaces_only_the_first_word() {
    for (draft, expected) in [
        ("/mod  gpt-5  ", "/model  gpt-5  "),
        ("/model gpt-5", "/model gpt-5"),
    ] {
        let card = commands::Card::of(draft).unwrap();
        assert_eq!(card.spelling(), expected);
    }
}

#[test]
fn tab_completes_without_sending_and_keeps_the_card_and_words() {
    let root = workspace_with("deploy", "");
    let mut view = frame(root.path());
    view.paste("/mod  gpt-5  ");
    frame::painted(&mut view, SCREEN.0, SCREEN.1);
    assert_eq!(view.key(Key::Tab), Outcome::Edited);
    assert_eq!(view.composer().text(), "/model  gpt-5  ");
    assert!(card_open(&frame::painted(&mut view, SCREEN.0, SCREEN.1)));
}

#[test]
fn shift_tab_and_up_walk_the_mark_back_and_stop_at_the_ends() {
    let root = workspace_with("deploy", "");
    let mut view = frame(root.path());
    view.paste("/");
    frame::painted(&mut view, SCREEN.0, SCREEN.1);
    view.key(Key::Down);
    assert_eq!(view.card().unwrap().selected(), 1);
    assert_eq!(view.key(Key::BackTab), Outcome::Edited);
    assert_eq!(view.card().unwrap().selected(), 0);
    assert_eq!(view.key(Key::BackTab), Outcome::Unchanged);
    assert_eq!(view.key(Key::Up), Outcome::Unchanged);
    for _ in 0..20 {
        view.key(Key::Down);
    }
    assert_eq!(view.card().unwrap().selected(), 7);
    view.key(Key::Up);
    assert_eq!(view.card().unwrap().selected(), 6);
}

#[test]
fn the_footer_leads_with_completion_only_while_a_card_is_drawn() {
    let root = workspace_with("deploy", "");
    let mut view = frame(root.path());
    view.paste("/mod");
    for width in [50, 120] {
        let drawn = frame::painted(&mut view, width, 24);
        assert!(
            drawn
                .rows()
                .last()
                .unwrap()
                .trim_start()
                .starts_with("Tab complete"),
            "{:?}",
            drawn.rows()
        );
    }
    view.key(Key::Escape);
    let drawn = frame::painted(&mut view, 120, 24);
    assert!(!drawn.rows().last().unwrap().contains("Tab"));
    let mut view = frame(root.path());
    view.key(Key::Char('?'));
    let drawn = frame::painted(&mut view, 120, 40);
    assert!(
        drawn
            .rows()
            .iter()
            .any(|row| row.contains("Tab") && row.contains("complete the marked"))
    );
    assert!(
        drawn
            .rows()
            .iter()
            .any(|row| row.contains("Shift+Tab") && row.contains("commands row back"))
    );
}

#[test]
fn unknown_first_class_skill_names_send_without_loading() {
    let root = workspace_with("deploy", "");
    let loaded = Loaded::default();
    let mut view = frame_holding(root.path(), &loaded);
    let draft = "/no-such-skill  words";
    view.paste(draft);
    assert!(!card_open(&frame::painted(&mut view, SCREEN.0, SCREEN.1)));
    match view.key(Key::Enter) {
        Outcome::Send(prompt) => assert_eq!(prompt.text(), draft),
        other => panic!("expected ordinary prompt, got {other:?}"),
    }
    assert!(loaded.names().is_empty());
}

#[test]
fn tab_is_inert_without_a_drawn_commands_card() {
    let root = workspace_with("deploy", "");
    for draft in ["bare", "@file", "words\n/mod", "/mod\nwords"] {
        let mut view = frame(root.path());
        view.paste(draft);
        let drawn = frame::painted(&mut view, SCREEN.0, SCREEN.1);
        assert!(!card_open(&drawn), "{draft}");
        assert_eq!(view.key(Key::Tab), Outcome::Unchanged, "{draft}");
        assert_eq!(view.composer().text(), draft);
    }
}

#[test]
fn escape_keeps_a_card_with_trailing_words_closed() {
    let root = workspace_with("deploy", "");
    let mut view = frame(root.path());
    view.paste("/mod  ");
    assert!(card_open(&frame::painted(&mut view, SCREEN.0, SCREEN.1)));
    view.key(Key::Escape);
    assert!(!card_open(&frame::painted(&mut view, SCREEN.0, SCREEN.1)));
    assert_eq!(view.key(Key::Tab), Outcome::Unchanged);
    assert_eq!(view.composer().text(), "/mod  ");
}

#[test]
fn tab_keeps_the_card_when_the_draft_has_later_lines() {
    let root = workspace_with("deploy", "");
    let mut view = frame(root.path());
    view.paste("/mod\nwords");
    for _ in 0..6 {
        view.key(Key::Left);
    }
    assert!(card_open(&frame::painted(&mut view, SCREEN.0, SCREEN.1)));
    assert_eq!(view.key(Key::Tab), Outcome::Edited);
    assert_eq!(view.composer().text(), "/model\nwords");
    assert!(card_open(&frame::painted(&mut view, SCREEN.0, SCREEN.1)));
}

#[test]
fn tab_completes_the_row_the_reader_marked() {
    let root = workspace_with("deploy", "");
    let mut view = frame(root.path());
    view.paste("/");
    frame::painted(&mut view, SCREEN.0, SCREEN.1);
    view.key(Key::Down);
    assert_eq!(view.key(Key::Tab), Outcome::Edited);
    assert_eq!(view.composer().text(), "/help");
    assert!(card_open(&frame::painted(&mut view, SCREEN.0, SCREEN.1)));
}

#[test]
fn tab_keeps_the_card_when_the_preserved_words_wrap() {
    let root = workspace_with("deploy", "");
    let mut view = frame(root.path());
    let tail = " words".repeat(30);
    view.paste(&format!("/mod{tail}"));
    frame::painted(&mut view, SCREEN.0, SCREEN.1);
    for _ in 0..tail.len() {
        view.key(Key::Left);
    }
    assert!(card_open(&frame::painted(&mut view, SCREEN.0, SCREEN.1)));
    assert_eq!(view.key(Key::Tab), Outcome::Edited);
    assert_eq!(view.composer().text(), format!("/model{tail}"));
    assert!(card_open(&frame::painted(&mut view, SCREEN.0, SCREEN.1)));
}

#[test]
fn no_workspace_retains_every_line_and_sends_nothing() {
    for draft in [
        "/deploy  kept",
        "/quit",
        "/skillet",
        "ordinary prompt\nkept  ",
    ] {
        let loaded = Loaded::default();
        let mut view = View::of(
            Capabilities::default(),
            Facts::default()
                .asked()
                .railed(ledger::Facts::default().loading(loaded.clone())),
            Transcript::none(),
        );
        view.paste(draft);
        assert_eq!(view.key(Key::Enter), Outcome::Unchanged, "{draft}");
        assert_eq!(view.composer().text(), draft);
        assert!(loaded.names().is_empty());
    }
}

#[test]
fn ordinary_prompts_send_the_exact_draft() {
    let root = workspace_with("deploy", "");
    for draft in ["ordinary  prompt\nkept  ", "/skillet  kept"] {
        let mut view = frame(root.path());
        view.paste(draft);
        match view.key(Key::Enter) {
            Outcome::Send(prompt) => assert_eq!(prompt.text(), draft),
            other => panic!("expected a prompt, got {other:?}"),
        }
        assert_eq!(view.composer().text(), "");
    }
}

#[test]
fn human_only_activation_protects_the_original_body_after_catalog_changes() {
    let root = workspace_with("deploy", "disable-model-invocation: true\n");
    let loaded = Loaded::default();
    let mut view = frame_holding(root.path(), &loaded);
    assert!(matches!(say(&mut view, "/deploy"), Outcome::Send(_)));
    let protected = loaded.protected();
    assert_eq!(protected.len(), 1);
    assert!(protected[0].contains("Body."));
    std::fs::remove_dir_all(root.path().join(".agents/skills/deploy")).unwrap();
    assert_eq!(loaded.protected(), protected);
}

#[test]
fn human_activation_keeps_repo_precedence_and_load_order() {
    let root = workspace_with("deploy", "");
    let loaded = Loaded::default();
    let mut view = frame_holding(root.path(), &loaded);
    assert!(matches!(say(&mut view, "/deploy"), Outcome::Send(_)));
    let catalog = sindri_core::skills::Catalog::discover(root.path(), None);
    assert_eq!(
        loaded.protected(),
        [catalog.get("deploy").unwrap().wrapper()]
    );
    let directory = root.path().join(".agents/skills/deploy");
    std::fs::write(
        directory.join("SKILL.md"),
        "---\nname: deploy\ndescription: changed\n---\nCHANGED_BODY",
    )
    .unwrap();
    assert!(matches!(say(&mut view, "/deploy"), Outcome::Send(_)));
    assert!(!loaded.protected()[0].contains("CHANGED_BODY"));
    assert_eq!(loaded.names(), ["deploy"]);
}

#[test]
fn first_class_skill_invocations_send_every_time_with_or_without_a_request() {
    let root = workspace_with("deploy", "disable-model-invocation: true\n");
    let loaded = Loaded::default();
    let mut view = frame_holding(root.path(), &loaded);
    for (draft, expected) in [
        ("/deploy", "Use the deploy skill."),
        (
            "/deploy ship staging",
            "Use the deploy skill.\n\nship staging",
        ),
        ("/deploy", "Use the deploy skill."),
    ] {
        match say(&mut view, draft) {
            Outcome::Send(prompt) => assert_eq!(prompt.text(), expected),
            other => panic!("expected skill invocation, got {other:?}"),
        }
        assert_eq!(loaded.names(), ["deploy"]);
        assert_eq!(loaded.protected().len(), 1);
        assert_eq!(view.composer().text(), "");
    }
}

#[test]
fn tab_completes_a_first_class_skill_then_enter_invokes_it() {
    let root = workspace_with("deploy", "");
    let loaded = Loaded::default();
    let mut view = frame_holding(root.path(), &loaded);
    view.paste("/dep ship staging");
    frame::painted(&mut view, SCREEN.0, SCREEN.1);
    assert_eq!(view.key(Key::Tab), Outcome::Edited);
    assert_eq!(view.composer().text(), "/deploy ship staging");
    match view.key(Key::Enter) {
        Outcome::Send(prompt) => assert_eq!(prompt.text(), "Use the deploy skill.\n\nship staging"),
        other => panic!("expected skill invocation, got {other:?}"),
    }
    assert_eq!(loaded.names(), ["deploy"]);
}

#[test]
fn enter_invokes_the_highlighted_first_class_skill() {
    let root = workspace_with("deploy", "");
    let loaded = Loaded::default();
    let mut view = frame_holding(root.path(), &loaded);
    view.paste("/");
    frame::painted(&mut view, SCREEN.0, SCREEN.1);
    assert_eq!(view.card().unwrap().spelling(), "/deploy");
    view.key(Key::Down);
    view.key(Key::Up);
    match view.key(Key::Enter) {
        Outcome::Send(prompt) => assert_eq!(prompt.text(), "Use the deploy skill."),
        other => panic!("expected skill invocation, got {other:?}"),
    }
    assert_eq!(loaded.names(), ["deploy"]);
}

#[test]
fn nonhuman_skills_are_neither_commands_nor_completions() {
    let root = workspace_with("deploy", "user-invocable: false\n");
    let catalog = sindri_core::skills::Catalog::discover(root.path(), None);
    assert!(commands::Card::with_catalog("/dep", &catalog).is_none());
    assert!(commands::read_with_catalog("/deploy", &catalog).is_none());
    let loaded = Loaded::default();
    let mut view = frame_holding(root.path(), &loaded);
    match say(&mut view, "/deploy ship") {
        Outcome::Send(prompt) => assert_eq!(prompt.text(), "/deploy ship"),
        other => panic!("expected ordinary prompt, got {other:?}"),
    }
    assert!(matches!(say(&mut view, "/deploy"), Outcome::Send(_)));
    assert!(loaded.names().is_empty());
}

#[test]
fn builtin_collisions_keep_only_the_builtin_command() {
    let root = workspace_with("help", "");
    let catalog = sindri_core::skills::Catalog::discover_with_builtin(root.path(), None, &[]);
    let card = commands::Card::with_catalog("/help", &catalog).unwrap();
    assert_eq!(
        card.rows().iter().filter(|row| row.name == "/help").count(),
        1
    );
    assert_eq!(
        commands::read_with_catalog("/help", &catalog),
        Some(Command::Help)
    );
    let loaded = Loaded::default();
    let mut view = frame_holding(root.path(), &loaded);
    assert_eq!(say(&mut view, "/help"), Outcome::Edited);
    assert!(loaded.names().is_empty());
}

#[test]
fn legacy_skill_invocations_never_complete_load_or_invoke() {
    let root = workspace_with("deploy", "");
    let catalog = sindri_core::skills::Catalog::discover_with_builtin(root.path(), None, &[]);
    for draft in [
        "/skill:deploy",
        "/skill deploy",
        "/skill:deploy  ship\nkept  ",
        "/skill deploy ship",
    ] {
        assert!(commands::read_with_catalog(draft, &catalog).is_none());
        assert!(commands::Card::with_catalog(draft, &catalog).is_none());
        let loaded = Loaded::default();
        let mut view = frame_holding(root.path(), &loaded);
        view.paste(draft);
        let drawn = frame::painted(&mut view, 120, 40);
        assert!(!card_open(&drawn));
        if draft == "/skill:deploy" {
            drawn.snapshot("commands-legacy-prompt", "commands-legacy-prompt-120x40");
        }
        assert_eq!(view.key(Key::Tab), Outcome::Unchanged);
        assert_eq!(view.composer().text(), draft);
        match view.key(Key::Enter) {
            Outcome::Send(prompt) => assert_eq!(prompt.text(), draft),
            other => panic!("expected unchanged legacy prompt, got {other:?}"),
        }
        assert!(loaded.names().is_empty());
        assert!(loaded.protected().is_empty());
    }
}

#[test]
fn skill_is_an_available_first_class_skill_name() {
    let root = workspace_with("skill", "");
    let loaded = Loaded::default();
    let mut view = frame_holding(root.path(), &loaded);
    view.paste("/ski request");
    frame::painted(&mut view, 120, 40);
    assert_eq!(view.key(Key::Tab), Outcome::Edited);
    assert_eq!(view.composer().text(), "/skill request");
    match view.key(Key::Enter) {
        Outcome::Send(prompt) => assert_eq!(prompt.text(), "Use the skill skill.\n\nrequest"),
        other => panic!("expected skill invocation, got {other:?}"),
    }
    assert_eq!(loaded.names(), ["skill"]);
}
