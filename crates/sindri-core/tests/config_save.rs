//! Proves a config save touches only the keys it was handed, through a staged
//! temp file renamed into place: a person's comments and key order survive, the
//! file lands owner-only, and a save that cannot land leaves the old one whole.

use std::collections::BTreeMap;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

use sindri_core::config::{Config, Flags, Touched};
use sindri_core::provider::Route;
use sindri_core::wire::Wire;
use tempfile::TempDir;

/// A config home holding one `config.toml` with `text` in it, as a person left it.
fn config_home(text: &str) -> TempDir {
    let home = tempfile::tempdir().expect("a temp dir");
    let directory = home.path().join("sindri");
    std::fs::create_dir_all(&directory).expect("the sindri directory");
    std::fs::write(directory.join("config.toml"), text).expect("the config file");
    home
}

/// A config home with no `config.toml` in it at all: a first run.
fn bare_home() -> TempDir {
    tempfile::tempdir().expect("a temp dir")
}

/// The one file inside a config home.
fn file_in(home: &TempDir) -> PathBuf {
    home.path().join("sindri").join("config.toml")
}

/// The file's text, read the way any other reader would read it.
fn read(path: &Path) -> String {
    std::fs::read_to_string(path).expect("a readable config")
}

/// The permission bits a file carries, without the file-type bits beside them.
fn mode(path: &Path) -> u32 {
    std::fs::metadata(path)
        .expect("a file to stat")
        .permissions()
        .mode()
        & 0o777
}

/// Every name in a directory, sorted, so a leftover temp file cannot hide.
fn names_in(directory: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(directory)
        .expect("a readable directory")
        .map(|entry| {
            entry
                .expect("an entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

/// A file a person wrote: comments above keys and beside them, keys in an order
/// no serializer would choose, and a blank line where they wanted one.
const HAND_WRITTEN: &str = "\
# My settings, and my notes on them. The comments are the point.
status = false   # OSC 7501 reporting, off until I trust it

# The gateway moved last spring; this is where it went.
base_url = \"https://gateway.test/v1\"
# swapped a few times a week
model = \"old-model\"  # note beside it
max_turns = 250

thinking = true
";

/// [`HAND_WRITTEN`] after one edit, and not one byte of anything else: the
/// comments stay, the order stays, and the touched line keeps its neighbours.
const HAND_WRITTEN_AFTER_ONE_MODEL: &str = "\
# My settings, and my notes on them. The comments are the point.
status = false   # OSC 7501 reporting, off until I trust it

# The gateway moved last spring; this is where it went.
base_url = \"https://gateway.test/v1\"
# swapped a few times a week
model = \"new-model\"  # note beside it
max_turns = 250

thinking = true
";

#[test]
fn a_file_with_comments_and_unusual_key_order_keeps_both() {
    let home = config_home(HAND_WRITTEN);
    let path = file_in(&home);

    Config::save_to(
        &path,
        &Flags {
            model: Some(String::from("new-model")),
            ..Flags::default()
        },
    )
    .expect("the save lands");

    assert_eq!(
        read(&path),
        HAND_WRITTEN_AFTER_ONE_MODEL,
        "a save must keep the comments and the key order it found"
    );
}

/// The line numbers that differ between two texts that hold the same number of
/// lines, so a test can say exactly which lines a save rewrote.
fn differing_lines(before: &str, after: &str) -> Vec<usize> {
    let before: Vec<&str> = before.lines().collect();
    let after: Vec<&str> = after.lines().collect();
    assert_eq!(
        before.len(),
        after.len(),
        "a save of existing keys must not add or drop a line:\nbefore: {before:#?}\nafter: {after:#?}"
    );
    (0..before.len())
        .filter(|line| before[*line] != after[*line])
        .collect()
}

#[test]
fn only_the_touched_keys_change_and_every_other_byte_is_identical() {
    let home = config_home(HAND_WRITTEN);
    let path = file_in(&home);

    Config::save_to(
        &path,
        &Flags {
            thinking: Some(false),
            max_turns: Some(400),
            ..Flags::default()
        },
    )
    .expect("the save lands");

    let after = read(&path);
    let changed = differing_lines(HAND_WRITTEN, &after);
    let touched: Vec<usize> = HAND_WRITTEN
        .lines()
        .enumerate()
        .filter(|(_, line)| line.starts_with("thinking") || line.starts_with("max_turns"))
        .map(|(index, _)| index)
        .collect();

    assert_eq!(
        changed, touched,
        "only the keys the save was handed may change, and the file is:\n{after}"
    );
    assert_eq!(
        Config::load_from(&path).expect("the file still reads"),
        Config {
            model: Some(String::from("old-model")),
            base_url: Some(String::from("https://gateway.test/v1")),
            status: Some(false),
            thinking: Some(false),
            max_turns: Some(400),
            ..Config::default()
        }
    );
}

#[test]
fn a_key_the_file_never_had_is_appended_after_the_ones_it_has() {
    let home = config_home(HAND_WRITTEN);
    let path = file_in(&home);

    Config::save_to(
        &path,
        &Flags {
            provider: Some(Wire::Openai),
            ..Flags::default()
        },
    )
    .expect("the save lands");

    assert_eq!(
        read(&path),
        format!("{HAND_WRITTEN}provider = \"openai\"\n"),
        "a key the file never had is appended, and nothing else moves"
    );
}

#[test]
fn a_save_leaves_the_config_readable_by_its_owner_alone() {
    let home = config_home(HAND_WRITTEN);
    let path = file_in(&home);
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))
        .expect("the file left as wide as a person's umask would leave it");

    Config::save_to(
        &path,
        &Flags {
            model: Some(String::from("new-model")),
            ..Flags::default()
        },
    )
    .expect("the save lands");

    assert_eq!(
        mode(&path),
        0o600,
        "the save is owner-only even when the file it replaced was not"
    );

    let first_run = bare_home();
    let created = first_run.path().join("sindri").join("config.toml");
    Config::save_to(
        &created,
        &Flags {
            max_turns: Some(10),
            ..Flags::default()
        },
    )
    .expect("a first run needs no file to start from");

    assert_eq!(mode(&created), 0o600, "a first save is owner-only too");
    assert_eq!(read(&created), "max_turns = 10\n");
}

#[test]
fn an_emptied_key_leaves_the_file_so_the_environment_speaks() {
    let home = config_home(HAND_WRITTEN);
    let path = file_in(&home);

    Config::save_to(
        &path,
        &Flags {
            model: Some(String::new()),
            ..Flags::default()
        },
    )
    .expect("the save lands");

    let after = read(&path);
    assert!(
        !after.contains("model"),
        "an emptied key is gone from the file, which is what hands it back to the \
         environment, and the file is:\n{after}"
    );
    assert_eq!(
        Config::load_from(&path).expect("the file still reads"),
        Config {
            base_url: Some(String::from("https://gateway.test/v1")),
            status: Some(false),
            thinking: Some(true),
            max_turns: Some(250),
            ..Config::default()
        }
    );
}

#[test]
fn a_save_replaces_the_file_rather_than_writing_into_it() {
    let home = config_home(HAND_WRITTEN);
    let path = file_in(&home);
    let witness = home.path().join("witness.toml");
    std::fs::hard_link(&path, &witness).expect("a second name for the same bytes");

    Config::save_to(
        &path,
        &Flags {
            model: Some(String::from("new-model")),
            ..Flags::default()
        },
    )
    .expect("the save lands");

    assert_eq!(
        read(&witness),
        HAND_WRITTEN,
        "a save that wrote into the file would have changed every name for it, so a \
         reader would have seen the file half written"
    );
    assert_eq!(read(&path), HAND_WRITTEN_AFTER_ONE_MODEL);
    assert_ne!(
        std::fs::metadata(&path).expect("the file").ino(),
        std::fs::metadata(&witness).expect("the witness").ino(),
        "the path must name a file the save landed whole, not the one it edited"
    );
    assert_eq!(
        names_in(&home.path().join("sindri")),
        ["config.toml"],
        "the staged file is gone once the rename has landed"
    );
}

#[test]
fn a_save_that_cannot_stage_leaves_the_old_config_untouched() {
    let home = config_home(HAND_WRITTEN);
    let directory = home.path().join("sindri");
    let path = file_in(&home);
    let before = mode(&path);
    std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o555))
        .expect("the directory closed to writes");

    let outcome = Config::save_to(
        &path,
        &Flags {
            model: Some(String::from("new-model")),
            ..Flags::default()
        },
    );
    let untouched = read(&path);
    let after = mode(&path);
    std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o755))
        .expect("the directory opened again");

    let error = outcome.expect_err("a save that cannot stage its temp file must fail");
    assert_eq!(error.exit_code(), 2);
    assert!(
        error.to_string().contains(&path.display().to_string()),
        "the error must name the file, it says: {error}"
    );
    assert_eq!(untouched, HAND_WRITTEN, "the old file is untouched");
    assert_eq!(after, before, "and so are its permissions");
}

#[test]
fn a_save_whose_rename_cannot_land_discards_what_it_staged() {
    let home = bare_home();
    let directory = home.path().join("sindri");
    std::fs::create_dir_all(&directory).expect("the sindri directory");
    // A rename cannot land on a directory that already holds something, which is
    // the one failure a test can force *after* the new bytes are staged.
    let path = directory.join("config.toml");
    std::fs::create_dir(&path).expect("a directory where the file would be");
    std::fs::write(path.join("mine.toml"), HAND_WRITTEN).expect("the config that was there");

    let outcome = Config::save_to(
        &path,
        &Flags {
            model: Some(String::from("new-model")),
            ..Flags::default()
        },
    );

    let error = outcome.expect_err("a rename onto a filled directory cannot land");
    assert_eq!(error.exit_code(), 2);
    assert!(
        error.to_string().contains(&path.display().to_string()),
        "the error must name the file, it says: {error}"
    );
    assert_eq!(
        read(&path.join("mine.toml")),
        HAND_WRITTEN,
        "what was at the destination is still there, byte for byte"
    );
    assert_eq!(
        names_in(&directory),
        ["config.toml"],
        "the staged file is dropped rather than left behind as litter"
    );
}

#[test]
fn a_file_that_is_not_toml_is_never_rewritten() {
    let broken = "model = \n";
    let home = config_home(broken);
    let path = file_in(&home);

    let error = Config::save_to(
        &path,
        &Flags {
            max_turns: Some(10),
            ..Flags::default()
        },
    )
    .expect_err("a file that will not parse cannot be patched");

    assert_eq!(error.exit_code(), 2);
    assert!(
        error.to_string().contains(&path.display().to_string()),
        "the error must name the file, it says: {error}"
    );
    assert_eq!(
        read(&path),
        broken,
        "a file it cannot read is never clobbered"
    );
}

/// Every key in the schema, set at once, as the save is handed them.
fn every_key_touched() -> Flags {
    Flags {
        provider: Some(Wire::Anthropic),
        model: Some(String::from("claude-sonnet-4-5")),
        route: Some(Route::Responses),
        base_url: Some(String::from("https://gateway.test/v1")),
        api_key: Some(String::from("sk-not-a-real-key")),
        thinking: Some(true),
        extra_writable_paths: Some(vec![PathBuf::from("/srv/data")]),
        extra_headers: Some(vec![String::from("X-Gateway-Session: {session}")]),
        context_window: Some(200_000),
        max_turns: Some(250),
        timeout: Some(120),
        command_budget_ms: Some(3_600_000),
        output_budget_bytes: Some(8_388_608),
        status: Some(true),
        ssh_agent: Some(false),
    }
}

#[test]
fn a_save_writes_the_keys_the_schema_has_and_invents_none() {
    let home = bare_home();
    let path = file_in(&home);
    let touched = every_key_touched();

    Config::save_to(&path, &touched).expect("the save lands");

    let text = read(&path);
    let document: toml::Table = text.parse().expect("the file it wrote is TOML");
    let mut written: Vec<&str> = document.keys().map(String::as_str).collect();
    written.sort_unstable();
    let mut schema: Vec<&str> = Config::KEYS.to_vec();
    schema.sort_unstable();

    assert_eq!(
        written, schema,
        "the save writes exactly the schema's keys, and the file is:\n{text}"
    );
    assert!(
        !text.contains("yolo"),
        "the escape hatch is not a key, so no save can persist it:\n{text}"
    );
    assert_eq!(
        Config::load_from(&path).expect("the file still reads"),
        Config::from(touched),
        "every key written is the key the save was handed"
    );
}

/// The working directory a test stands in, put back when the test ends, so a
/// repo-local `.sindri.toml` can be planted under the process's own feet.
struct Standing {
    /// Where the process stood before, which is where it is put back.
    previous: PathBuf,
}

impl Standing {
    fn in_directory(directory: &Path) -> Standing {
        let previous = std::env::current_dir().expect("a working directory");
        std::env::set_current_dir(directory).expect("a working directory to stand in");
        Standing { previous }
    }
}

impl Drop for Standing {
    fn drop(&mut self) {
        std::env::set_current_dir(&self.previous).expect("the working directory put back");
    }
}

#[test]
fn a_sindri_toml_in_the_working_directory_is_never_written() {
    let workspace = tempfile::tempdir().expect("a temp workspace");
    let planted = workspace.path().join(".sindri.toml");
    let repo_local = "extra_writable_paths = [\"/etc\"]\n";
    std::fs::write(&planted, repo_local).expect("the repo-local config");
    let home = bare_home();
    let environment: BTreeMap<String, String> = [(
        String::from("XDG_CONFIG_HOME"),
        home.path().display().to_string(),
    )]
    .into_iter()
    .collect();

    {
        let _standing = Standing::in_directory(workspace.path());
        Config::save(
            &environment,
            &Flags {
                max_turns: Some(10),
                ..Flags::default()
            },
        )
        .expect("the save lands");
    }

    assert_eq!(
        read(&planted),
        repo_local,
        "a checked-in config is neither read nor written, whatever a save is handed"
    );
    assert_eq!(
        read(&file_in(&home)),
        "max_turns = 10\n",
        "the save went to the one file the environment points at"
    );
}

#[test]
fn a_save_writes_the_file_the_environment_points_at() {
    let home = bare_home();
    let environment: BTreeMap<String, String> = [(
        String::from("XDG_CONFIG_HOME"),
        home.path().display().to_string(),
    )]
    .into_iter()
    .collect();

    Config::save(
        &environment,
        &Flags {
            model: Some(String::from("claude-sonnet-4-5")),
            ..Flags::default()
        },
    )
    .expect("the save lands");

    assert_eq!(
        read(&file_in(&home)),
        "model = \"claude-sonnet-4-5\"\n",
        "the save goes to the one file the read path uses"
    );
}

/// The keys a settings screen touched, and the text a reader left each row holding: the
/// patch `Enter` hands the one save.
fn touched(pairs: &[(&'static str, &str)]) -> Touched {
    let mut touched = Touched::new();
    for (key, text) in pairs {
        touched.set(key, *text);
    }
    touched
}

#[test]
fn a_settings_save_rewrites_the_one_line_a_reader_touched_and_no_other_byte() {
    let home = config_home(HAND_WRITTEN);
    let path = file_in(&home);

    Config::save_touched_to(&path, &touched(&[("model", "new-model")])).expect("the save lands");

    assert_eq!(
        read(&path),
        HAND_WRITTEN_AFTER_ONE_MODEL,
        "a visit that touched one row writes one line: the comments, the blank line and \
         every key the reader never reached are the bytes they were"
    );
}

#[test]
fn a_context_window_a_reader_types_is_written_as_a_number_the_file_reads_back() {
    let home = bare_home();
    let path = file_in(&home);

    Config::save_touched_to(&path, &touched(&[("context_window", "200000")]))
        .expect("the save lands");

    assert!(
        read(&path).contains("context_window = 200000"),
        "a window is a count, so the file holds it as a number and not as a string: {}",
        read(&path)
    );
    assert_eq!(
        Config::load_from(&path)
            .expect("the file still reads")
            .context_window,
        Some(200_000),
        "the number the row held is the number the next read takes back"
    );
}

#[test]
fn a_context_window_of_nothing_a_reader_types_is_refused_naming_the_row() {
    let home = bare_home();
    let path = file_in(&home);

    let error = Config::save_touched_to(&path, &touched(&[("context_window", "0")]))
        .expect_err("a window of no tokens is a share nothing can be taken of");

    let message = error.to_string();
    assert!(
        message.contains("context_window") && message.contains("at least one token"),
        "the refusal names the row a reader has to go and fix, and it says: {message}"
    );
    assert!(
        !path.exists(),
        "a refused save stages nothing, so there is no file to read"
    );
}

#[test]
fn a_value_a_screen_refused_names_the_key_and_leaves_the_file_alone() {
    let home = config_home(HAND_WRITTEN);
    let path = file_in(&home);

    let error = Config::save_touched_to(
        &path,
        &touched(&[("model", "new-model"), ("max_turns", "two hundred")]),
    )
    .expect_err("a turn cap that is not a number is a bad value");

    assert!(
        error.to_string().contains("max_turns"),
        "a refusal names the key a reader has to go and fix, and it says: {error}"
    );
    assert_eq!(
        read(&path),
        HAND_WRITTEN,
        "a refused save writes nothing at all — not the bad key and not the good one \
         beside it"
    );
}

#[test]
fn an_emptied_row_removes_the_key_so_the_environment_speaks() {
    let home = config_home(HAND_WRITTEN);
    let path = file_in(&home);

    Config::save_touched_to(&path, &touched(&[("model", "")])).expect("the save lands");

    let after = read(&path);
    assert!(
        !after.contains("model"),
        "an emptied row unsets the key rather than blanking it, and the file is:\n{after}"
    );
    assert_eq!(
        after,
        HAND_WRITTEN.replace(
            "# swapped a few times a week\nmodel = \"old-model\"  # note beside it\n",
            ""
        ),
        "the key leaves, and the note about it goes with it because it is the key's own \
         decor: every other line is the byte it was"
    );
}

#[test]
fn a_screens_save_lands_owner_only_and_leaves_no_temp_file_behind() {
    let home = bare_home();
    let path = file_in(&home);

    Config::save_touched_to(&path, &touched(&[("model", "claude-sonnet-4-5")]))
        .expect("the save lands");

    assert_eq!(
        mode(&path),
        sindri_core::config::MODE,
        "one of the keys a screen can write is a secret, so the file is owner-only"
    );
    assert_eq!(
        names_in(path.parent().expect("a directory")),
        vec![String::from("config.toml")],
        "the save is one rename into place: a staged temp file left behind would be a \
         second copy of what the file holds"
    );
}

#[test]
fn every_key_a_screen_shows_is_the_text_a_save_of_it_reads_back() {
    let home = bare_home();
    let path = file_in(&home);
    let written = every_key_touched();
    let shown = Config::from(written.clone());

    // The one key a screen may not show: a secret is masked wherever it is drawn,
    // so its row is a mask and never the text a file would hold.
    let mut readers = Vec::new();
    for key in Config::KEYS {
        if key != "api_key" {
            readers.push((key, shown.text(key)));
        }
    }
    let mut patch = Touched::new();
    for (key, text) in &readers {
        patch.set(key, text.as_str());
    }

    Config::save_touched_to(&path, &patch).expect("the save lands");

    assert_eq!(
        Config::load_from(&path).expect("the file still reads"),
        Config {
            api_key: None,
            ..Config::from(written)
        },
        "the text a row shows is the text a save of that row reads back, so a visit that \
         touched every row hands the file what the screen said it held"
    );
    assert_eq!(
        shown.text("api_key"),
        String::from(sindri_core::redact::MASK),
        "a secret is never in a frame: the row says a key is set and not which one"
    );
}
