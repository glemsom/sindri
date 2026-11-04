//! The README's gateway section, asserted rather than trusted: a reader who
//! follows the worked example and nothing else has to reach the gateway, so the
//! wire it names, the address it points at, the model it asks and every level
//! that sets its header are read here — and the tree's own words for the token
//! a header may carry, so a rename fails this file rather than a reader's run.

use sindri_core::config::Provider;
use sindri_core::provider::SESSION_TOKEN;
use sindri_tui::command::COMMANDS;

/// The README, as committed: `include_str!` so the test cannot pass against a
/// file it read from the wrong directory.
const README: &str = include_str!("../../../README.md");

/// The README's `## Configuration` section, where the config's own table sits
/// and, beside it, the worked example for a gateway.
fn configuration_section() -> &'static str {
    README
        .split("\n## ")
        .find(|section| section.starts_with("Configuration"))
        .unwrap_or_else(|| panic!("README.md has no `## Configuration` section"))
}

/// The gateway the worked example points a reader at, the anchor its heading
/// makes, and the header it routes on — facts the tree cannot carry, because a
/// gateway's own address and header belong to the gateway.
const GATEWAY: &str = "OpenCode Go";
const GATEWAY_ANCHOR: &str = "opencode-go";
const GATEWAY_HEADER: &str = "x-opencode-session";

/// The gateway's own subsection, from its heading to the next one or the end of
/// the Configuration section — so a word said below it is not read as its own.
fn gateway_section() -> &'static str {
    let heading = format!("\n### {GATEWAY}\n");
    let section = configuration_section();
    let at = section
        .find(&heading)
        .unwrap_or_else(|| panic!("the Configuration section has no `### {GATEWAY}`: {section:?}"));
    let body = &section[at + heading.len()..];
    body.split("\n### ").next().unwrap_or(body)
}

/// How the composer spells a command, read from the catalogue rather than typed.
fn command_line(name: &str) -> String {
    let spec = COMMANDS
        .iter()
        .find(|spec| spec.name == name)
        .unwrap_or_else(|| panic!("the composer sends no `/{name}`"));
    format!("`/{}`", spec.name)
}

#[test]
fn the_readme_extra_headers_row_leads_to_the_gateway_section() {
    let section = configuration_section();
    let row = section
        .split('\n')
        .find(|line| line.starts_with("| Extra request headers"))
        .unwrap_or_else(|| panic!("the config table has no extra-headers row"));

    let anchor = format!("](#{GATEWAY_ANCHOR})");
    assert!(
        row.contains(&anchor),
        "a header a gateway routes on is the row's own business, so the row must lead a \
         reader to the worked example, at {anchor}, rather than leave them to find it: {row:?}"
    );
    assert!(
        section.contains(&format!("### {GATEWAY}")),
        "and the anchor is the one that heading makes, or the link lands nowhere: {row:?}"
    );
}

#[test]
fn the_readme_gateway_example_states_the_wire_the_address_the_header_and_its_levels() {
    let gateway = gateway_section();
    let wire = format!("`{}`", Provider::Openai.as_str());
    let settings = command_line("settings");
    for needed in [
        wire.as_str(),
        "`base_url`",
        "/v1",
        "model = ",
        "[extra_headers]",
        GATEWAY_HEADER,
        settings.as_str(),
        "`SINDRI_EXTRA_HEADERS`",
        "`--header`",
    ] {
        assert!(
            gateway.contains(needed),
            "a reader who follows the worked example and nothing else reaches the gateway \
             only if it names {needed} — the wire, the address, the model it asks, and every \
             level that sets a header: {gateway:?}"
        );
    }
}

#[test]
fn the_readme_says_what_the_session_token_becomes_and_where_to_read_it_back() {
    let gateway = gateway_section();
    let new = command_line("new");
    for needed in [
        SESSION_TOKEN,
        "caches",
        new.as_str(),
        "`sindri sessions show`",
    ] {
        assert!(
            gateway.contains(needed),
            "the token is taught by what it becomes — what the gateway caches, the session \
             a run writes, and the {new} that mints another — so the section must say \
             {needed}: {gateway:?}"
        );
    }
}
