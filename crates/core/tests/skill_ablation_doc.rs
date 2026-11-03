//! The ablation page and the bench behind it, held apart. `docs/skill-ablation.md`
//! carries the argument — the three states a built-in can be in, the rule for
//! when one may ship, and what the measurement does not prove — and
//! `scripts/bench-skill-ablation/README.md` is the machine. A method line in
//! both is the same rule written twice, and the copy nobody is reading is the
//! one that goes stale; so the page is held to carrying none of the README's,
//! and to still making the argument only it makes.

/// The page, as committed: the argument.
const PAGE: &str = include_str!("../../../docs/skill-ablation.md");

/// The bench's own README: the method.
const README: &str = include_str!("../../../scripts/bench-skill-ablation/README.md");

/// The sentences of `text` worth calling a method line: seven words or more,
/// with wrapping normalised away, so two files that say the same thing in the
/// same words are caught whichever way each of them is folded.
fn sentences(text: &str) -> Vec<String> {
    text.split_terminator('.')
        .map(|sentence| sentence.split_whitespace().collect::<Vec<&str>>().join(" "))
        .filter(|sentence| sentence.split(' ').count() >= 7)
        .collect()
}

#[test]
fn no_method_line_is_carried_in_both_the_page_and_the_bench_readme() {
    let page = sentences(PAGE);
    let machine = sentences(README);
    let shared: Vec<&String> = machine
        .iter()
        .filter(|sentence| page.contains(sentence))
        .collect();
    assert!(
        shared.is_empty(),
        "the page argues and the README is the machine, so neither repeats the other: {shared:#?}"
    );
}

#[test]
fn the_page_still_states_why_its_table_is_empty_and_when_a_built_in_ships() {
    assert!(
        PAGE.contains("not yet measured"),
        "the table's cells say no measurement is behind them"
    );
    assert!(
        PAGE.contains("no live run has been made"),
        "the page says why every cell is empty"
    );
    assert!(
        PAGE.contains("## The shipping rule"),
        "the page states the rule a built-in earns its catalog line by"
    );
    for condition in [
        "**it is reached**",
        "**it changes the outcome**",
        "**it pays for itself**",
    ] {
        assert!(
            PAGE.contains(condition),
            "the shipping rule is three conditions, and `{condition}` is one of them"
        );
    }
    for state in ["**Unused.**", "**Useless.**", "**Useful.**"] {
        assert!(
            PAGE.contains(state),
            "the three states a skill can be in are the page's own argument, and `{state}` is one of them"
        );
    }
}

#[test]
fn the_page_names_no_script_the_bench_readme_already_runs() {
    for script in [
        "build_arm.sh",
        "run_one.sh",
        "sweep.sh",
        "self-test.sh",
        "bench.py",
    ] {
        assert!(
            !PAGE.contains(script),
            "`{script}` is the README's to run, and the page argues instead of repeating it"
        );
    }
    assert!(
        README.contains("build_arm.sh") && README.contains("sweep.sh"),
        "the README is where the commands live, so the page can point at it"
    );
}
