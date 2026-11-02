//! The measurement page, held to the prompt the tree ships. `docs/system-prompt.md`
//! is the one place the preamble's cost is argued, so a sentence in it the shipped
//! text cannot support is a page describing a prompt no run ever carries — the
//! failure the page exists to prevent. Both texts are `include_str!`, so the test
//! reads the files themselves rather than a copy that could drift from them.

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
