//! The sandbox policy's contract: what the writable set accepts, what the argv
//! enforces, and that enforcement and description are the same value twice.

#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken contract reports itself"
)]

use std::path::{Path, PathBuf};

use sindri_core::sandbox::{Enforcement, Extras, FLOOR, PolicyError, SandboxPolicy, WritableSet};
use sindri_core::session::policy_for;
use tempfile::TempDir;

fn policy(workspace: &TempDir, session_temp: &Path) -> SandboxPolicy {
    let writable = WritableSet::new([workspace.path().to_path_buf()]).expect("writable set");
    SandboxPolicy::new(
        writable,
        session_temp.to_path_buf(),
        "/usr/bin:/bin".to_string(),
        Enforcement::Bubblewrap,
    )
    .expect("policy")
}

fn fixtures() -> (TempDir, PathBuf, SandboxPolicy) {
    let workspace = TempDir::new().expect("workspace");
    let session_temp = workspace.path().join("session/tmp");
    std::fs::create_dir_all(session_temp.join("home")).expect("session temp");
    let policy = policy(&workspace, &session_temp);
    (workspace, session_temp, policy)
}

#[test]
fn the_writable_set_rejects_root_relative_and_non_canonical_paths() {
    assert!(matches!(
        WritableSet::new([PathBuf::from("/")]),
        Err(PolicyError::Root)
    ));
    assert!(matches!(
        WritableSet::new([PathBuf::from("relative/path")]),
        Err(PolicyError::NotAbsolute { .. })
    ));

    let real = TempDir::new().expect("dir");
    let mut dotted = real.path().to_path_buf();
    dotted.push(".");
    assert!(
        matches!(
            WritableSet::new([dotted]),
            Err(PolicyError::NotCanonical { .. })
        ),
        "a path that is not already canonical must be refused"
    );

    let link = real.path().join("link");
    std::os::unix::fs::symlink(real.path(), &link).expect("symlink");
    assert!(
        matches!(
            WritableSet::new([link]),
            Err(PolicyError::NotCanonical { .. })
        ),
        "a symlink must be refused in favour of its target"
    );
}

#[test]
fn the_writable_set_rejects_nested_and_duplicate_paths() {
    let outer = TempDir::new().expect("outer");
    let inner = outer.path().join("inner");
    std::fs::create_dir_all(&inner).expect("inner");
    let error =
        WritableSet::new([outer.path().to_path_buf(), inner]).expect_err("nesting must be refused");
    assert!(matches!(error, PolicyError::Nested { .. }), "{error}");

    let same = outer.path().to_path_buf();
    let error = WritableSet::new([same.clone(), same]).expect_err("duplicates must be refused");
    assert!(matches!(error, PolicyError::Duplicate { .. }), "{error}");
}

/// A home directory holding the shipped paths named, and nothing else.
fn home_with(names: &[&str]) -> TempDir {
    let home = TempDir::new().expect("home");
    for name in names {
        std::fs::create_dir_all(home.path().join(name)).expect("a shipped path");
    }
    home
}

/// The floor as this host holds it: canonical, in the order `FLOOR` names it.
fn floor_of(home: &TempDir) -> Vec<PathBuf> {
    let home = home.path().canonicalize().expect("canonical home");
    FLOOR.iter().map(|name| home.join(name)).collect()
}

#[test]
fn nothing_named_mounts_the_floor_every_host_is_expected_to_have() {
    let home = home_with(&FLOOR);
    let workspace = TempDir::new().expect("workspace");
    assert_eq!(
        Extras::Floor.resolve(Some(home.path()), workspace.path()),
        floor_of(&home),
        "a run with nothing configured starts at the shipped floor"
    );
}

#[test]
fn a_shipped_path_the_host_has_not_is_dropped_without_a_word() {
    let home = home_with(&[".cargo"]);
    let workspace = TempDir::new().expect("workspace");
    let resolved = Extras::Floor.resolve(Some(home.path()), workspace.path());
    assert_eq!(
        resolved,
        vec![
            home.path()
                .canonicalize()
                .expect("canonical")
                .join(".cargo")
        ],
        "only the shipped path this host has is mounted"
    );
    assert!(
        Extras::Floor.resolve(None, workspace.path()).is_empty(),
        "a host with no home directory mounts none of them"
    );

    // A file where the floor expects a directory is not a mount: the convenience
    // yields to the host rather than being refused for it.
    let home = home_with(&[]);
    std::fs::write(home.path().join(".cache"), "not a directory").expect("a file");
    assert!(
        Extras::Floor
            .resolve(Some(home.path()), workspace.path())
            .is_empty()
    );
}

#[test]
fn a_shipped_path_that_nests_with_the_workspace_is_dropped_without_a_word() {
    let home = home_with(&[".cache", "go"]);
    let canonical = home.path().canonicalize().expect("canonical home");
    let workspace = canonical.join(".cache/checkout");
    std::fs::create_dir_all(&workspace).expect("a workspace inside the floor");
    assert_eq!(
        Extras::Floor.resolve(Some(home.path()), &workspace),
        vec![canonical.join("go")],
        "the shipped path that would nest is dropped, and the rest still mount"
    );
    assert_eq!(
        Extras::Floor.resolve(Some(home.path()), &canonical.join("go/checkout")),
        vec![canonical.join(".cache")],
        "and one that holds the workspace is dropped the same way"
    );
}

#[test]
fn a_named_list_replaces_the_floor_and_an_empty_one_means_none() {
    let home = home_with(&FLOOR);
    let workspace = TempDir::new().expect("workspace");
    let extra = TempDir::new().expect("extra");
    let extra = extra.path().canonicalize().expect("canonical extra");

    assert_eq!(
        Extras::Named(vec![extra.clone()]).resolve(Some(home.path()), workspace.path()),
        vec![extra],
        "a named list is exactly what was named, floor or no floor"
    );
    assert!(
        Extras::Named(Vec::new())
            .resolve(Some(home.path()), workspace.path())
            .is_empty(),
        "a list named empty means none"
    );
}

#[test]
fn the_policy_mounts_the_workspace_and_then_the_floor_it_resolves() {
    let workspace = TempDir::new().expect("workspace");
    let canonical = workspace
        .path()
        .canonicalize()
        .expect("canonical workspace");
    let session_temp = canonical.join("tmp");
    std::fs::create_dir_all(&session_temp).expect("session temp");

    let policy = policy_for(
        &canonical,
        &Extras::Floor,
        session_temp,
        "/usr/bin:/bin".to_string(),
        Enforcement::Bubblewrap,
    )
    .expect("a policy over the floor");
    let resolved = Extras::Floor.resolve(sindri_core::paths::home().as_deref(), &canonical);

    let paths = policy.writable().paths();
    assert_eq!(paths.first(), Some(&canonical), "the workspace comes first");
    assert_eq!(
        &paths[1..],
        resolved.as_slice(),
        "then exactly the floor this host has, and nothing the description would not print"
    );
    for path in &resolved {
        assert!(
            policy.describe().contains(&path.display().to_string()),
            "what is mounted is what is described: {}",
            path.display()
        );
    }
}

#[test]
fn a_named_path_the_writable_set_refuses_is_still_a_refusal() {
    let workspace = TempDir::new().expect("workspace");
    let canonical = workspace
        .path()
        .canonicalize()
        .expect("canonical workspace");
    let session_temp = canonical.join("tmp");
    std::fs::create_dir_all(&session_temp).expect("session temp");

    let missing = policy_for(
        &canonical,
        &Extras::Named(vec![PathBuf::from("/nonexistent-sindri-extra")]),
        session_temp.clone(),
        "/usr/bin:/bin".to_string(),
        Enforcement::Bubblewrap,
    )
    .expect_err("a named path that is not there is a usage error");
    assert!(
        matches!(missing, PolicyError::Unreadable { .. }),
        "{missing}"
    );

    let parent = canonical.parent().expect("a parent").to_path_buf();
    let nested = policy_for(
        &canonical,
        &Extras::Named(vec![parent]),
        session_temp,
        "/usr/bin:/bin".to_string(),
        Enforcement::Bubblewrap,
    )
    .expect_err("a named path that contains the workspace is a usage error");
    assert!(matches!(nested, PolicyError::Nested { .. }), "{nested}");
}

#[test]
fn the_argv_mounts_the_read_only_root_first_then_writable_paths_then_dev() {
    let (workspace, session_temp, policy) = fixtures();
    let argv = policy.to_argv(&["true".to_string()], None);

    assert_eq!(&argv[..4], ["bwrap", "--ro-bind", "/", "/"]);
    let workspace = workspace.path().display().to_string();
    let temp = session_temp.display().to_string();
    let binds = binds(&argv);
    assert_eq!(
        binds,
        vec![&workspace, &temp],
        "the writable set, then the session temp"
    );

    let dev = argv
        .iter()
        .position(|arg| arg == "--dev")
        .expect("device mount");
    assert_eq!(argv[dev + 1], "/dev");
    assert!(
        dev > argv.iter().position(|a| a == "--bind").expect("binds"),
        "the device mount comes after the writable binds"
    );
}

#[test]
fn the_argv_never_skips_a_bind_silently() {
    let (_, _, policy) = fixtures();
    let argv = policy.to_argv(&["true".to_string()], None);
    const SKIPPING: [&str; 3] = ["--bind-try", "--ro-bind-try", "--dev-bind-try"];
    for arg in &argv {
        assert!(
            !SKIPPING.contains(&arg.as_str()),
            "a silently skipping bind would let the description claim a mount that never happened: {arg}"
        );
    }
}

#[test]
fn the_argv_redirects_the_temp_variables_and_a_writable_home_into_the_session_temp() {
    let (_workspace, session_temp, policy) = fixtures();
    let argv = policy.to_argv(&["true".to_string()], None);
    let value = |key: &str| -> String {
        let at = argv
            .iter()
            .position(|arg| arg == key)
            .unwrap_or_else(|| panic!("{key} is set"));
        argv[at + 1].clone()
    };

    let temp = session_temp.display().to_string();
    assert_eq!(value("TMPDIR"), temp, "TMPDIR is the session temp");
    assert_eq!(
        value("HOME"),
        format!("{temp}/home"),
        "HOME lives beneath it"
    );
    assert!(
        session_temp.join("home").is_dir(),
        "HOME exists and is writable"
    );

    let cleared = argv
        .iter()
        .position(|a| a == "--clearenv")
        .expect("--clearenv");
    let home = argv.iter().position(|a| a == "HOME").expect("HOME");
    assert!(cleared < home, "the inherited environment is dropped first");
    assert!(
        session_temp.join("home").starts_with(&session_temp),
        "HOME is created inside the session temp, never the user's own"
    );
}

fn binds(argv: &[String]) -> Vec<&String> {
    argv.iter()
        .enumerate()
        .filter(|(i, _)| *i > 0 && argv[i - 1] == "--bind")
        .map(|(_, arg)| arg)
        .collect()
}

#[test]
fn the_description_names_every_variable_the_argv_sets() {
    let (_workspace, _, policy) = fixtures();
    let argv = policy.to_argv(&["true".to_string()], None);
    let set: Vec<&str> = argv
        .windows(3)
        .filter(|window| window[0] == "--setenv")
        .map(|window| window[1].as_str())
        .collect();
    assert!(!set.is_empty(), "the argv clears and then sets variables");

    let description = policy.describe();
    let line = description
        .lines()
        .find(|line| line.trim_start().starts_with("environment:"))
        .unwrap_or_else(|| panic!("the environment is stated: {description}"));
    // A name is matched whole: the line is split on everything that is not a
    // name character, so `PATH` in prose is not the `PATH` that is set.
    let named: Vec<&str> = line
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .collect();
    for name in set {
        assert!(
            named.contains(&name),
            "the description understates the boundary: {name} is set and not named: {line}"
        );
    }
}

#[test]
fn the_argv_unshares_the_namespaces_that_matter_and_keeps_the_network() {
    let (_, _, policy) = fixtures();
    let argv = policy.to_argv(&["true".to_string()], None);
    for flag in [
        "--unshare-pid",
        "--die-with-parent",
        "--new-session",
        "--share-net",
    ] {
        assert!(argv.iter().any(|arg| arg == flag), "{flag} is load-bearing");
    }
    assert!(
        !argv.iter().any(|arg| arg == "--unshare-net"),
        "network stays shared by default"
    );
}

#[test]
fn the_argv_parsed_back_into_a_writable_set_describes_the_same_sandbox() {
    let (_workspace, session_temp, policy) = fixtures();
    let argv = policy.to_argv(&["true".to_string()], None);

    let parsed: Vec<PathBuf> = binds(&argv)
        .into_iter()
        .map(PathBuf::from)
        .filter(|path| path != &session_temp)
        .collect();

    let round_tripped = SandboxPolicy::new(
        WritableSet::new(parsed).expect("the argv names a legal writable set"),
        session_temp.clone(),
        policy.search_path().to_string(),
        policy.enforcement(),
    )
    .expect("policy");

    assert_eq!(
        round_tripped.describe(),
        policy.describe(),
        "the description is a function of the writable set the argv enforces"
    );
}

#[test]
fn the_description_names_the_read_only_boundary_once() {
    let (_, _, policy) = fixtures();
    let description = policy.describe();
    let stated = description.matches("read-only").count();
    assert_eq!(
        stated, 1,
        "the read-only host root is stated once, not repeated: {description}"
    );
    assert!(
        description.contains(
            "read-only host root (readable, not writable), the writable set mounted read-write"
        ),
        "one line says what is writable and what is merely readable: {description}"
    );
}

#[test]
fn an_unsafe_policy_wraps_nothing_and_says_so() {
    let (_workspace, session_temp, _) = fixtures();
    let writable =
        WritableSet::new([std::env::temp_dir().canonicalize().expect("temp")]).expect("writable");
    let unsafe_policy = SandboxPolicy::new(
        writable,
        session_temp,
        "/usr/bin:/bin".to_string(),
        Enforcement::YoloUnsafe,
    )
    .expect("policy");

    assert_eq!(
        unsafe_policy.to_argv(&["echo".to_string(), "hi".to_string()], None),
        vec!["echo".to_string(), "hi".to_string()],
        "bwrap is bypassed entirely"
    );
    assert!(
        unsafe_policy.describe().contains("none"),
        "the description states that nothing is enforced: {}",
        unsafe_policy.describe()
    );
    assert!(
        unsafe_policy.describe().contains("your own privileges"),
        "{}",
        unsafe_policy.describe()
    );
}

#[test]
fn the_argv_mounts_a_fresh_procfs_so_the_pid_namespace_is_not_a_lie() {
    let (_workspace, _, policy) = fixtures();
    let argv = policy.to_argv(&["true".to_string()], None);
    let proc_mount = argv
        .iter()
        .position(|arg| arg == "--proc")
        .expect("a fresh procfs is mounted");
    assert_eq!(argv[proc_mount + 1], "/proc");
    assert!(
        proc_mount
            > argv
                .iter()
                .position(|arg| arg == "--ro-bind")
                .expect("root bind"),
        "the procfs mount follows the read-only root"
    );
}
