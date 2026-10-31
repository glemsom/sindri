//! The theme key, proven at the process seam: a name from the closed set runs,
//! and a name outside it is a usage error naming the value, the themes there
//! are and the file, wherever the name was set.

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
