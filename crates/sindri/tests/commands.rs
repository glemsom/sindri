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
fn the_two_skill_spellings_are_one_line() {
    assert_eq!(
        commands::read("/skill:commit-message"),
        commands::read("/skill commit-message"),
        "the rule says `/skill:name` and `/skill name` are the same line, so they are one value \
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
    say(&mut view, "/skill:commit-message");
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
fn a_skill_a_person_may_not_ask_for_is_not_loaded() {
    let root = workspace_with("model-only", "user-invocable: false\n");
    let loaded = Loaded::default();
    let mut view = frame_holding(root.path(), &loaded);

    assert_eq!(
        say(&mut view, "/skill:model-only"),
        Outcome::Unchanged,
        "which names a person may ask for is the catalog's decision, and this one is the \
         model's door alone"
    );
    assert!(loaded.names().is_empty());
}

#[test]
fn a_skill_the_catalog_does_not_hold_loads_nothing() {
    let root = workspace_with("commit-message", "");
    let loaded = Loaded::default();
    let mut view = frame_holding(root.path(), &loaded);

    assert_eq!(say(&mut view, "/skill:no-such-skill"), Outcome::Unchanged);
    assert!(
        loaded.names().is_empty(),
        "a name in the set would answer a later model load with `already_loaded` for a body \
         nothing ever handed over"
    );
}

#[test]
fn loading_one_skill_twice_is_one_load() {
    let root = workspace_with("commit-message", "");
    let loaded = Loaded::default();
    let mut view = frame_holding(root.path(), &loaded);

    assert_eq!(say(&mut view, "/skill commit-message"), Outcome::Edited);
    assert_eq!(
        say(&mut view, "/skill:commit-message"),
        Outcome::Unchanged,
        "the two spellings are one line, and two loads of one skill in one conversation are \
         one load — the second carries no body, so it changes nothing"
    );
    assert_eq!(loaded.names(), ["commit-message"]);
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

    assert_eq!(say(&mut view, "/skill:commit-message"), Outcome::Unchanged);
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
        drawn.contains("/settings") && drawn.contains("/skill"),
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
fn a_command_that_takes_a_word_is_written_into_the_draft_to_name_it() {
    let root = workspace_with("commit-message", "");
    let loaded = Loaded::default();
    let mut view = frame_holding(root.path(), &loaded);
    view.paste("/sk");

    assert_eq!(view.key(Key::Enter), Outcome::Edited);
    assert_eq!(
        view.composer().text(),
        "/skill:",
        "`/skill` is no command without a word after it, so the row writes the name and the \
         reader says which skill"
    );
    let drawn = frame::painted(&mut view, SCREEN.0, SCREEN.1);
    assert!(
        card_open(&drawn),
        "the card stands while the word is named: {}",
        drawn.rows().join("\n")
    );

    view.paste("commit-message");
    assert_eq!(view.key(Key::Enter), Outcome::Edited);
    assert_eq!(loaded.names(), ["commit-message"]);
}

#[test]
fn a_press_on_a_command_row_chooses_it_the_way_a_file_row_is_chosen() {
    let mut clicked = View::of(
        caps::pointing(),
        Facts::default().asked(),
        Transcript::none(),
    );
    clicked.paste("/s");
    let drawn = frame::painted(&mut clicked, 120, 40);
    let row = drawn.area().y + drawn.row_holding(drawn.area(), "/stats");

    let mut typed = View::of(
        Capabilities::default(),
        Facts::default().asked(),
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
fn skill_rows_rank_the_tail_and_show_only_human_invocable_descriptions() {
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
    let card = commands::Card::with_catalog("/skill:dep", &catalog).unwrap();
    let rows = card.rows();
    assert_eq!(
        rows.iter().map(|row| row.name.as_ref()).collect::<Vec<_>>(),
        [
            "/skill:deploy",
            "/skill:deploy-staging",
            "/skill:redeploy",
            "/skill:"
        ]
    );
    assert_eq!(rows[0].says, "the deploy skill");
    assert_eq!(card.spelling(), "/skill:deploy");
}

#[test]
fn skill_rows_keep_catalog_order_and_cap_and_count_only_long_lists() {
    let root = workspace_with("zulu", "");
    for name in [
        "alpha", "bravo", "charlie", "delta", "echo", "foxtrot", "golf", "hotel",
    ] {
        let directory = root.path().join(".agents/skills").join(name);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            directory.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: {name}\n---\nBody.\n"),
        )
        .unwrap();
    }
    let catalog = sindri_core::skills::Catalog::discover_with_builtin(root.path(), None, &[]);
    for draft in ["/skill", "/skill:"] {
        let card = commands::Card::with_catalog(draft, &catalog).unwrap();
        assert_eq!(
            card.rows()
                .iter()
                .map(|row| row.name.as_ref())
                .collect::<Vec<_>>(),
            [
                "/skill:",
                "/skill:alpha",
                "/skill:bravo",
                "/skill:charlie",
                "/skill:delta",
                "/skill:echo",
                "/skill:foxtrot",
                "/skill:golf"
            ]
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
    }
    let short = commands::Card::with_catalog("/skill:zul", &catalog).unwrap();
    assert_eq!(short.body().len(), 2);
}

#[test]
fn skill_naming_displays_descriptions_and_refreshes_after_leaving() {
    let root = workspace_with("deploy", "");
    let mut view = frame(root.path());
    view.paste("/skill:dep");
    let drawn = frame::painted(&mut view, SCREEN.0, SCREEN.1);
    assert!(
        drawn
            .rows()
            .iter()
            .any(|row| row.contains("/skill:deploy") && row.contains("the deploy skill"))
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
    view.paste("/skill:dep");
    assert!(
        frame::painted(&mut view, SCREEN.0, SCREEN.1)
            .rows()
            .iter()
            .any(|row| row.contains("/skill:deploy-new") && row.contains("freshly written"))
    );
}

#[test]
fn the_skill_command_row_hands_the_reader_its_colon() {
    let root = workspace_with("deploy", "");
    let catalog = sindri_core::skills::Catalog::discover_with_builtin(root.path(), None, &[]);
    let card = commands::Card::with_catalog("/skill", &catalog).unwrap();
    assert_eq!(card.spelling(), "/skill:");
}

#[test]
fn skill_rows_include_skills_written_before_the_card_is_prepared() {
    let root = workspace_with("deploy", "");
    let mut view = frame(root.path());
    view.paste("/skill:deploy");
    let directory = root.path().join(".agents/skills/deploy-new");
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join("SKILL.md"),
        "---\nname: deploy-new\ndescription: freshly written\n---\nBody.\n",
    )
    .unwrap();

    let drawn = frame::painted(&mut view, SCREEN.0, SCREEN.1);
    assert!(
        drawn
            .rows()
            .iter()
            .any(|row| row.contains("/skill:deploy-new"))
    );
}

#[test]
fn loaded_skill_rows_lead_their_description_with_loaded() {
    let root = workspace_with("deploy", "");
    let catalog = sindri_core::skills::Catalog::discover_with_builtin(root.path(), None, &[]);
    let loaded = Loaded::from_iter(["deploy"]);
    let mut card = commands::Card::with_catalog("/skill:dep", &catalog).unwrap();
    card.loading(&loaded);
    assert_eq!(card.rows()[0].says, "loaded · the deploy skill");
}

#[test]
fn an_unmatched_skill_tail_draws_a_refusal_instead_of_command_rows() {
    let root = workspace_with("deploy", "");
    let catalog = sindri_core::skills::Catalog::discover_with_builtin(root.path(), None, &[]);
    let card = commands::Card::with_catalog("/skill:zx", &catalog).unwrap();
    assert!(card.rows().is_empty());
    let body = card.body();
    assert_eq!(body.len(), 1);
    assert_eq!(
        body[0]
            .spans
            .iter()
            .map(|(text, _)| text.as_str())
            .collect::<String>(),
        "no skill matches zx"
    );
}

#[test]
fn a_refusal_row_cannot_be_chosen_or_spell_a_replacement() {
    use sindri::kept::{Kept, Move};
    let root = workspace_with("deploy", "");
    let catalog = sindri_core::skills::Catalog::discover_with_builtin(root.path(), None, &[]);
    let mut card = commands::Card::with_catalog("/skill:zx keep this", &catalog).unwrap();
    assert!(!card.pressed(0));
    assert!(!card.walked(Move::Next));
    assert_eq!(card.spelling_of(0), None);
    assert_eq!(card.spelling(), "/skill:zx keep this");
}

#[test]
fn the_view_draws_loaded_ahead_of_the_skill_description() {
    let root = workspace_with("deploy", "");
    let loaded = Loaded::from_iter(["deploy"]);
    let mut view = frame_holding(root.path(), &loaded);
    view.paste("/skill:dep");
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
fn a_refusal_keeps_the_card_open_and_both_keys_leave_the_draft() {
    let root = workspace_with("deploy", "");
    let mut view = frame(root.path());
    view.paste("/skill:zx keep this");
    let drawn = frame::painted(&mut view, 120, 40);
    assert!(card_open(&drawn));
    assert!(
        drawn
            .rows()
            .iter()
            .any(|row| row.contains("no skill matches zx"))
    );
    view.key(Key::Tab);
    assert_eq!(view.composer().text(), "/skill:zx keep this");
    assert_eq!(view.key(Key::Enter), Outcome::Unchanged);
    assert_eq!(view.composer().text(), "/skill:zx keep this");
    frame::painted(&mut view, 120, 40)
        .snapshot("commands-skill-refusal", "commands-skill-refusal-120x40");
    for draft in ["/skillet", "/usr"] {
        let mut view = frame(root.path());
        view.paste(draft);
        assert!(!card_open(&frame::painted(&mut view, 120, 40)));
        assert_eq!(view.composer().text(), draft);
    }
}

#[test]
fn a_refusal_footer_does_not_offer_sending_until_the_tail_matches() {
    let root = workspace_with("deploy", "");
    let mut view = frame(root.path());
    view.paste("/skill:zx");
    let drawn = frame::painted(&mut view, 120, 40);
    assert!(!drawn.rows().iter().any(|row| row.contains("Enter send")));
    view.key(Key::Backspace);
    view.key(Key::Backspace);
    view.paste("dep");
    let drawn = frame::painted(&mut view, 120, 40);
    assert!(drawn.rows().iter().any(|row| row.contains("Enter send")));
}

#[test]
fn a_row_replaces_only_the_first_word_and_hands_over_the_skill_colon() {
    for (draft, expected) in [
        ("/ski  words\tkept\nnext  ", "/skill:  words\tkept\nnext  "),
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
    assert_eq!(view.card().unwrap().selected(), 6);
    view.key(Key::Up);
    assert_eq!(view.card().unwrap().selected(), 5);
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
fn tab_and_enter_hand_the_skill_row_over_to_naming() {
    let root = workspace_with("deploy", "");
    for key in [Key::Tab, Key::Enter] {
        let mut view = frame(root.path());
        view.paste("/ski");
        frame::painted(&mut view, SCREEN.0, SCREEN.1);
        assert_eq!(view.key(key), Outcome::Edited);
        assert_eq!(view.composer().text(), "/skill:");
        let drawn = frame::painted(&mut view, SCREEN.0, SCREEN.1);
        assert!(card_open(&drawn));
        assert!(drawn.rows().iter().any(|row| row.contains("/skill:deploy")));
    }
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
