//! The terminals a test states rather than probes: the binary's own frames are drawn on these.

use std::collections::BTreeMap;

use sindri::capability::{Answers, Capabilities};

/// A terminal that names Kitty and read the block as one cell: pictures at the nominal cell and no
/// large text.
pub(crate) fn pictured() -> Capabilities {
    probed(
        &[("TERM", "xterm-kitty"), ("COLORTERM", "truecolor")],
        Answers::silent(),
    )
}

/// A terminal that names Kitty and drew the block at size: pictures, large text and a pointer.
pub(crate) fn sized() -> Capabilities {
    probed(
        &[("TERM", "xterm-kitty"), ("COLORTERM", "truecolor")],
        Answers::silent().sized(),
    )
}

fn probed(terms: &[(&str, &str)], answers: Answers) -> Capabilities {
    let terms: BTreeMap<String, String> = terms
        .iter()
        .map(|(key, value)| (String::from(*key), String::from(*value)))
        .collect();
    Capabilities::probed(&terms, &answers, true).0
}
