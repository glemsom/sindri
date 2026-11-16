use super::*;

#[test]
fn the_context_window_flag_is_a_count_of_tokens_the_run_is_handed() {
    let cli = Cli::try_parse_from(["sindri", "--context-window", "200000"])
        .expect("a window of tokens is a flag this parser holds");

    assert_eq!(
        cli.flags().context_window,
        Some(200_000),
        "a window named on the command line is read above the file and the environment, \
             which is how every other run dial gets in"
    );
}

#[test]
fn a_context_window_of_nothing_on_the_command_line_is_refused() {
    let error = Cli::try_parse_from(["sindri", "--context-window", "0"])
        .expect_err("a window of no tokens is a share nothing can be taken of");

    assert_eq!(
        error.exit_code(),
        2,
        "a value that will not do is a usage error"
    );
    assert!(
        error.to_string().contains("at least one token"),
        "the refusal says the smallest window that would do, and it says: {error}"
    );
}
