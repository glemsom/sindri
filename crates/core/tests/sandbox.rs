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

/// The home directory a policy is given: the host's own, as a value. Nothing is
/// mounted for it, so it need not be a directory here.
const HOST_HOME: &str = "/home/tester";

/// The ssh agent socket a policy is given: the host's own, as a value. A socket
/// connects across the read-only root, so it is named and never mounted, and it
/// need not exist here either.
const HOST_AGENT: &str = "/run/user/1000/ssh-agent.socket";

fn policy(workspace: &TempDir, session_temp: &Path) -> SandboxPolicy {
    policy_homed(workspace, session_temp, Some(Path::new(HOST_HOME)))
}

fn policy_homed(workspace: &TempDir, session_temp: &Path, home: Option<&Path>) -> SandboxPolicy {
    let writable = WritableSet::new([workspace.path().to_path_buf()]).expect("writable set");
    SandboxPolicy::new(
        writable,
        session_temp.to_path_buf(),
        home.map(Path::to_path_buf),
        None,
        "/usr/bin:/bin".to_string(),
        Enforcement::Bubblewrap,
    )
    .expect("policy")
}

/// A policy that names the host's ssh agent socket, or none.
fn policy_naming_agent(
    workspace: &TempDir,
    session_temp: &Path,
    agent: Option<&Path>,
) -> SandboxPolicy {
    let writable = WritableSet::new([workspace.path().to_path_buf()]).expect("writable set");
    SandboxPolicy::new(
        writable,
        session_temp.to_path_buf(),
        Some(PathBuf::from(HOST_HOME)),
        agent.map(Path::to_path_buf),
        "/usr/bin:/bin".to_string(),
        Enforcement::Bubblewrap,
    )
    .expect("policy")
}

fn fixtures() -> (TempDir, PathBuf, SandboxPolicy) {
    let workspace = TempDir::new().expect("workspace");
    let session_temp = workspace.path().join("session/tmp");
    std::fs::create_dir_all(&session_temp).expect("session temp");
    let policy = policy(&workspace, &session_temp);
    (workspace, session_temp, policy)
}

/// The value the argv hands `key`, which the caller knows is set at all.
fn value_of(argv: &[String], key: &str) -> String {
    let at = argv
        .iter()
        .position(|arg| arg == key)
        .unwrap_or_else(|| panic!("{key} is set: {argv:?}"));
    argv[at + 1].clone()
}

/// The one line of the description that opens with `prefix`.
fn described_line(policy: &SandboxPolicy, prefix: &str) -> String {
    policy
        .describe()
        .lines()
        .find(|line| line.trim_start().starts_with(prefix))
        .map(str::to_string)
        .unwrap_or_else(|| panic!("{prefix} is stated: {}", policy.describe()))
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
        None,
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
        None,
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
        None,
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
fn the_argv_maps_the_temp_and_hands_over_the_host_home_unchanged() {
    let (_workspace, session_temp, policy) = fixtures();
    let argv = policy.to_argv(&["true".to_string()], None);

    assert_eq!(
        value_of(&argv, "TMPDIR"),
        session_temp.display().to_string(),
        "TMPDIR is the session temp"
    );
    assert_eq!(
        value_of(&argv, "HOME"),
        HOST_HOME,
        "HOME is the host's own, unchanged"
    );

    let cleared = argv
        .iter()
        .position(|a| a == "--clearenv")
        .expect("--clearenv");
    let home = argv.iter().position(|a| a == "HOME").expect("HOME");
    assert!(cleared < home, "the inherited environment is dropped first");

    // Nothing is made or mounted for the home: the read-only root already
    // carries it, so a write to `$HOME` fails loudly instead of landing in a
    // scratch directory nobody reads.
    for path in binds(&argv) {
        assert!(
            !Path::new(path).starts_with(HOST_HOME),
            "no bind is made for the home: {path}"
        );
    }
    assert!(
        !session_temp.join("home").exists(),
        "and no directory is created for it"
    );
}

#[test]
fn a_host_with_no_home_sets_no_home_variable_at_all() {
    let workspace = TempDir::new().expect("workspace");
    let session_temp = workspace.path().join("session/tmp");
    std::fs::create_dir_all(&session_temp).expect("session temp");
    let policy = policy_homed(&workspace, &session_temp, None);
    let argv = policy.to_argv(&["true".to_string()], None);

    assert!(
        !argv.iter().any(|arg| arg == "HOME"),
        "an absent home is absent, never an empty one: {argv:?}"
    );
    assert_eq!(
        value_of(&argv, "TMPDIR"),
        session_temp.display().to_string(),
        "the temp is mapped either way"
    );
}

#[test]
fn the_session_temp_bind_comes_after_the_read_only_root_so_scratch_stays_writable() {
    // With no `XDG_STATE_HOME` the session temp sits under the home, which is
    // read-only. The bind is what makes that scratch writable and mount order is
    // the whole of why it wins, so the order is pinned here rather than left to
    // be "fixed" by moving scratch out of the state directory.
    let workspace = TempDir::new().expect("workspace");
    let home = TempDir::new().expect("home");
    let session_temp = home.path().join("state/sindri/sessions/guid/tmp");
    std::fs::create_dir_all(&session_temp).expect("session temp");
    let policy = policy_homed(&workspace, &session_temp, Some(home.path()));
    let argv = policy.to_argv(&["true".to_string()], None);

    let root = argv
        .iter()
        .position(|arg| arg == "--ro-bind")
        .expect("the read-only root is mounted");
    let temp = session_temp.display().to_string();
    let scratch = argv
        .iter()
        .position(|arg| arg == &temp)
        .expect("the session temp is bound");
    assert!(
        root < scratch,
        "the writable bind must come after the read-only root: {argv:?}"
    );
    assert!(
        session_temp.starts_with(home.path()),
        "the temp this pins is the one nested inside the home"
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
fn the_description_says_the_home_is_the_hosts_own_and_read_only() {
    let (_, _, policy) = fixtures();
    let line = described_line(&policy, "home:");

    assert!(line.contains(HOST_HOME), "the home is named: {line}");
    assert!(line.contains("read-only"), "and stated unwritable: {line}");
    assert!(
        line.contains("$TMPDIR"),
        "and says where to write instead: {line}"
    );
}

#[test]
fn a_host_with_no_home_is_stated_as_a_fact_about_the_host() {
    let workspace = TempDir::new().expect("workspace");
    let session_temp = workspace.path().join("session/tmp");
    std::fs::create_dir_all(&session_temp).expect("session temp");
    let policy = policy_homed(&workspace, &session_temp, None);
    let line = described_line(&policy, "home:");

    assert!(
        line.contains("the host has no HOME"),
        "the absence is the host's: {line}"
    );
    assert!(
        !line.contains("sandbox"),
        "and the sandbox is not blamed for it: {line}"
    );
    assert!(
        line.contains("$TMPDIR"),
        "and says where to write instead: {line}"
    );
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
        policy.home().map(Path::to_path_buf),
        policy.ssh_agent().map(Path::to_path_buf),
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
    // Two and no more: the host root, and the home that sits inside it. Any
    // third is the boundary said again in different words.
    let stated = description.matches("read-only").count();
    assert_eq!(
        stated, 2,
        "the read-only host root and the home are each stated once: {description}"
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
        Some(PathBuf::from(HOST_HOME)),
        None,
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

#[test]
fn the_agent_socket_is_exported_and_the_variable_is_mounted_by_nothing() {
    let workspace = TempDir::new().expect("workspace");
    let session_temp = workspace.path().join("session/tmp");
    std::fs::create_dir_all(&session_temp).expect("session temp");
    let policy = policy_naming_agent(&workspace, &session_temp, Some(Path::new(HOST_AGENT)));

    let argv = policy.to_argv(&["true".to_string()], None);
    assert_eq!(
        value_of(&argv, "SSH_AUTH_SOCK"),
        HOST_AGENT,
        "the socket is the host's own, named exactly as it was read"
    );
    assert!(
        !binds(&argv).iter().any(|bind| *bind == HOST_AGENT),
        "and it needs no mount: a socket across the read-only root connects: {:?}",
        binds(&argv)
    );
}

#[test]
fn a_host_with_no_agent_hands_the_sandbox_no_variable_at_all() {
    let workspace = TempDir::new().expect("workspace");
    let session_temp = workspace.path().join("session/tmp");
    std::fs::create_dir_all(&session_temp).expect("session temp");
    let policy = policy_naming_agent(&workspace, &session_temp, None);

    let argv = policy.to_argv(&["true".to_string()], None);
    assert!(
        !argv.iter().any(|arg| arg == "SSH_AUTH_SOCK"),
        "the variable is absent rather than empty, so `ssh` falls back to what \
         it would do anywhere: {argv:?}"
    );
    assert!(
        !policy.describe().contains("SSH_AUTH_SOCK"),
        "the description names no variable the argv does not set: {}",
        policy.describe()
    );
}

#[test]
fn the_description_names_the_socket_and_gains_no_line_of_its_own() {
    let workspace = TempDir::new().expect("workspace");
    let session_temp = workspace.path().join("session/tmp");
    std::fs::create_dir_all(&session_temp).expect("session temp");
    let with = policy_naming_agent(&workspace, &session_temp, Some(Path::new(HOST_AGENT)));
    let without = policy_naming_agent(&workspace, &session_temp, None);

    // The environment list already publishes every name the policy sets, so the
    // socket arrives there: one name, and the description is otherwise the same
    // text. A dedicated line would be the model told about ssh.
    assert!(
        described_line(&with, "environment:").contains("SSH_AUTH_SOCK"),
        "the environment list names it: {}",
        with.describe()
    );
    let described = with.describe();
    let plain = without.describe();
    let difference: Vec<&str> = described
        .lines()
        .filter(|line| !plain.lines().any(|other| other == *line))
        .collect();
    assert_eq!(
        difference.len(),
        1,
        "one line differs — the environment list — and no line is added: {difference:?}"
    );
    assert!(
        difference[0].trim_start().starts_with("environment:"),
        "the difference is the list it already publishes: {difference:?}"
    );
    assert!(
        !with.describe().contains(HOST_AGENT),
        "and the socket's path is nowhere in it: the names are the fact, not a \
         new sentence: {}",
        with.describe()
    );
}
