use std::collections::BTreeMap;

use sindri::capability::{Answers, Capabilities};

pub(crate) fn pictured() -> Capabilities {
    probed(
        &[("TERM", "xterm-kitty"), ("COLORTERM", "truecolor")],
        Answers::silent(),
    )
}

fn probed(terms: &[(&str, &str)], answers: Answers) -> Capabilities {
    let terms: BTreeMap<String, String> = terms
        .iter()
        .map(|(key, value)| (String::from(*key), String::from(*value)))
        .collect();
    Capabilities::probed(&terms, &answers, true).0
}
