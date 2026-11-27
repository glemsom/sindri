use super::*;

#[test]
fn missing_helpers_are_warnings_not_requirements() {
    let output = RECOMMENDED
        .iter()
        .map(|name| format!("missing:{name}\n"))
        .collect::<String>();
    let helpers = Helpers::parse(&output).unwrap();
    assert!(helpers.render().contains("Warning: recommended helpers checked but not found: git, python3, jq, yq, awk, sed, rg, grep."));
    assert!(helpers.render().contains("execution PATH: none."));
}

#[test]
fn incomplete_probe_output_is_not_an_absent_helper() {
    assert!(Helpers::parse("available:git\n").is_err());
}

#[test]
fn available_and_missing_helpers_are_reported_separately() {
    let output = RECOMMENDED
        .iter()
        .map(|name| {
            format!(
                "{}:{name}\n",
                if *name == "rg" {
                    "missing"
                } else {
                    "available"
                }
            )
        })
        .collect::<String>();
    let rendered = Helpers::parse(&output).unwrap().render();
    assert!(rendered.contains("execution PATH: git, python3, jq, yq, awk, sed, grep."));
    assert!(rendered.contains("not found: rg."));
}
