//! The offer the composer makes once `/skill` is typed: the skill names the boot
//! found, ranked by the one function the command names are ranked by, and taken
//! in the spelling the reader is already writing — a colon keeps its colon, a
//! space keeps its space.

use super::{Query, Trigger, ranked};
use crate::command::{SKILL_COLON, SKILL_SPACE};

/// The skill names a draft asks for: `/skill:` or `/skill ` and what is typed
/// after it, on one row and with no whitespace yet — the moment a tail begins,
/// the line is a prompt and the offer is over.
pub fn ask(draft: &str) -> Option<Query> {
    if let Some(typed) = draft.strip_prefix(SKILL_COLON) {
        return one_row(typed).then(|| Query {
            // The colon spelling replaces the whole command, slash and colon
            // included: what is typed after it is the name being narrowed.
            at: 0,
            typed: typed.to_string(),
            trigger: Trigger::Skill,
            inline: true,
        });
    }
    let typed = draft.strip_prefix(SKILL_SPACE)?;
    one_row(typed).then(|| Query {
        // The space spelling leaves the command where it is and replaces the
        // name after it, because a splice ends where the whitespace does.
        at: SKILL_SPACE.chars().count(),
        typed: typed.to_string(),
        trigger: Trigger::Skill,
        inline: false,
    })
}

fn one_row(typed: &str) -> bool {
    !typed.chars().any(char::is_whitespace)
}

/// The skill names a fragment names, in the tiers the commands are ranked by and
/// in the order the catalog holds them, so a fragment reads alike whether it
/// names a command or a skill. The list is the names a human may reach for: a
/// skill the user may not invoke was never in it, and so is never offered.
pub fn offered(typed: &str, skills: &[String], inline: bool) -> Vec<String> {
    ranked(skills.iter().map(String::as_str), typed)
        .into_iter()
        .map(|name| match inline {
            true => format!("{SKILL_COLON}{name}"),
            false => name.to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{ask, offered};

    fn names(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| name.to_string()).collect()
    }

    #[test]
    fn a_draft_names_skills_only_once_the_command_that_takes_one_is_typed() {
        let query = ask("/skill:").expect("a colon asks for the names");
        assert_eq!((query.at, query.typed.as_str()), (0, ""));
        assert!(query.inline, "the colon spelling is kept");
        let query = ask("/skill hou").expect("a space asks for them too");
        assert_eq!(
            query.at, 7,
            "the command before the name is left where it is"
        );
        assert_eq!(query.typed, "hou");
        assert!(!query.inline);
        assert!(
            ask("/skill").is_none(),
            "the command itself is still a command"
        );
        assert!(
            ask("/skill:house-style tidy").is_none(),
            "a tail ends the offer"
        );
        assert!(ask("/skill a b").is_none(), "and so does a second word");
        assert!(
            ask("/skil").is_none(),
            "a fragment of the command is not one"
        );
    }

    #[test]
    fn the_names_come_in_the_tiers_the_commands_come_in() {
        let skills = names(&["house-style", "release-checks", "house-rules"]);
        assert_eq!(
            offered("hous", &skills, true),
            names(&["/skill:house-style", "/skill:house-rules"]),
            "the ones it starts, in the catalog's order"
        );
        assert_eq!(
            offered("rules", &skills, false),
            names(&["house-rules"]),
            "a name it sits inside, and the spelling the draft is in"
        );
        assert_eq!(
            offered("rlc", &skills, false),
            names(&["release-checks"]),
            "a name it is a subsequence of"
        );
        assert!(
            offered("zzz", &skills, false).is_empty(),
            "and a fragment that names nothing offers nothing"
        );
    }

    #[test]
    fn at_most_eight_names_are_offered() {
        let many: Vec<String> = (0..20).map(|index| format!("skill-{index}")).collect();
        assert_eq!(offered("skill", &many, false).len(), super::super::ROWS);
    }
}
