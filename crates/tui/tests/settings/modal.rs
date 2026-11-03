//! The modal itself: every key of the schema on screen, a save that applies,
//! a refusal that writes nothing, and a draft `Esc` throws away.

use super::*;
use sindri_tui::overlay::Overlay;

#[test]
fn the_thinking_toggle_is_a_keystroke_and_lands_in_the_file() {
    let mut f = fixture(Some("model = \"m\"\n"));
    open(&mut f.app);
    down_to(&mut f.app, Key::Thinking);
    assert!(
        row(&screen(&mut f.app, 100, 30), "thinking").contains("off"),
        "the file said nothing, so the toggle shows the run's default"
    );
    press(&mut f.app, KeyCode::Char(' '), KeyModifiers::NONE);
    assert!(
        row(&screen(&mut f.app, 100, 30), "thinking").contains("on"),
        "space flips it"
    );
    press(&mut f.app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(
        written(&f.path).contains("thinking = true"),
        "and saving writes it: {}",
        written(&f.path)
    );
}

#[test]
fn the_extra_writable_paths_are_a_field_the_run_would_accept() {
    let mut f = fixture(Some("model = \"m\"\n"));
    open(&mut f.app);
    down_to(&mut f.app, Key::ExtraWritablePaths);
    press(&mut f.app, KeyCode::Char('u'), KeyModifiers::CONTROL);
    type_text(&mut f.app, "/usr/share");
    press(&mut f.app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(
        written(&f.path).contains("/usr/share"),
        "the path is in the file: {}",
        written(&f.path)
    );
}

#[test]
fn an_unknown_key_in_the_file_refuses_the_modal_and_says_why() {
    let mut f = fixture(Some("model = \"m\"\nmax_turn = 5\n"));
    open(&mut f.app);

    assert!(
        f.app.overlay().is_none(),
        "no modal opens over a broken file"
    );
    let said = notes(&f.app);
    assert!(
        said.contains("max_turn") && said.contains(&f.path.display().to_string()),
        "the transcript names the key and the file: {said}"
    );
    assert_eq!(
        written(&f.path),
        "model = \"m\"\nmax_turn = 5\n",
        "and nothing was rewritten"
    );
}

#[test]
fn the_modal_fits_a_hundred_columns_and_a_hundred_and_sixty() {
    let mut f = fixture(Some("model = \"qwen3-32b\"\napi_key = \"sk-hidden\"\n"));
    open(&mut f.app);
    down_to(&mut f.app, Key::ApiKey);
    type_text(&mut f.app, "sk-typed-secret");
    for width in [100u16, 160] {
        let rows = screen(&mut f.app, width, 30);
        let text = shown(&rows);
        for wanted in [
            "settings",
            "provider",
            "model",
            "api_key",
            "thinking",
            "extra_headers",
            "plaintext",
            "Enter save",
            "Esc discard",
        ] {
            assert!(
                text.contains(wanted),
                "{width} columns shows {wanted}: {text}"
            );
        }
        assert!(
            !text.contains("sk-typed-secret") && !text.contains("sk-hidden"),
            "{width} columns never shows a secret: {text}"
        );
    }
}

#[test]
fn the_modal_dims_the_frame_it_covers_and_reads_at_full_strength_itself() {
    let mut f = fixture(Some("model = \"m\"\n"));
    open(&mut f.app);
    let buf = buffer(&mut f.app, 100, 30);

    assert!(
        buf[(0, 0)].modifier.contains(Modifier::DIM),
        "what the modal leaves uncovered recedes instead of reading as a hole"
    );
    let title = (0..30)
        .find(|y| {
            (0..100)
                .map(|x| buf[(x, *y)].symbol())
                .collect::<String>()
                .contains(" settings ")
        })
        .expect("the modal's title row");
    // The rail keeps showing to the right of the modal, dimmed: the modal's
    // own columns are the ones between its two corners.
    let left = (0..100)
        .find(|x| buf[(*x, title)].symbol() == "╭")
        .expect("the modal's left corner");
    let right = (left..100)
        .find(|x| buf[(*x, title)].symbol() == "╮")
        .expect("the modal's right corner");
    assert!(
        (left..right)
            .filter(|x| buf[(*x, title)].symbol() != " ")
            .all(|x| !buf[(x, title)].modifier.contains(Modifier::DIM)),
        "and the modal itself is drawn at full strength: {title}"
    );
}

/// The floor this host has, resolved the way the row resolves it: the modal
/// shows a run's own mount, so a test on a machine with a home directory asks
/// the host rather than assuming one.
fn host_floor() -> Vec<PathBuf> {
    let workspace = std::env::current_dir().expect("a working directory");
    sindri_core::sandbox::Extras::Floor.resolve(sindri_core::paths::home().as_deref(), &workspace)
}

/// The text the modal holds for `key`: the field is what a save writes, so
/// what a row says is read here rather than off a truncated screen row.
fn field(f: &Fixture, key: Key) -> String {
    let Some(Overlay::Settings(settings)) = f.app.overlay() else {
        panic!("the settings modal is open");
    };
    settings
        .fields()
        .iter()
        .find(|field| field.key == key)
        .expect("every key has a field")
        .text
        .clone()
}

#[test]
fn the_writable_row_starts_at_the_floor_so_a_blank_row_never_means_four_mounts() {
    let mut f = fixture(Some("model = \"m\"\n"));
    open(&mut f.app);
    down_to(&mut f.app, Key::ExtraWritablePaths);

    let floor: Vec<String> = host_floor()
        .iter()
        .map(|path| path.display().to_string())
        .collect();
    assert_eq!(
        field(&f, Key::ExtraWritablePaths),
        floor.join(":"),
        "the row starts at the shipped floor, so no blank row stands for four mounts"
    );
}

#[test]
fn emptying_the_writable_field_writes_an_explicit_empty_list() {
    let mut f = fixture(Some("model = \"m\"\n"));
    open(&mut f.app);
    down_to(&mut f.app, Key::ExtraWritablePaths);

    // Deleting the row's text is the act that means none: the field is left
    // holding nothing, and nothing is a value this key can carry.
    for _ in 0..=field(&f, Key::ExtraWritablePaths).len() {
        press(&mut f.app, KeyCode::Backspace, KeyModifiers::NONE);
    }
    press(&mut f.app, KeyCode::Enter, KeyModifiers::NONE);

    assert!(
        written(&f.path).contains("extra_writable_paths = []"),
        "an emptied row writes the empty list, not an absence: {}",
        written(&f.path)
    );
}

#[test]
fn clearing_the_writable_field_leaves_the_key_out_of_the_file() {
    let mut f = fixture(Some(
        "model = \"m\"\nextra_writable_paths = [\"/usr/share\"]\n",
    ));
    open(&mut f.app);
    down_to(&mut f.app, Key::ExtraWritablePaths);
    assert!(
        row(&screen(&mut f.app, 100, 30), "extra_writable_paths").contains("/usr/share"),
        "the file's own list is on screen"
    );

    press(&mut f.app, KeyCode::Char('u'), KeyModifiers::CONTROL);
    press(&mut f.app, KeyCode::Enter, KeyModifiers::NONE);

    let file = written(&f.path);
    assert!(
        !file.contains("extra_writable_paths"),
        "Ctrl-U unsets the key, so the shipped floor speaks again: {file}"
    );
    assert!(
        file.contains("model = \"m\""),
        "and nothing else moved: {file}"
    );
}

#[test]
fn the_ssh_agent_is_one_row_a_toggle_and_on_by_default() {
    let mut f = fixture(Some("model = \"m\"\n"));
    open(&mut f.app);
    down_to(&mut f.app, Key::SshAgent);
    assert!(
        row(&screen(&mut f.app, 100, 30), "ssh_agent").contains("on (default)"),
        "the file says nothing, so the row states the default the run is on"
    );
    press(&mut f.app, KeyCode::Char(' '), KeyModifiers::NONE);
    press(&mut f.app, KeyCode::Char(' '), KeyModifiers::NONE);
    assert!(
        row(&screen(&mut f.app, 100, 30), "ssh_agent").contains("off"),
        "space flips it"
    );
    press(&mut f.app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(
        written(&f.path).contains("ssh_agent = false"),
        "and saving writes the setting the run reads: {}",
        written(&f.path)
    );
}

/// A config file whose path is longer than `cells` whatever the sandbox's own
/// temp directory is: the directories stay short, so every break has a
/// separator to fall on, and the length the frame is asked to hold is a fact
/// about this test rather than about whose `TMPDIR` it runs in.
fn long_path(dir: &TempDir, cells: usize) -> PathBuf {
    let mut path = dir.path().to_path_buf();
    while path.display().to_string().len() < cells {
        path.push("dddddddd");
    }
    path.join("sindri/config.toml")
}

#[test]
fn a_path_the_frame_cannot_hold_breaks_at_a_separator_and_never_splits_the_name() {
    let dir = TempDir::new().expect("a temp directory");
    let path = long_path(&dir, 180);
    let mut app = App::new(common::launch(), None).with_config(&path);
    open(&mut app);

    let rows = shown(&screen(&mut app, 100, 30));
    assert!(
        rows.contains("config.toml"),
        "the file's name reaches a reader whole: {rows}"
    );
    for cut in [
        "c",
        "co",
        "con",
        "conf",
        "confi",
        "config",
        "config.",
        "config.t",
        "config.to",
        "config.tom",
    ] {
        assert!(
            !rows.contains(&format!("{cut}\n")),
            "no row ends inside the name, cut after {cut:?}: {rows}"
        );
    }
}
