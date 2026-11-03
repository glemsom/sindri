//! The third scope: a `SKILL.md` compiled into the binary, read with no
//! directory behind it, and displaced by a nearer scope that carries the same
//! name. Every tree here holds no skill of its own, so a name in the catalog
//! belongs to the binary or to the one directory the test wrote.

use sindri_core::skills::{
    BUILTINS, CONTENT_CEILING, Catalog, DESCRIPTION_CEILING, FILE_CEILING, Scope, Skill, builtins,
    discover,
};
use tempfile::TempDir;

use crate::fixtures::{repo, skill};

/// Every skill the binary ships, which is the set each rule below is held to:
/// a rule proved for the first skill is one the next never met.
fn shipped() -> Vec<Skill> {
    let skills = builtins().skills;
    assert!(!skills.is_empty(), "the binary ships a skill");
    skills
}

/// The names those skills are matched on, which is what a test writes a nearer
/// scope's copy under.
fn shipped_names() -> Vec<String> {
    shipped().into_iter().map(|skill| skill.name).collect()
}

/// The built-in that holds the procedure for changing a file, and the one
/// holding the procedure for reading one: the two names the catalog routes a
/// task to, and the pair that must not shadow each other.
const EDITING: &str = "edit-files";
const FINDING: &str = "search-and-read-files";

/// One built-in by name.
fn builtin_named(name: &str) -> Skill {
    builtins()
        .skills
        .into_iter()
        .find(|skill| skill.name == name)
        .unwrap_or_else(|| panic!("the binary ships no `{name}`"))
}

#[test]
fn a_builtin_skill_is_found_in_a_workspace_that_holds_no_skill_at_all() {
    let workspace = TempDir::new().expect("a workspace");
    let shipped = builtins();

    let catalog = discover(workspace.path(), None);

    let names: Vec<&str> = catalog
        .skills
        .iter()
        .map(|skill| skill.name.as_str())
        .collect();
    let mut shipped_names: Vec<&str> = shipped
        .skills
        .iter()
        .map(|skill| skill.name.as_str())
        .collect();
    // The catalog is sorted by name and the table is in the order a reader of it
    // wants, so the two are compared as the sets of names they are.
    shipped_names.sort_unstable();
    assert_eq!(
        names, shipped_names,
        "the binary's own are in every catalog"
    );
    for found in &catalog.skills {
        assert_eq!(found.scope, Scope::Builtin);
        assert!(
            found.dir().is_none(),
            "a built-in is the one skill with no directory: {found:?}"
        );
    }
    assert!(catalog.refused.is_empty(), "{catalog:?}");
}

#[test]
fn a_builtin_body_is_read_from_the_binary_and_every_file_beside_it_too() {
    for found in shipped() {
        let body = found.body().expect("the bytes are compiled in");
        assert!(!body.trim().is_empty());
        assert!(
            !body.starts_with("---") && !body.contains("\nname: "),
            "the frontmatter is stripped, as it is for any skill: {body}"
        );

        let listing = found.listing();
        assert_eq!(
            listing.len(),
            found.files().len(),
            "every file a body may name is listed from the same source: {listing:?}"
        );
        for file in found.files() {
            assert!(
                listing.contains(&file.path.to_string()),
                "{} is listed: {listing:?}",
                file.path
            );
            assert!(
                !file.text.trim().is_empty(),
                "{} is compiled in, so it can be read: {:?}",
                file.path,
                file.text
            );
        }
    }
}

#[test]
fn every_builtin_skill_parses_and_carries_a_body_a_model_can_read() {
    let found = builtins();
    assert!(
        found.refused.is_empty(),
        "a built-in that will not read is a refusal and not a silence: {found:?}"
    );
    assert_eq!(
        found.skills.len(),
        BUILTINS.len(),
        "every entry in the table is a skill, so nothing was dropped on the way in"
    );
    for found in found.skills {
        assert!(
            !found.description.trim().is_empty(),
            "a skill with no routing key is one nothing is matched to: {found:?}"
        );
        let body = found.body().expect("a built-in's frontmatter parses");
        assert!(
            body.lines().filter(|line| !line.trim().is_empty()).count() >= 2,
            "{} carries a heading and the instructions under it: {body}",
            found.name
        );
    }
}

#[test]
fn neither_shipped_body_names_a_file_because_neither_ships_one() {
    // The other direction of the same rule: a body naming `references/…` is read
    // as a file the model may open with `bash`, and a built-in has no directory
    // for one to be in. Both shipped skills are a page of instructions and
    // nothing beside them, so a reference named here would be a path a run could
    // not follow — and one the load would silently not deliver.
    for found in builtins().skills {
        let body = found.body().expect("a built-in's frontmatter parses");
        let named: Vec<&str> = body
            .split(|at: char| at.is_whitespace() || at == '`')
            .map(|word| word.trim_end_matches([',', '.', ':', ';', ')']))
            .filter(|word| word.starts_with("references/"))
            .collect();
        assert!(
            named.is_empty(),
            "{} names {named:?} and ships no file to read it from: {body}",
            found.name
        );
        assert!(
            found.files().is_empty(),
            "and the table carries none for it: {:?}",
            found.listing()
        );
    }
}

#[test]
fn two_builtins_never_ship_one_name_because_neither_can_shadow_the_other() {
    let shipped = builtins().skills;
    let mut names: Vec<&str> = shipped.iter().map(|skill| skill.name.as_str()).collect();
    let total = names.len();
    names.sort_unstable();
    names.dedup();
    assert_eq!(
        names.len(),
        total,
        "one built-in per name: a repeat inside one scope has no nearer skill to lose to, so \
         the fold would record it as its own loser and `sindri doctor` would print `builtin \
         shadows builtin`: {names:?}"
    );
}

#[test]
fn a_user_skill_of_a_builtins_name_displaces_it_and_the_shadow_names_what_lost() {
    let home = TempDir::new().expect("a home");
    let workspace = TempDir::new().expect("a workspace");
    for name in shipped_names() {
        skill(
            &home.path().join(".agents/skills"),
            &name,
            &format!("---\nname: {name}\ndescription: The user's own.\n---\n\n# mine\n"),
        );
    }

    let catalog: Catalog = discover(workspace.path(), Some(home.path()));

    let names = shipped_names();
    for name in &names {
        let winner = catalog
            .skills
            .iter()
            .find(|skill| &skill.name == name)
            .expect("the name is held");
        assert_eq!(winner.scope, Scope::User, "the nearer scope wins");
        assert_eq!(winner.description, "The user's own.");
        let shadow = catalog
            .shadowed
            .iter()
            .find(|shadow| &shadow.loser.name == name)
            .unwrap_or_else(|| panic!("`{name}` is recorded as displaced: {catalog:?}"));
        assert_eq!(shadow.loser.scope, Scope::Builtin);
        assert_eq!(
            shadow.loser.dir, None,
            "a built-in is not printed by a path it has not"
        );
        let report = catalog.report();
        assert!(
            report.contains(&format!("{name}: ")) && report.contains("shadows builtin"),
            "the displaced built-in is named with its origin: {report}"
        );
    }
    assert_eq!(
        catalog.shadowed.len(),
        names.len(),
        "one shadow per displaced name and none beside them: {catalog:?}"
    );
}

#[test]
fn a_repository_skill_takes_the_name_from_both_the_user_skill_and_the_builtin() {
    let home = TempDir::new().expect("a home");
    let (root, workspace) = repo();
    for name in shipped_names() {
        skill(
            &home.path().join(".agents/skills"),
            &name,
            &format!("---\nname: {name}\ndescription: The user's own.\n---\n"),
        );
        skill(
            &root.path().join(".agents/skills"),
            &name,
            &format!("---\nname: {name}\ndescription: This repository's.\n---\n"),
        );
    }

    let catalog = discover(&workspace, Some(home.path()));

    let names = shipped_names();
    for name in &names {
        let winner = catalog
            .skills
            .iter()
            .find(|skill| &skill.name == name)
            .expect("the name is held");
        assert_eq!(
            winner.scope,
            Scope::Repo,
            "nearest wins: repo, user, builtin"
        );
        let mut lost: Vec<Scope> = catalog
            .shadowed
            .iter()
            .filter(|shadow| &shadow.loser.name == name)
            .map(|shadow| shadow.loser.scope)
            .collect();
        lost.sort_by_key(|scope| scope.as_str());
        assert_eq!(
            lost,
            vec![Scope::Builtin, Scope::User],
            "both the losers of `{name}` are recorded: {catalog:?}"
        );
        for shadow in catalog
            .shadowed
            .iter()
            .filter(|shadow| &shadow.loser.name == name)
        {
            assert_eq!(
                shadow.winner.scope,
                Scope::Repo,
                "and the winner is the one that holds it"
            );
        }
    }
    assert_eq!(
        catalog.shadowed.len(),
        names.len() * 2,
        "two losers per displaced name and no shadow beside them: {catalog:?}"
    );
}

#[test]
fn the_report_prints_a_builtins_own_scope_word_and_no_path_under_it() {
    let workspace = TempDir::new().expect("a workspace");

    let report = discover(workspace.path(), None).report();

    for name in shipped_names() {
        let row = report
            .lines()
            .find(|line| line.contains(&name))
            .unwrap_or_else(|| panic!("`{name}` is a row of the report: {report}"));
        assert!(row.starts_with("  builtin"), "its own scope word: {row}");
        assert!(
            !row.contains('/'),
            "no path is printed for a skill that has none: {row}"
        );
    }
}

#[test]
fn every_builtin_description_fits_the_ceiling_so_none_can_refuse_itself_at_runtime() {
    let found = builtins();
    assert!(
        found.refused.is_empty(),
        "a description past the ceiling is refused by the reader a directory's copy goes \
         through, and a built-in that refused itself would be a skill the binary ships and \
         never hands the model: {found:?}"
    );
    let shipped = found.skills;
    for found in &shipped {
        let measured = found.description.chars().count();
        assert!(
            measured <= DESCRIPTION_CEILING,
            "`{}` spends {measured} of the {DESCRIPTION_CEILING} characters a description has, \
             and a built-in that refused itself would be a skill the binary ships and never \
             hands the model: {}",
            found.name,
            found.description
        );
    }
}

/// The body of one shipped skill, by the name a task is matched on: a body a
/// test wrote itself would be prose about something the binary does not ship.
fn body_of(name: &str) -> String {
    builtin_named(name)
        .body()
        .expect("a built-in's frontmatter parses")
}

#[test]
fn the_file_editing_skill_names_the_anchor_the_heredoc_and_the_one_script_pass() {
    // The failures the procedure exists to prevent, each in the words the model
    // is handed: an anchor that appears twice, a read-then-write that loses what
    // landed in between, and a pass of edits that half-succeeds.
    let body = body_of(EDITING);

    for (word, why) in [
        ("assert count == 1", "the count an anchor must be found in"),
        ("Path.read_text()", "the read half of a surgical edit"),
        ("Path.write_text()", "the write half"),
        (
            "AssertionError",
            "what a missed anchor answers with, and what to do next",
        ),
        (
            "re-read the fresh file",
            "the partial write a failed run may have left",
        ),
        ("heredoc", "how a new file is written whole"),
        (
            "one script per edit",
            "why a failed edit leaves no half-applied tree",
        ),
    ] {
        assert!(
            body.contains(word),
            "the body says nothing about {why}: {word:?} is not in it: {body}"
        );
    }
}

#[test]
fn the_file_editing_skill_sends_a_reader_to_the_skill_that_finds_the_anchor() {
    // The edit is only as good as the anchor it is written against, and the
    // skill that finds the anchor is a separate load: named here by the one name
    // the catalog answers, so a model that follows the pointer can load it.
    let body = body_of(EDITING);
    let named: Vec<String> = shipped_names();

    assert!(
        named.contains(&FINDING.to_string()),
        "the skill the body points at is one the binary ships: {named:?}"
    );
    assert!(
        body.contains(&format!("`{FINDING}`")),
        "and the body names it as the catalog spells it: {body}"
    );
}

#[test]
fn the_file_inspection_skill_names_the_find_the_read_and_the_filter() {
    // Three habits, one bullet each: a search that locates rather than prints, a
    // read that is the narrowest shape still telling the truth, and a filter
    // pushed into the command instead of applied after it.
    let body = body_of(FINDING);

    for (word, why) in [
        ("rg -l", "the search that locates files"),
        (
            "rg -n --heading --color=never",
            "the view of the matching lines",
        ),
        ("sed -n 'A,Bp'", "a range read"),
        ("wc -l", "the size taken before a whole-file read"),
        ("--stat", "the diff summary that comes before the diff"),
        (
            "Filter as you read",
            "the predicate pushed into the command",
        ),
    ] {
        assert!(
            body.contains(word),
            "the body says nothing about {why}: {word:?} is not in it: {body}"
        );
    }
}

#[test]
fn no_shipped_file_is_large_enough_to_multiply_the_cost_of_one_load() {
    for found in builtins().skills {
        let body = found.body().expect("the frontmatter parses");
        let files: Vec<&str> = found.files().iter().map(|file| file.text).collect();
        let total = body.len() + files.iter().map(|text| text.len()).sum::<usize>();

        for text in std::iter::once(body.as_str()).chain(files) {
            let measured = text.len();
            assert!(
                measured <= FILE_CEILING,
                "`{}` ships a file of {measured} bytes, past the {FILE_CEILING} one file may \
                 spend: every file is inlined beside the body, so one long reference is a \
                 second body paid for on every load",
                found.name
            );
        }
        assert!(
            total <= CONTENT_CEILING,
            "`{}` spends {total} bytes on one load, past the {CONTENT_CEILING} a skill's whole \
             content may: the files beside a body multiply the cost of loading it",
            found.name
        );
    }
}
