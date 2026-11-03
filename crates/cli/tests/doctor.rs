//! The process seam for `sindri doctor`: what a user sees before any model is
//! in the loop.

#![allow(
    clippy::expect_used,
    reason = "a panicking harness is how a process test reports a setup failure"
)]

mod common;

use std::path::PathBuf;

use common::{Env, path_without, required_binaries};
use predicates::prelude::*;
use tempfile::TempDir;

#[test]
fn doctor_prints_the_sandbox_description_and_exits_zero() {
    let env = Env::new();
    let assert = env.cmd().arg("doctor").assert().success();

    let output = assert.get_output();
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "",
        "doctor is silent on success"
    );

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(
        stdout.ends_with('\n'),
        "the description ends with a newline"
    );
    assert!(
        !stdout.ends_with("\n\n"),
        "exactly one trailing newline: {stdout:?}"
    );

    let workspace = env.canonical_workspace();
    let workspace = workspace.to_str().expect("utf-8 workspace");
    assert!(
        stdout.contains(workspace),
        "the workspace is in the writable set: {stdout}"
    );
    assert!(stdout.contains("writable set"), "{stdout}");
    assert!(stdout.contains("network: shared"), "{stdout}");
    assert!(
        stdout.contains("(TMPDIR)"),
        "the temp mapping is stated: {stdout}"
    );
    assert!(
        stdout.contains("home: "),
        "the sandbox HOME is stated: {stdout}"
    );
    assert!(
        stdout.contains(
            "read-only host root (readable, not writable), the writable set mounted read-write"
        ),
        "the writable set and the merely readable rest are one line: {stdout}"
    );
    assert_eq!(
        stdout.matches("read-only").count(),
        1,
        "the read-only boundary is stated once: {stdout}"
    );
}

#[test]
fn doctor_prints_the_description_and_the_catalog_and_nothing_else() {
    let env = Env::new();
    let output = env.cmd().arg("doctor").output().expect("run");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();

    // The binary ships a skill, so no machine has an empty catalog: what is
    // held here is that the catalog is the last thing printed and says one row
    // per skill in it.
    let shipped = sindri_core::skills::builtins();
    let (_, catalog) = stdout
        .split_once(&format!("skills: {} found\n", shipped.skills.len()))
        .unwrap_or_else(|| panic!("the catalog follows the description: {stdout}"));
    assert_eq!(
        catalog.lines().count(),
        shipped.skills.len(),
        "one row per skill and nothing after them: {stdout}"
    );

    // Nothing but the description and the catalog: every line is a known part
    // of one, so prose that is neither cannot leak onto stdout.
    for line in stdout.lines() {
        let known = line == "sandbox policy"
            || line.starts_with("skills: ")
            || line.starts_with("  builtin")
            || (line.starts_with("    /") && line.trim_start() == line.trim())
            || [
                "enforcement: ",
                "network: ",
                "writable set:",
                "session temp: ",
                "home: ",
                "environment: ",
                "isolation: ",
            ]
            .iter()
            .any(|prefix| line.trim_start().starts_with(prefix));
        assert!(known, "unexpected line in doctor output: {line:?}");
    }
}

fn hidden(omit: &[&str]) -> TempDir {
    path_without(&required_binaries(), omit, &common::host_path())
}

#[test]
fn doctor_exits_three_with_one_line_when_bwrap_is_missing() {
    let env = Env::new();
    let path = hidden(&["bwrap"]);
    env.cmd()
        .arg("doctor")
        .env("PATH", path.path())
        .assert()
        .code(3)
        .stdout(predicate::str::is_empty())
        .stderr(
            predicate::str::contains("bwrap")
                .and(predicate::str::contains("bubblewrap"))
                .and(predicate::function(|stderr: &str| {
                    stderr.lines().count() == 1
                })),
        );
}

#[test]
fn doctor_exits_three_with_one_line_when_a_required_binary_is_missing() {
    let env = Env::new();
    let path = hidden(&["yq"]);
    env.cmd()
        .arg("doctor")
        .env("PATH", path.path())
        .assert()
        .code(3)
        .stdout(predicate::str::is_empty())
        .stderr(
            predicate::str::contains("yq")
                .and(predicate::str::contains("sindri doctor"))
                .and(predicate::function(|stderr: &str| {
                    stderr.lines().count() == 1
                })),
        );
}

#[test]
fn doctor_exits_three_with_one_line_when_bwrap_cannot_create_a_sandbox() {
    let env = Env::new();
    let path = hidden(&["bwrap"]);
    let fake = path.path().join("bwrap");
    std::fs::write(
        &fake,
        "#!/bin/sh\necho 'setting up uid map: Permission denied' >&2\nexit 1\n",
    )
    .expect("write fake bwrap");
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).expect("chmod");

    env.cmd()
        .arg("doctor")
        .env("PATH", path.path())
        .assert()
        .code(3)
        .stdout(predicate::str::is_empty())
        .stderr(
            predicate::str::contains("Permission denied")
                .and(predicate::str::contains("user namespaces"))
                .and(predicate::function(|stderr: &str| {
                    stderr.lines().count() == 1
                })),
        );
}

#[test]
fn a_refused_repository_config_is_announced_by_doctor_once() {
    let env = Env::new();
    std::fs::write(
        env.workspace.join(".sindri.toml"),
        "extra_writable_paths = [\"/\"]\n",
    )
    .expect("a committed config");

    // The diagnostic reads the config through the same door a run does, so the
    // note a refused repository config earns is said here too — once, on the
    // channel every front end says it on, and never as part of the description.
    let assert = env.cmd().arg("doctor").assert().success();
    let output = assert.get_output();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert_eq!(
        stderr.matches(".sindri.toml").count(),
        1,
        "the refusal is said exactly once: {stderr}"
    );
    assert!(stderr.contains("ignor"), "and says what it did: {stderr}");
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("writable set"),
        "the description is still printed: {:?}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn a_config_a_run_would_refuse_is_refused_by_doctor_too() {
    let env = Env::new();
    common::write_config(&env, "max_turns = 0\n");

    env.cmd()
        .arg("doctor")
        .assert()
        .code(2)
        .stdout(predicate::str::is_empty())
        .stderr(
            predicate::str::contains("max_turns").and(predicate::function(|stderr: &str| {
                stderr.lines().count() == 1
            })),
        );
}

#[test]
fn doctor_describes_the_configured_extras_the_model_is_told_about() {
    let env = Env::new();
    let extra = env.workspace.parent().expect("root").join("extra");
    std::fs::create_dir_all(&extra).expect("extra dir");
    let extra = extra.canonicalize().expect("canonical extra");
    common::write_config(
        &env,
        &format!("extra_writable_paths = [\"{}\"]\n", extra.display()),
    );

    let assert = env.cmd().arg("doctor").assert().success();
    let output = String::from_utf8_lossy(&assert.get_output().stdout).to_string();

    assert!(
        output.contains(&extra.display().to_string()),
        "doctor prints the description a run would really give the model: {output}"
    );
}

/// The shipped floor, made in this test's own home: a fresh checkout with
/// nothing configured is the case the floor exists for.
fn floor(env: &Env) -> Vec<PathBuf> {
    let mut made = Vec::new();
    for name in sindri_core::sandbox::FLOOR {
        let path = env.home.join(name);
        std::fs::create_dir_all(&path).expect("a shipped path");
        made.push(path.canonicalize().expect("canonical shipped path"));
    }
    made
}

/// Whether the writable set the description printed names `path`: a mounted
/// path is a row of its own, indented under the only heading that names one.
fn lists(stdout: &str, path: &std::path::Path) -> bool {
    stdout
        .lines()
        .any(|line| line.trim() == path.display().to_string())
}

#[test]
fn doctor_lists_the_shipped_floor_when_nothing_names_a_writable_path() {
    let env = Env::new();
    let floor = floor(&env);

    let assert = env.cmd().arg("doctor").assert().success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).to_string();

    for path in &floor {
        assert!(
            lists(&stdout, path),
            "what is mounted is what is printed, and {} is mounted: {stdout}",
            path.display()
        );
    }
    assert!(
        lists(&stdout, &env.canonical_workspace()),
        "the workspace is still the first of them: {stdout}"
    );
}

#[test]
fn a_shipped_path_this_host_has_not_is_dropped_and_never_printed() {
    let env = Env::new();
    let kept = env.home.join(".cargo");
    std::fs::create_dir_all(&kept).expect("a shipped path");
    let kept = kept.canonicalize().expect("canonical shipped path");
    let absent = env.home.join("go");

    let assert = env.cmd().arg("doctor").assert().success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).to_string();

    assert!(lists(&stdout, &kept), "{stdout}");
    assert!(
        !stdout.contains(&absent.display().to_string()),
        "a convenience the host cannot keep is dropped without a word: {stdout}"
    );
}

#[test]
fn a_named_list_replaces_the_floor_and_doctor_lists_only_what_is_mounted() {
    let env = Env::new();
    let floor = floor(&env);

    // An explicit empty list is a list that was named: it means none.
    common::write_config(&env, "extra_writable_paths = []\n");
    let assert = env.cmd().arg("doctor").assert().success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).to_string();
    for path in &floor {
        assert!(
            !lists(&stdout, path),
            "an empty list mounts none of the floor, {} included: {stdout}",
            path.display()
        );
    }

    // And a named one is exactly what was named.
    let extra = TempDir::new().expect("an extra path");
    let extra = extra.path().canonicalize().expect("canonical extra");
    let assert = env
        .cmd()
        .args(["--writable", &extra.display().to_string(), "doctor"])
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).to_string();
    assert!(lists(&stdout, &extra), "{stdout}");
    for path in &floor {
        assert!(
            !lists(&stdout, path),
            "the flag replaces the floor rather than adding to it: {stdout}"
        );
    }
}

#[test]
fn doctor_with_the_flag_proves_no_sandbox_and_says_the_run_has_none() {
    let env = Env::new();
    // A machine where bwrap cannot even be found: the run will not use it, so
    // its absence is not this run's failure.
    let path = hidden(&["bwrap"]);
    let output = env
        .cmd()
        .args(["doctor", "--yolo-unsafe"])
        .env("PATH", path.path())
        .output()
        .expect("run");

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "",
        "doctor is silent on success"
    );
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(stdout.contains("enforcement: none"), "{stdout}");
    assert!(stdout.contains("your own privileges"), "{stdout}");
    assert!(
        !stdout.contains("writable set"),
        "nothing bounds the run, so nothing is claimed to: {stdout}"
    );
}

#[test]
fn doctor_with_the_flag_still_reports_a_missing_binary() {
    let env = Env::new();
    let path = hidden(&["yq"]);
    env.cmd()
        .args(["doctor", "--yolo-unsafe"])
        .env("PATH", path.path())
        .assert()
        .code(3)
        .stdout(predicate::str::is_empty())
        .stderr(
            predicate::str::contains("yq").and(predicate::function(|stderr: &str| {
                stderr.lines().count() == 1
            })),
        );
}

#[test]
fn the_flag_does_not_excuse_the_writable_set_it_no_longer_bounds() {
    let env = Env::new();
    // Beside the workspace, never inside it: an extra nested in a path already
    // granted is refused, flag or no flag.
    let extra = TempDir::new().expect("an extra path");
    let extra = extra.path().to_path_buf();

    // Granted and validated, though nothing about the run is bounded by it.
    let output = env
        .cmd()
        // `--writable` is not global: it comes before the subcommand.
        .args([
            "--writable",
            &extra.display().to_string(),
            "doctor",
            "--yolo-unsafe",
        ])
        .output()
        .expect("run");
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(
        !stdout.contains(&extra.display().to_string()),
        "the unsafe description claims no bound it does not enforce: {stdout}"
    );

    // A path that could not be mounted is a usage error under either policy.
    env.cmd()
        .args([
            "--writable",
            "/nonexistent-sindri-extra",
            "doctor",
            "--yolo-unsafe",
        ])
        .assert()
        .code(2)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("/nonexistent-sindri-extra"));
}

/// One skill directory, as a user or a repository would hold it.
fn write_skill(scope: &std::path::Path, name: &str, body: &str) -> std::path::PathBuf {
    let dir = scope.join(".agents/skills").join(name);
    std::fs::create_dir_all(&dir).expect("a skill directory");
    let file = dir.join("SKILL.md");
    std::fs::write(&file, body).expect("a SKILL.md");
    file
}

#[test]
fn doctor_lists_both_scopes_with_the_name_the_scope_and_who_may_invoke_it() {
    let env = Env::new();
    // A repository, so the repo scope has a root to stop at, with a skill
    // three directories down from it — which is where a run really boots.
    env.repository();
    write_skill(
        &env.workspace,
        "house-style",
        "---\nname: house-style\ndescription: Short sentences.\ndisable-model-invocation: true\n---\n",
    );
    write_skill(
        &env.home,
        "research",
        "---\nname: research\ndescription: Read primary sources.\n---\n",
    );

    let assert = env.cmd().arg("doctor").assert().success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).to_string();

    let shipped = sindri_core::skills::builtins().skills.len();
    let count = 2 + shipped;
    let report = stdout
        .split_once(&format!("skills: {count} found\n"))
        .expect("the catalog says how many it found")
        .1;
    let rows: Vec<&str> = report.lines().take(count).collect();
    assert!(
        rows[0].contains("repo") && rows[0].contains("house-style") && rows[0].contains("user"),
        "a repo skill the model may not see: {rows:?}"
    );
    assert!(
        rows[1].contains("user") && rows[1].contains("research") && rows[1].contains("model,user"),
        "a user skill both may invoke: {rows:?}"
    );
    assert!(
        rows[2].contains("builtin") && rows[2].contains("model,user"),
        "and the binary's own is the lowest of the three: {rows:?}"
    );
}

#[test]
fn doctor_prints_a_builtin_displaced_by_a_user_skill_of_its_name() {
    // Provenance is a field the doctor prints: the built-in holds its name in
    // every tree that carries no skill, and the moment a user drops one of the
    // same name in their home directory the row says `user` and the built-in is
    // named as the skill that lost — by its origin, since it has no path.
    let env = Env::new();
    env.repository();
    let builtin = sindri_core::skills::builtins()
        .skills
        .first()
        .map(|skill| skill.name.clone())
        .expect("the binary ships a built-in skill");
    write_skill(
        &env.home,
        &builtin,
        &format!("---\nname: {builtin}\ndescription: Ours, not the binary's.\n---\n"),
    );

    let assert = env.cmd().arg("doctor").assert().success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).to_string();

    let row = stdout
        .lines()
        .find(|line| line.contains(&builtin) && line.trim_start().starts_with("user"))
        .unwrap_or_else(|| panic!("the user's skill holds the name: {stdout}"));
    assert!(
        !row.contains("(compiled in)"),
        "the nearer scope's copy is the one listed: {row}"
    );
    assert!(
        stdout.contains(&format!("{builtin}: ")) && stdout.contains("shadows builtin"),
        "the built-in it displaced is named by its scope, not by a path it has not: {stdout}"
    );
}

#[test]
fn a_skill_that_will_not_load_is_named_with_its_file_and_line_and_is_not_a_failure() {
    let env = Env::new();
    env.repository();
    let file = write_skill(
        &env.workspace,
        "broken",
        "---\nname: broken\n---\n\n# Broken\n",
    );

    // A skill the scan refused is a diagnostic, not a broken environment: the
    // sandbox is still proven and doctor still exits zero.
    let assert = env.cmd().arg("doctor").assert().success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).to_string();

    assert!(
        stdout.contains(&format!(
            "skills: {} found",
            sindri_core::skills::builtins().skills.len()
        )),
        "a refused skill is a diagnostic, and the built-in is still found: {stdout}"
    );
    assert!(stdout.contains("skills: 1 refused"), "{stdout}");
    let at = format!("{}:1: ", file.display());
    assert!(
        stdout.contains(&at),
        "the file and the line are named — expected {at:?} in {stdout}"
    );
    assert!(stdout.contains("description"), "{stdout}");
}

#[test]
fn doctor_says_which_file_spent_too_much_description_and_drops_the_skill() {
    let env = Env::new();
    env.repository();
    let ceiling = sindri_core::skills::DESCRIPTION_CEILING;
    let file = write_skill(
        &env.workspace,
        "too-long",
        &format!(
            "---\nname: too-long\ndescription: {}\n---\n",
            "x".repeat(ceiling + 1)
        ),
    );

    // The process seam, not the catalog the report is written from: what a user
    // is handed is this line, at this file and this line, or the budget is only
    // a value a test happened to agree with.
    let assert = env.cmd().arg("doctor").assert().success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).to_string();

    let at = format!("{}:3: ", file.display());
    assert!(
        stdout.contains(&at),
        "the file and the line the key is on — expected {at:?} in {stdout}"
    );
    assert!(
        stdout.contains(&format!("{} characters", ceiling + 1))
            && stdout.contains(&format!("the {ceiling} a routing key may spend")),
        "the message says what the description measured and what the ceiling is: {stdout}"
    );
    assert!(
        stdout.contains(&format!(
            "skills: {} found",
            sindri_core::skills::builtins().skills.len()
        )),
        "a description past the ceiling is out of the catalog, not merely refused: {stdout}"
    );
}

/// The built-in the binary ships, and the row its catalog prints for it.
fn builtin_name() -> String {
    sindri_core::skills::builtins().skills[0].name.clone()
}

#[test]
fn doctor_prints_a_builtin_with_its_own_scope_word_and_no_path_it_has_not() {
    let env = Env::new();

    let assert = env.cmd().arg("doctor").assert().success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).to_string();

    let name = builtin_name();
    let (_, catalog) = stdout
        .split_once("skills: ")
        .expect("the catalog follows the description");
    let row = catalog
        .lines()
        .find(|line| line.contains(&name))
        .expect("the built-in is a row of the catalog");
    assert!(
        row.starts_with("  builtin"),
        "the third scope has its own word: {row}"
    );
    assert!(
        row.contains("(compiled in)"),
        "a compiled skill is marked where a scanned one's directory would go: {row}"
    );
    assert!(
        !row.contains('/'),
        "a skill with no directory is not printed by a path: {row}"
    );
}

#[test]
fn doctor_names_the_builtin_a_nearer_scope_displaced() {
    let env = Env::new();
    let name = builtin_name();
    write_skill(
        &env.home,
        &name,
        &format!("---\nname: {name}\ndescription: The team's own.\n---\n"),
    );

    let assert = env.cmd().arg("doctor").assert().success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).to_string();

    assert!(stdout.contains("skills: 1 shadowed"), "{stdout}");
    assert!(
        stdout.contains(&format!("{name}: ")) && stdout.contains("shadows builtin"),
        "what won is printed beside what it displaced, by origin: {stdout}"
    );
    let row = stdout
        .lines()
        .find(|line| line.contains(&name) && line.contains("user"))
        .expect("the nearer skill is the one in the catalog");
    assert!(
        row.starts_with("  user") && row.contains(".agents/skills"),
        "the user's directory holds the name: {row}"
    );
}
