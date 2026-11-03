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

/// The built-in that holds the procedure for changing a file, and the name the
/// catalog routes a task to.
const CHANGING: &str = "changing-files";

/// One built-in by name.
fn builtin_named(name: &str) -> Skill {
    builtins()
        .skills
        .into_iter()
        .find(|skill| skill.name == name)
        .unwrap_or_else(|| panic!("the binary ships no `{name}`"))
}

/// The built-in that holds the procedure for changing a file.
fn changing() -> Skill {
    builtin_named(CHANGING)
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
fn a_builtin_body_and_its_files_are_read_from_the_binary() {
    for found in shipped() {
        let body = found.body().expect("the bytes are compiled in");
        assert!(!body.trim().is_empty());
        assert!(
            !body.starts_with("---") && !body.contains("\nname: "),
            "the frontmatter is stripped, as it is for any skill: {body}"
        );

        let listing = found.listing();
        assert!(
            !listing.is_empty(),
            "the files are listed from the same source"
        );
        assert_eq!(
            listing.len(),
            found.files().len(),
            "every file the body may name is listed: {listing:?}"
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
fn every_builtin_skill_parses_and_names_a_file_its_body_points_at() {
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
    let shipped = found.skills;
    for found in shipped {
        assert!(!found.description.trim().is_empty(), "{found:?}");
        let body = found.body().expect("a built-in's frontmatter parses");
        for file in found.files() {
            assert!(
                body.contains(file.path),
                "{} is a file nothing in the body names: {body}",
                file.path
            );
        }
    }
}

#[test]
fn every_reference_a_builtin_body_names_is_a_file_it_ships() {
    // The other direction of the same rule: a body whose `references/…` is not
    // in the table reads as a file the model may open, and a built-in has no
    // directory for one to be in.
    for found in builtins().skills {
        let body = found.body().expect("a built-in's frontmatter parses");
        let shipped: Vec<&str> = found.files().iter().map(|file| file.path).collect();
        let named: Vec<&str> = body
            .split(|at: char| at.is_whitespace() || at == '`')
            .map(|word| word.trim_end_matches([',', '.', ':', ';', ')']))
            .filter(|word| word.starts_with("references/"))
            .collect();
        assert!(
            !named.is_empty(),
            "{} names no reference at all, so its long form is a body nobody can \
             read further from: {body}",
            found.name
        );
        for reference in named {
            assert!(
                shipped.contains(&reference),
                "{reference} is named by {} and is not a file it ships ({shipped:?})",
                found.name
            );
        }
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

/// Every fenced block of a body, each as its own text: one recipe is one block,
/// and a block holding two commands is two calls where the skill promised one.
fn fenced_blocks(body: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut open = false;
    for line in body.lines() {
        if line.trim_start().starts_with("```") {
            open = !open;
            if open {
                blocks.push(String::new());
            }
            continue;
        }
        if open {
            let block = blocks.last_mut().expect("a block is open");
            block.push_str(line);
            block.push('\n');
        }
    }
    assert!(!open, "every fence in the body is closed: {body}");
    blocks
}

#[test]
fn the_finding_skill_names_cat_repeated_greps_and_a_printed_whole_file() {
    // The three defaults a model brings to a file: print it, then grep per guess,
    // then print the whole of it because one line matched. A body that does not
    // name what it corrects is read as advice beside a habit it never touches.
    let body = body_of("search-and-read");

    for default in ["`cat`", "grep", "whole file"] {
        assert!(
            body.contains(default),
            "the body names the default it corrects, `{default}`: {body}"
        );
    }
}

#[test]
fn the_changing_skill_names_the_rewrite_the_half_edit_and_the_weakened_check() {
    // The three failures the procedure exists to prevent, each in the words the
    // model is handed: a whole-file rewrite that loses what surrounds the
    // change, a tree left half-edited by a pass that failed midway, and an
    // assertion weakened until it stopped noticing. A body that does not name
    // what it prevents is read as advice beside a habit it never touches.
    let body = body_of(CHANGING);

    for (word, prevented) in [
        (
            "rewrite",
            "the whole-file rewrite that loses what surrounds the change",
        ),
        (
            "half-edited",
            "a tree left half-edited by a pass that failed midway",
        ),
        ("weaken", "an assertion weakened until it stopped noticing"),
        ("anchor", "the anchor the change is made at"),
        ("exactly once", "the count an anchor must be found in"),
    ] {
        assert!(
            body.contains(word),
            "the body says nothing about {prevented}: {word:?} is not in it: {body}"
        );
    }
}

#[test]
fn the_changing_skill_ends_by_reading_the_change_back_and_running_the_narrowest_check() {
    // A write that printed nothing is not a change that happened, so the section
    // a model reads last is the proof: the region read back, and the narrowest
    // check that would have caught it. It is the last section rather than an
    // early one for the same reason it is the last step of the procedure.
    let body = body_of(CHANGING);
    let last = body
        .rsplit("\n## ")
        .next()
        .expect("the body carries sections");

    assert!(
        last.starts_with("Re-read the changed region"),
        "the body ends on the step that proves the change: {last}"
    );
    for (word, step) in [
        ("sed -n", "the region read back"),
        ("--check", "the narrowest check run"),
    ] {
        assert!(
            last.contains(word),
            "the last section does not say where {step} is: {word:?} is not in it: {last}"
        );
    }
}

#[test]
fn every_recipe_a_shipped_body_gives_is_one_call() {
    // The batching this skill teaches is a single `bash` call doing many things,
    // never several calls in a row: so a recipe is one command, and a block of
    // steps is the shape the skill exists to replace.
    for found in builtins().skills {
        let body = found.body().expect("a built-in's frontmatter parses");
        for block in fenced_blocks(&body) {
            let commands = block.lines().filter(|line| !line.trim().is_empty()).count();
            assert_eq!(
                commands, 1,
                "`{}` gives this recipe as {commands} commands and a recipe is one call: {block}",
                found.name
            );
        }
    }
}

#[test]
fn the_finding_skill_sends_the_many_file_loop_as_a_file_it_ships() {
    // The loop over many inputs is a script, and a script the body carried would
    // be a block of steps in a recipe's place: it ships as a file instead, and
    // the body says which file.
    let found = builtin_named("search-and-read");
    let body = found.body().expect("a built-in's frontmatter parses");
    let named: Vec<&str> = body
        .split(|at: char| at.is_whitespace() || at == '`')
        .map(|word| word.trim_end_matches([',', '.', ':', ';', ')']))
        .filter(|word| word.starts_with("references/"))
        .collect();

    assert!(
        named.contains(&"references/many-files.md"),
        "the loop over many files ships as a file the body names: {named:?}"
    );
    assert!(
        named.contains(&"references/recipes.md"),
        "and so does the table the body is too short to carry: {named:?}"
    );
}

#[test]
fn the_changing_skill_ships_its_many_file_pass_as_a_program_its_body_names() {
    // The pass over many files is a program, and a program the body carried
    // would be a block of steps in a recipe's place: it ships as a file the body
    // names, under its own name, so the file the pass is read from is not one
    // another skill's body also points a reader at.
    let found = changing();
    let body = found.body().expect("a built-in's frontmatter parses");

    assert!(
        body.contains("references/edit-plan.md"),
        "the pass over many files ships as a file the body names: {body}"
    );
    let pass = found
        .files()
        .iter()
        .find(|file| file.path == "references/edit-plan.md")
        .unwrap_or_else(|| panic!("the pass is one the table carries: {:?}", found.listing()));
    assert!(
        pass.text.contains("```python"),
        "and it is a program a run can execute, not a recipe it reads: {}",
        pass.text
    );
}

/// The first fenced `python` program in a shipped Markdown file, which is how a
/// reference can be run rather than read.
fn python_fence(path: &str, text: &str) -> String {
    let mut lines = text.lines();
    lines
        .find(|line| line.trim_end() == "```python")
        .unwrap_or_else(|| panic!("{path} holds no program to run: {text}"));
    let program: Vec<&str> = lines.by_ref().take_while(|line| *line != "```").collect();
    assert!(
        !program.is_empty(),
        "{path} opens a fence and holds nothing: {text}"
    );
    program.join("\n")
}

#[test]
fn the_many_file_procedure_checks_every_anchor_before_it_writes_any_file() {
    // The procedure is a script a model can run, so it is run rather than read:
    // one anchor that is there twice and one file that is not there, and the
    // file whose anchor was good is left exactly as it was, with both bad ones
    // named.
    let found = changing();
    let reference = found
        .files()
        .iter()
        .find(|file| file.path == "references/edit-plan.md")
        .expect("the pass over many files is a file beside the body");
    let script = python_fence(reference.path, reference.text);

    let tree = TempDir::new().expect("a tree");
    let one = tree.path().join("one.txt");
    let two = tree.path().join("two.txt");
    let gone = tree.path().join("gone.txt");
    std::fs::write(&one, "alpha = 1\n").expect("a file");
    std::fs::write(&two, "beta = 2\nbeta = 2\n").expect("a file");
    let program = tree.path().join("apply.py");
    std::fs::write(&program, script).expect("the shipped program");

    let plan = tree.path().join("plan.json");
    std::fs::write(
        &plan,
        serde_json::json!([
            {"file": one, "anchor": "alpha = 1", "replacement": "alpha = 3"},
            {"file": two, "anchor": "beta = 2", "replacement": "beta = 3"},
            {"file": gone, "anchor": "delta = 4", "replacement": "delta = 5"}
        ])
        .to_string(),
    )
    .expect("a plan");

    let out = std::process::Command::new("python3")
        .arg(&program)
        .arg(&plan)
        .output()
        .expect("python3 runs");
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    assert!(
        !out.status.success(),
        "a plan with an anchor that is not one is a failure: {said}"
    );
    assert!(
        said.contains("two.txt: the anchor appears 2 times, not once"),
        "the file whose anchor is there twice is named, with its count: {said}"
    );
    assert!(
        said.contains("gone.txt: no such file"),
        "and a file the tree does not hold is said to be missing, not counted zero: {said}"
    );
    assert_eq!(
        std::fs::read_to_string(&one).expect("the file is still there"),
        "alpha = 1\n",
        "the good edit waited for the bad ones to be reported, so the tree is untouched"
    );

    // The same plan with the anchor that is there twice fixed, and the file that
    // was missing written: every one of them is changed by the one pass.
    std::fs::write(&two, "beta = 2\ngamma = 2\n").expect("a file");
    std::fs::write(&gone, "delta = 4\n").expect("a file");
    let out = std::process::Command::new("python3")
        .arg(&program)
        .arg(&plan)
        .output()
        .expect("python3 runs");
    assert!(
        out.status.success(),
        "every anchor found once: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(&one).expect("written"),
        "alpha = 3\n"
    );
    assert_eq!(
        std::fs::read_to_string(&two).expect("written"),
        "beta = 3\ngamma = 2\n"
    );
    assert_eq!(
        std::fs::read_to_string(&gone).expect("written"),
        "delta = 5\n"
    );
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
