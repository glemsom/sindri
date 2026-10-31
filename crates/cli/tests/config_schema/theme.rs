//! The theme key, proven at the process seam: a name from the closed set runs,
//! and a name outside it is a usage error naming the value, the themes there
//! are and the file, wherever the name was set.

use sindri_core::config::{Document, Key};

use crate::common::provider::{MockProvider, Reply};
use crate::common::{Env, TEST_KEY, stderr, stdout, write_config};
use crate::run;

#[test]
fn a_theme_that_is_not_one_is_a_usage_error_naming_the_value_the_themes_and_the_file() {
    let env = Env::new();
    write_config(
        &env,
        r#"
model = "claude-from-file"
theme = "forge"
"#,
    );

    let mock = MockProvider::start(vec![Reply::answer("ok")]);
    let output = run(
        &env,
        &mock,
        &["-b", "summarise this"],
        &[("SINDRI_API_KEY", TEST_KEY)],
    );

    assert_eq!(
        output.status.code(),
        Some(2),
        "a theme that is not one is a usage error: {}",
        stderr(&output)
    );
    assert_eq!(stdout(&output), "", "a refused config writes no answer");
    let message = stderr(&output);
    assert!(
        message.contains("forge"),
        "the error names the value: {message}"
    );
    assert!(
        message.contains("anvil"),
        "the error names the themes there are: {message}"
    );
    assert!(
        message.contains("config.toml"),
        "the error names the file: {message}"
    );
    assert_eq!(
        mock.requests().len(),
        0,
        "a config that cannot be read sends nothing"
    );
}

#[test]
fn a_theme_that_is_one_runs_from_the_file_the_environment_and_a_flag() {
    let env = Env::new();
    write_config(
        &env,
        r#"
model = "claude-from-file"
theme = "anvil"
"#,
    );

    let mock = MockProvider::start(vec![Reply::answer("from the file")]);
    let output = run(
        &env,
        &mock,
        &["-b", "summarise this"],
        &[("SINDRI_API_KEY", TEST_KEY)],
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "the file's theme runs: {}",
        stderr(&output)
    );
    assert_eq!(stdout(&output), "from the file\n", "and the answer prints");

    let mock = MockProvider::start(vec![Reply::answer("from the environment")]);
    let output = run(
        &env,
        &mock,
        &["-b", "summarise this"],
        &[("SINDRI_API_KEY", TEST_KEY), ("SINDRI_THEME", "anvil")],
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "the environment's theme runs: {}",
        stderr(&output)
    );
    assert_eq!(stdout(&output), "from the environment\n");

    let mock = MockProvider::start(vec![Reply::answer("from the flag")]);
    let output = run(
        &env,
        &mock,
        &["-b", "summarise this", "--theme", "anvil"],
        &[("SINDRI_API_KEY", TEST_KEY)],
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "the flag's theme runs: {}",
        stderr(&output)
    );
    assert_eq!(stdout(&output), "from the flag\n");
}

#[test]
fn the_theme_the_settings_document_writes_is_the_one_the_next_run_reads() {
    let env = Env::new();
    write_config(&env, "model = \"claude-from-file\"\n");
    let path = env.config.join("sindri").join("config.toml");

    let mut document = Document::load_at(&path).expect("the config parses");
    document
        .set(Key::Theme, "anvil")
        .expect("the closed set's only name");
    document.save().expect("the file is written");

    let mock = MockProvider::start(vec![Reply::answer("ok")]);
    let output = run(&env, &mock, &["-b", "summarise this"], &[]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "the name the form wrote is one the run accepts: {}",
        stderr(&output)
    );
    assert_eq!(stdout(&output), "ok\n", "and the answer prints");
}

#[test]
fn the_name_the_form_refuses_never_reaches_the_file_the_run_reads() {
    let env = Env::new();
    write_config(&env, "model = \"claude-from-file\"\n");
    let path = env.config.join("sindri").join("config.toml");

    let mut document = Document::load_at(&path).expect("the config parses");
    let refused = document
        .set(Key::Theme, "forge")
        .expect_err("forge is not a theme");
    assert!(
        refused.message().contains("forge") && refused.message().contains("anvil"),
        "the refusal names the name and the themes there are: {}",
        refused.message()
    );
    document.save().expect("the file is written");

    let mock = MockProvider::start(vec![Reply::answer("ok")]);
    let output = run(&env, &mock, &["-b", "summarise this"], &[]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "the file holds no theme, so the run takes the default: {}",
        stderr(&output)
    );
}
