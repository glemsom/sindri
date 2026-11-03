//! The measurement page and the bench behind it, held to the prompt the tree
//! ships. `docs/system-prompt.md` is the one place the preamble's cost is argued,
//! so a sentence in it the shipped text cannot support is a page describing a
//! prompt no run ever carries — the failure the page exists to prevent. The bench
//! is held the same way: its baseline is `PREAMBLE` read out of `rig_wire.rs`, and
//! a copy of the prose in `variants.py` is the same drift one step earlier. The
//! page and the modules are `include_str!`, and the bench is run as the bench —
//! `python3` importing `variants.py` — so neither reading can drift from the
//! file it claims to describe.

/// The page, as committed.
const PAGE: &str = include_str!("../../../docs/system-prompt.md");

/// The `bash` tool's own module: the description the batching sentence lives in.
const TOOL: &str = include_str!("../src/agent/tool.rs");

/// The provider's module, holding the preamble and the test that keeps the two
/// texts apart.
const RIG_WIRE: &str = include_str!("../src/provider/rig_wire.rs");

/// The batching sentence as it is written in the tool description, quoted rather
/// than written down twice: a second copy of it is the drift this test catches.
const BATCHING: &str = "put the steps of one task in one script, not in three calls";

/// The test that asserts the preamble carries none of what the tool description
/// carries, named here because the page points a reader at it.
const KEEPER: &str = "the_script_sentence_is_stated_once_and_in_the_tool_description";

/// The directory `variants.py` lives in, found from the crate rather than from
/// whatever directory the test binary happens to run in.
const BENCH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../scripts/bench-system-prompt"
);

/// The page's section `heading`, up to the next `## `. A claim is only a claim in
/// the section that makes it, and the rest of the page is not this test's business.
fn section(heading: &str) -> &'static str {
    let start = PAGE
        .find(heading)
        .unwrap_or_else(|| panic!("docs/system-prompt.md has no `{heading}` section"));
    let body = &PAGE[start + heading.len()..];
    let end = body.find("\n## ").unwrap_or(body.len());
    &body[..end]
}

#[test]
fn the_measurement_page_does_not_claim_the_preamble_carries_what_the_tool_description_carries() {
    let said = section("## The batching sentence");
    assert!(
        TOOL.contains(BATCHING),
        "the tool description no longer carries `{BATCHING}`, so the page quotes nothing"
    );
    assert!(
        said.contains(BATCHING),
        "the page quotes the batching sentence as the tool description writes it: {said}"
    );
    assert!(
        said.contains("tool description"),
        "the page says where the batching sentence lives: {said}"
    );
    assert!(
        !said.contains("$TMPDIR"),
        "the preamble carries no `$TMPDIR`, and the page no longer says it does: {said}"
    );
    assert!(
        said.contains("seventy-five"),
        "the page states the words the preamble actually ships: {said}"
    );
    assert!(
        said.contains("has not been run"),
        "v14 was never measured, and the page still says so: {said}"
    );
    assert!(
        said.contains(KEEPER) && RIG_WIRE.contains(KEEPER),
        "the page names the `rig_wire` test that keeps the two texts apart: {said}"
    );
}

/// `PREAMBLE` as `rig_wire.rs` spells it: the literal's continuations and escapes
/// resolved, which is the reading `variants.py` performs on the same file.
fn preamble() -> String {
    const MARK: &str = "const PREAMBLE: &str = \"";
    let declared = RIG_WIRE
        .find(MARK)
        .unwrap_or_else(|| panic!("rig_wire.rs declares PREAMBLE"))
        + MARK.len();
    let literal: Vec<char> = RIG_WIRE[declared..].chars().collect();
    let mut text = String::new();
    let mut at = 0;
    while literal[at] != '"' {
        if literal[at] != '\\' {
            text.push(literal[at]);
            at += 1;
            continue;
        }
        let escape = literal[at + 1];
        at += 2;
        match escape {
            '\n' => {
                while literal[at] == ' ' || literal[at] == '\t' {
                    at += 1;
                }
            }
            'n' => text.push('\n'),
            't' => text.push('\t'),
            escaped => text.push(escaped),
        }
    }
    text
}

/// The variants table's row for `variant`, which is where the page states what
/// that prompt is.
fn row(variant: &str) -> &'static str {
    let want = format!("| {variant} |");
    PAGE.lines()
        .find(|line| line.starts_with(&want))
        .unwrap_or_else(|| panic!("docs/system-prompt.md has no `{variant}` row"))
}

#[test]
fn the_bench_baseline_is_the_preamble_the_tree_ships() {
    assert!(
        row("v00").contains("`PREAMBLE`") && row("v00").contains("`rig_wire.rs`"),
        "the v00 row says v00 is the preamble the tree ships, rather than a text and a count of its own: {}",
        row("v00")
    );
    let printed = std::process::Command::new("python3")
        .args([
            "-c",
            "import sys; sys.path.insert(0, sys.argv[1]); import variants; sys.stdout.write(variants.BASE)",
        ])
        .arg(BENCH)
        .output()
        .expect("python3 runs the bench's own module");
    assert!(
        printed.status.success(),
        "variants.py would not import: {}",
        String::from_utf8_lossy(&printed.stderr)
    );
    assert_eq!(
        String::from_utf8(printed.stdout).expect("variants.py prints the baseline as text"),
        preamble(),
        "`variants.BASE` is the shipped `PREAMBLE`, so `build_variant.sh v00` builds the prompt the tree ships"
    );
}
