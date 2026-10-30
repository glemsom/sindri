#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken contract reports itself"
)]

//! The sandbox policy's contract: what the writable set accepts, what the argv
//! enforces, and that enforcement and description are the same value twice.

use std::path::{Path, PathBuf};

use sindri_core::sandbox::{
    Enforcement, Invocation, NetworkPosture, PolicyError, SandboxPolicy, WritableSet,
};
use tempfile::TempDir;

fn policy(workspace: &TempDir, session_temp: &Path) -> SandboxPolicy {
    let writable = WritableSet::new([workspace.path().to_path_buf()]).expect("writable set");
    SandboxPolicy::new(
        writable,
        session_temp.to_path_buf(),
        "/usr/bin:/bin".to_string(),
        NetworkPosture::Shared,
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

#[test]
fn the_argv_mounts_the_read_only_root_first_then_writable_paths_then_dev() {
    let (workspace, session_temp, policy) = fixtures();
    let Invocation::Sandboxed(argv) = policy.to_argv(&["true".to_string()], None) else {
        panic!("a policy without --yolo-unsafe must be sandboxed");
    };

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
    let Invocation::Sandboxed(argv) = policy.to_argv(&["true".to_string()], None) else {
        panic!("sandboxed");
    };
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
    let Invocation::Sandboxed(argv) = policy.to_argv(&["true".to_string()], None) else {
        panic!("sandboxed");
    };
    let value = |key: &str| -> String {
        let at = argv
            .iter()
            .position(|arg| arg == key)
            .unwrap_or_else(|| panic!("{key} is set"));
        argv[at + 1].clone()
    };

    let temp = session_temp.display().to_string();
    for key in ["TMPDIR", "TEMP", "TMP"] {
        assert_eq!(value(key), temp, "{key} is the session temp");
    }
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
fn the_argv_unshares_the_namespaces_that_matter_and_keeps_the_network() {
    let (_, _, policy) = fixtures();
    let Invocation::Sandboxed(argv) = policy.to_argv(&["true".to_string()], None) else {
        panic!("sandboxed");
    };
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
    let Invocation::Sandboxed(argv) = policy.to_argv(&["true".to_string()], None) else {
        panic!("sandboxed");
    };

    let parsed: Vec<PathBuf> = binds(&argv)
        .into_iter()
        .map(PathBuf::from)
        .filter(|path| path != &session_temp)
        .collect();

    let round_tripped = SandboxPolicy::new(
        WritableSet::new(parsed).expect("the argv names a legal writable set"),
        session_temp.clone(),
        policy.search_path().to_string(),
        policy.network(),
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
fn an_unsafe_policy_wraps_nothing_and_says_so() {
    let (_workspace, session_temp, _) = fixtures();
    let writable =
        WritableSet::new([std::env::temp_dir().canonicalize().expect("temp")]).expect("writable");
    let unsafe_policy = SandboxPolicy::new(
        writable,
        session_temp,
        "/usr/bin:/bin".to_string(),
        NetworkPosture::Shared,
        Enforcement::YoloUnsafe,
    )
    .expect("policy");

    assert_eq!(
        unsafe_policy.to_argv(&["echo".to_string(), "hi".to_string()], None),
        Invocation::Unsandboxed(vec!["echo".to_string(), "hi".to_string()]),
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
    let Invocation::Sandboxed(argv) = policy.to_argv(&["true".to_string()], None) else {
        panic!("sandboxed");
    };
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
