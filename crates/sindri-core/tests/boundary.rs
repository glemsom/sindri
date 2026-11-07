//! Proves the boundary is one value whose views cannot disagree: the `bash` tool
//! description the model is given, the bytes `doctor` prints, the policy block a
//! session record stores and the fact the rail draws are all read off it, and
//! the rendered bytes are the same on every route and every run.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use sindri_core::config::{Config, Flags};
use sindri_core::policy::{Boundary, BoundaryError, Enforcement, MAX_TIMEOUT_SECONDS, WritableSet};
use tempfile::TempDir;

/// An environment a test can hold as data, instead of mutating the process's.
fn environment(vars: &[(&str, &str)]) -> BTreeMap<String, String> {
    vars.iter()
        .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
        .collect()
}

/// A config home holding one `config.toml` with `text` in it.
fn config_home(text: &str) -> TempDir {
    let home = tempfile::tempdir().expect("a temp dir");
    let directory = home.path().join("sindri");
    std::fs::create_dir_all(&directory).expect("the sindri directory");
    std::fs::write(directory.join("config.toml"), text).expect("the config file");
    home
}

/// The config one precedence resolves to: file, then environment, then flags.
fn resolved(home: &Path, vars: &[(&str, &str)], flags: Flags) -> Config {
    let mut environment = environment(vars);
    environment.insert(String::from("XDG_CONFIG_HOME"), home.display().to_string());
    Config::resolve(&environment, flags).expect("the config resolves")
}

/// Real directories on disk, because a canonical path is resolved through the
/// filesystem rather than tidied as a string: a set of paths that do not exist
/// is refused, so a test that wants one has to make it.
struct Ground {
    /// Held so the directories outlive the ground that names them.
    _root: TempDir,
    /// The same directory, as the filesystem spells it.
    path: PathBuf,
}

impl Ground {
    fn new() -> Ground {
        let root = tempfile::tempdir().expect("a temp ground");
        let path = std::fs::canonicalize(root.path()).expect("the ground resolves");
        Ground { _root: root, path }
    }

    /// A directory under the ground, created and returned canonical.
    fn directory(&self, name: &str) -> PathBuf {
        let directory = self.path.join(name);
        std::fs::create_dir_all(&directory).expect("the directory");
        std::fs::canonicalize(&directory).expect("the directory resolves")
    }

    /// The workspace a run stands in.
    fn workspace(&self) -> PathBuf {
        self.directory("workspace")
    }
}

/// The boundary a run holds, from the config in force and the workspace it
/// stands in.
fn boundary(config: &Config, workspace: &Path, enforcement: Enforcement) -> Boundary {
    Boundary::from_config(config, workspace, enforcement).expect("the boundary holds")
}

#[test]
fn doctor_prints_the_bytes_the_model_was_given() {
    let ground = Ground::new();
    let data = ground.directory("data");
    let home = config_home(&format!(
        "extra_writable_paths = [\"{}\"]\n",
        data.display()
    ));
    let workspace = ground.workspace();
    let boundary = boundary(
        &resolved(home.path(), &[], Flags::default()),
        &workspace,
        Enforcement::Bubblewrap,
    );

    let given = boundary.description();
    assert!(
        given.contains(&data.display().to_string()) && given.contains("120s"),
        "the description is the boundary, not an empty string: {given}"
    );

    assert_eq!(
        boundary.doctor_description().as_bytes(),
        given.as_bytes(),
        "doctor prints the description the model sees, byte for byte"
    );
}

#[test]
fn the_description_names_every_writable_path_the_timeout_and_the_budgets() {
    let ground = Ground::new();
    let data = ground.directory("data");
    let workspace = ground.workspace();
    let home = config_home(&format!(
        "extra_writable_paths = [\"{}\"]\n",
        data.display()
    ));
    let boundary = boundary(
        &resolved(home.path(), &[], Flags::default()),
        &workspace,
        Enforcement::Bubblewrap,
    );

    // The worked example, written out: an independent statement of what the
    // model is told, so the renderer cannot drift into saying something else.
    let paths = format!("  {}\n  {}", data.display(), workspace.display());
    let expected = format!(
        "Run a command with `bash`.\n\n\
         Commands run in unprivileged bubblewrap: the filesystem is read-only except the writable paths below.\n\n\
         {paths}\n\n\
         The per-call timeout is 120s; a longer call is killed and reports `timed_out`.\n\
         The run's budgets are 3600000ms of command wall clock and 8388608 bytes of command output."
    );

    assert_eq!(boundary.description(), expected);
}

#[test]
fn the_configured_limits_are_the_ones_the_description_names() {
    let ground = Ground::new();
    let workspace = ground.workspace();
    let home =
        config_home("timeout = 300\ncommand_budget_ms = 60000\noutput_budget_bytes = 1024\n");
    let boundary = boundary(
        &resolved(home.path(), &[], Flags::default()),
        &workspace,
        Enforcement::Bubblewrap,
    );

    let description = boundary.description();

    assert!(
        description.contains("300s")
            && description.contains("60000ms")
            && description.contains("1024 bytes"),
        "the description is the policy in force, not the defaults: {description}"
    );
    assert!(
        !description.contains("120s"),
        "a configured timeout must not be rendered as the default: {description}"
    );
}

#[test]
fn the_description_is_byte_identical_every_time_it_is_rendered() {
    let ground = Ground::new();
    let data = ground.directory("data");
    let workspace = ground.workspace();
    let home = config_home(&format!(
        "extra_writable_paths = [\"{}\"]\n",
        data.display()
    ));
    let config = resolved(home.path(), &[], Flags::default());

    let once = boundary(&config, &workspace, Enforcement::Bubblewrap);
    let twice = boundary(&config, &workspace, Enforcement::Bubblewrap);

    assert_eq!(
        once.description().as_bytes(),
        once.description().as_bytes(),
        "one boundary renders one string, however often it is asked"
    );
    assert_eq!(
        once.description().as_bytes(),
        twice.description().as_bytes(),
        "two runs of one policy render one string, byte for byte"
    );
}

#[test]
fn the_same_boundary_reached_by_file_environment_or_flags_renders_identical_bytes() {
    let ground = Ground::new();
    let data = ground.directory("data");
    let logs = ground.directory("logs");
    let workspace = ground.workspace();
    let (data, logs) = (data.display().to_string(), logs.display().to_string());

    // The same two extra paths, spelled through each route in the precedence,
    // and in a different order each time: the set is sorted, so the order the
    // route happened to name them in never reaches the model.
    let through_file = boundary(
        &resolved(
            config_home(&format!(
                "extra_writable_paths = [\"{data}\", \"{logs}\"]\n"
            ))
            .path(),
            &[],
            Flags::default(),
        ),
        &workspace,
        Enforcement::Bubblewrap,
    );

    let through_environment = boundary(
        &resolved(
            config_home("").path(),
            &[("SINDRI_EXTRA_WRITABLE_PATHS", &format!("{logs}:{data}"))],
            Flags::default(),
        ),
        &workspace,
        Enforcement::Bubblewrap,
    );

    let through_flags = boundary(
        &resolved(
            config_home("").path(),
            &[],
            Flags {
                extra_writable_paths: Some(vec![PathBuf::from(&data), PathBuf::from(&logs)]),
                ..Flags::default()
            },
        ),
        &workspace,
        Enforcement::Bubblewrap,
    );

    assert_eq!(through_file, through_environment);
    assert_eq!(through_file, through_flags);
    assert_eq!(
        through_file.description().as_bytes(),
        through_environment.description().as_bytes(),
        "a boundary reached through the environment renders the file's bytes"
    );
    assert_eq!(
        through_file.description().as_bytes(),
        through_flags.description().as_bytes(),
        "a boundary reached through flags renders the file's bytes"
    );
}

#[test]
fn the_record_stores_the_description_the_model_was_given() {
    let ground = Ground::new();
    let data = ground.directory("data");
    let workspace = ground.workspace();
    let home = config_home(&format!(
        "extra_writable_paths = [\"{}\"]\n",
        data.display()
    ));
    let boundary = boundary(
        &resolved(home.path(), &[], Flags::default()),
        &workspace,
        Enforcement::Bubblewrap,
    );

    let record = boundary.record_policy();
    let json = serde_json::to_value(&record).expect("the record serialises");

    assert_eq!(
        record.description.as_bytes(),
        boundary.description().as_bytes(),
        "the record's policy block holds the bytes the model was given"
    );
    assert_eq!(
        json["description"],
        serde_json::Value::from(boundary.description()),
        "and it holds them through a serialise, which is how #18 writes them"
    );
    assert_eq!(json["enforcement"], serde_json::Value::from("bubblewrap"));
    assert_eq!(
        json["writable"],
        serde_json::json!([data.display().to_string(), workspace.display().to_string()])
    );
    assert_eq!(json["timeout_seconds"], serde_json::Value::from(120));
    assert_eq!(
        json["command_budget_ms"],
        serde_json::Value::from(3_600_000u64)
    );
    assert_eq!(
        json["output_budget_bytes"],
        serde_json::Value::from(8_388_608u64)
    );
}

#[test]
fn enforcement_is_two_values_each_with_the_words_its_surfaces_use() {
    for (enforcement, word, rail) in [
        (Enforcement::Bubblewrap, "bubblewrap", "bwrap"),
        (Enforcement::Unsafe, "unsafe", "unsafe"),
    ] {
        assert_eq!(enforcement.word(), word);
        assert_eq!(enforcement.to_string(), word);
        assert_eq!(enforcement.rail_label(), rail);
        assert_eq!(
            serde_json::to_value(enforcement).expect("the enforcement serialises"),
            serde_json::Value::from(word),
            "the record and the JSON use the spec's word"
        );
    }
}

#[test]
fn unsafe_is_reachable_by_invocation_only_and_says_so() {
    let ground = Ground::new();
    let workspace = ground.workspace();

    assert!(
        !Config::KEYS.iter().any(|key| key.contains("yolo")),
        "the escape hatch cannot be persisted, so it is not a key"
    );

    let home = config_home("yolo_unsafe = true\n");
    let mut environment = environment(&[]);
    environment.insert(
        String::from("XDG_CONFIG_HOME"),
        home.path().display().to_string(),
    );
    assert!(
        Config::resolve(&environment, Flags::default()).is_err(),
        "a file cannot grant the escape hatch"
    );

    let nothing = resolved(
        config_home("").path(),
        &[("SINDRI_YOLO_UNSAFE", "true")],
        Flags::default(),
    );
    assert_eq!(
        nothing,
        Config::default(),
        "no variable can grant the escape hatch"
    );

    let boundary = boundary(&nothing, &workspace, Enforcement::Unsafe);
    let description = boundary.description();

    assert!(
        description.contains("Unsafe"),
        "an unsafe run says so to the model: {description}"
    );
    assert!(
        !description.contains("read-only"),
        "an unsafe run must not claim a confinement it does not have: {description}"
    );
    assert_eq!(
        boundary.record_policy().enforcement.word(),
        "unsafe",
        "the record says unsafe too"
    );
    assert_eq!(boundary.enforcement().rail_label(), "unsafe");
}

#[test]
fn a_relative_path_is_refused_naming_it() {
    let error: BoundaryError =
        WritableSet::new(["relative/data"]).expect_err("a relative path grants nothing");

    assert_eq!(
        error.to_string(),
        "writable path relative/data is not absolute"
    );
}

#[test]
fn the_root_directory_is_refused_naming_it() {
    let error = WritableSet::new(["/"]).expect_err("the root grants every path");

    assert!(
        error.to_string().contains("is the root directory"),
        "it says: {error}"
    );
}

#[test]
fn a_path_that_is_not_there_is_refused_naming_it() {
    let ground = Ground::new();
    let missing = ground.path.join("missing");

    let error =
        WritableSet::new([&missing]).expect_err("a directory that is not there grants nothing");

    assert!(
        error.to_string().contains(&missing.display().to_string()),
        "it names the path: {error}"
    );
}

#[test]
fn two_spellings_of_one_directory_are_one_directory_and_are_refused() {
    let ground = Ground::new();
    let data = ground.directory("data");
    let doubled = data.join("..").join("data");

    for spelling in [data.join("."), doubled] {
        let error =
            WritableSet::new([&data, &spelling]).expect_err("one directory twice is not a set");
        assert!(
            error.to_string().contains(&data.display().to_string())
                && error.to_string().contains("appears twice"),
            "it names the path that appeared twice: {error}"
        );
    }

    let link = ground.path.join("alias");
    std::os::unix::fs::symlink(&data, &link).expect("a symlink to the same directory");
    let error = WritableSet::new([&data, &link]).expect_err("a symlink is the same directory");
    assert!(
        error.to_string().contains("appears twice"),
        "a symlink spelling is resolved, not taken as a second path: {error}"
    );
}

#[test]
fn a_path_nested_inside_another_is_refused_naming_both() {
    let ground = Ground::new();
    let data = ground.directory("data");
    let cache = ground.directory("data/cache");

    // Both orders: the set is sorted, so a caller cannot slip a nested path in
    // by naming it first.
    for paths in [[&data, &cache], [&cache, &data]] {
        let error = WritableSet::new(paths).expect_err("a nested path grants nothing new");
        let message = error.to_string();
        assert!(
            message.contains(&cache.display().to_string())
                && message.contains(&data.display().to_string())
                && message.contains("is inside"),
            "it names the nested path and the one containing it: {message}"
        );
    }
}

#[test]
fn the_writable_set_holds_one_sorted_order_whatever_order_it_was_given_in() {
    let ground = Ground::new();
    let data = ground.directory("data");
    let logs = ground.directory("logs");

    let forwards = WritableSet::new([&data, &logs]).expect("two sibling directories");
    let backwards = WritableSet::new([&logs, &data]).expect("the same two, named the other way");

    assert_eq!(forwards, backwards);
    assert_eq!(forwards.paths(), [data, logs]);
}

#[test]
fn a_timeout_above_the_ceiling_is_refused_naming_the_limit() {
    assert_eq!(MAX_TIMEOUT_SECONDS, 3_600, "§4's ceiling is 3600s");

    let ground = Ground::new();
    let workspace = ground.workspace();
    let home = config_home("timeout = 3601\n");

    let error = Boundary::from_config(
        &resolved(home.path(), &[], Flags::default()),
        &workspace,
        Enforcement::Bubblewrap,
    )
    .expect_err("a timeout above the ceiling is refused, not clamped");

    assert!(
        error.to_string().contains("3601") && error.to_string().contains("3600"),
        "it names the timeout and the ceiling: {error}"
    );
}
