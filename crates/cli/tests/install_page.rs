//! The install page and the version line the build prints: a reader quotes that
//! line in a bug report, so the arguments the page gives are the arguments the
//! build passes, read off the build script rather than trusted to keep up.

#![allow(
    clippy::panic,
    reason = "a panicking test is how a broken contract reports itself"
)]

/// The install page, as committed: `include_str!` so the test cannot pass against
/// a file it read from the wrong directory.
const INSTALL_PAGE: &str = include_str!("../../../docs/install.md");

/// The build script that cuts the version line: `include_str!` so an argument the
/// build starts passing reaches the page or fails this test.
const BUILD_SCRIPT: &str = include_str!("../../../crates/types/build.rs");

/// The arguments the build passes `git describe`, read off the one call that asks:
/// a flag added there is a flag the install page owes the reader.
fn describe_arguments() -> Vec<String> {
    let call = BUILD_SCRIPT
        .lines()
        .find(|line| line.contains("\"describe\""))
        .unwrap_or_else(|| panic!("crates/types/build.rs asks `git describe` for the version"));
    call.split('"')
        .skip(1)
        .step_by(2)
        .map(str::to_string)
        .collect()
}

/// The paragraph of the install page that states the version line: the one naming
/// `git describe`, its wrapped lines read as one.
fn version_line_paragraph() -> String {
    INSTALL_PAGE
        .split("\n\n")
        .find(|paragraph| paragraph.contains("`git describe"))
        .map(|paragraph| paragraph.replace('\n', " "))
        .unwrap_or_else(|| {
            panic!("docs/install.md never says what the version line is: {INSTALL_PAGE:?}")
        })
}

#[test]
fn the_install_page_gives_the_describe_arguments_the_build_passes() {
    let arguments = describe_arguments();
    assert_eq!(
        arguments.first().map(String::as_str),
        Some("describe"),
        "the build asks `git describe` for the version: {arguments:?}"
    );
    let command = format!("git {}", arguments.join(" "));
    let paragraph = version_line_paragraph();

    assert!(
        paragraph.contains(&format!("`{command}`")),
        "the build passes {arguments:?}, so the page must give `{command}` and not a \
         shorter one the build never runs: {paragraph:?}"
    );
}

#[test]
fn the_install_page_says_what_the_dirty_suffix_means() {
    let paragraph = version_line_paragraph();

    assert!(
        paragraph.contains("`-dirty`"),
        "a suffix no page explains is the one a reader quotes in a bug report, so it \
         must be named: {paragraph:?}"
    );
    assert!(
        paragraph.contains("uncommitted changes"),
        "`-dirty` says the tree had uncommitted changes when the binary was built, and \
         the page must say that rather than leave the suffix to be guessed: \
         {paragraph:?}"
    );
}

#[test]
fn the_install_page_prints_a_clean_builds_example_line() {
    let paragraph = version_line_paragraph();
    let printed: Vec<&str> = paragraph
        .split('`')
        .filter(|span| span.starts_with("sindri ") && span.contains('('))
        .collect();

    assert_eq!(
        printed,
        ["sindri 9be47ec (2026-11-03)"],
        "the version line the page prints is a clean build's, and the suffix beside it \
         is explained in words rather than shown as the ordinary case: {paragraph:?}"
    );
}

#[test]
fn the_install_page_keeps_the_fallback_for_a_tree_with_no_git() {
    let page = INSTALL_PAGE.replace('\n', " ");

    assert!(
        page.contains("`Cargo.toml` version") && page.contains("`unknown`"),
        "a tree without `git` falls back to the workspace version and a commit date of \
         `unknown`, and that sentence stays: {page:?}"
    );
}
