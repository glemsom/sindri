//! The `skill` tool: the catalog the model may see, said in the tool's own
//! description, and one skill loaded into the conversation once. The catalog is
//! built at boot and read from the schema while the model is choosing, so a
//! workspace with no skill the model may see registers no tool at all and the
//! preamble stays exactly what it was.

use std::collections::BTreeSet;
use std::fmt;
use std::sync::Mutex;
use std::time::Instant;

use crate::provider::{Call, ToolFacts, ToolOutcome, ToolSpec};
use crate::skills::{Catalog, Skill};

/// The most files one activation lists. A skill directory is not the place to
/// spend a run's budget, and a file past this one is still reachable by name.
const LISTED: usize = 200;

/// Cloned rather than rebuilt: a prompt is handed the schema and the catalog
/// the boot decided on, not a second derivation of them.
#[derive(Clone)]
pub struct SkillTool {
    spec: ToolSpec,
    /// Only the skills the model may see: a user-only skill is in neither the
    /// schema nor the catalog, because a permission the call site does not hold
    /// is a permission nobody holds.
    skills: Vec<Skill>,
}

impl SkillTool {
    /// The tool for a catalog, or `None` when it holds no skill the model may
    /// see. Built once, with the description it will be handed to the model
    /// with: the catalog is a fact about the tree, not about a turn.
    pub fn new(catalog: &Catalog) -> Option<SkillTool> {
        let skills: Vec<Skill> = catalog
            .skills
            .iter()
            .filter(|skill| skill.model_invocable)
            .cloned()
            .collect();
        if skills.is_empty() {
            return None;
        }
        Some(SkillTool {
            spec: describe(&skills),
            skills,
        })
    }

    pub fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    /// One activation. The body is read before the name is claimed, so a skill
    /// that cannot be read is answered as unreadable rather than as loaded; the
    /// claim is what makes two identical calls in one turn's wave one load.
    pub fn call(&self, call: &Call, name: &str, loaded: &Loaded) -> ToolOutcome {
        let Some(skill) = self.skills.iter().find(|skill| skill.name == name) else {
            return ToolOutcome::malformed(call, &self.unknown(name));
        };
        let started = Instant::now();
        let body = match skill.body() {
            Ok(body) => body,
            Err(problem) => return refused(call, skill, &problem, started),
        };
        // The word the content opens with is the word the fields carry: one
        // result, two spellings, derived here so they cannot disagree.
        let (content, outcome) = if loaded.claim(name) {
            (activation(skill, &body), "loaded")
        } else {
            (
                format!("outcome: already_loaded\nskill: {name}\n"),
                "already_loaded",
            )
        };
        ToolOutcome::new(call, content, false).with_facts(ToolFacts {
            outcome: Some(outcome.to_string()),
            duration_ms: Some(millis(started)),
            ..ToolFacts::default()
        })
    }

    /// The names the schema offers, which is the set a call may take.
    fn names(&self) -> Vec<&str> {
        self.skills
            .iter()
            .map(|skill| skill.name.as_str())
            .collect()
    }

    /// A name the catalog does not hold: a hallucination is answered with the
    /// names that would have worked, so the next turn can take one of them.
    fn unknown(&self, name: &str) -> String {
        let valid: Vec<String> = self
            .names()
            .iter()
            .map(|name| format!("`{name}`"))
            .collect();
        format!(
            "there is no skill called `{name}`: `skill` takes one of {}",
            valid.join(", ")
        )
    }
}

impl fmt::Debug for SkillTool {
    /// The spec is a paragraph and the catalog a list of paths: a test that
    /// prints the tool prints what it offers, not the prose it offers it in.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SkillTool")
            .field("skills", &self.names())
            .finish_non_exhaustive()
    }
}

/// The doctrine and the catalog, in the one place a model reads while it is
/// choosing a tool. Every word is paid for on every turn of every run that has
/// a skill, so it says only what the schema and the result cannot.
fn describe(skills: &[Skill]) -> ToolSpec {
    let mut description = String::from(
        "Loads one skill — instructions written for you — into this conversation. Call it \
         before acting when a task is one it covers: the result is the skill's body with its \
         frontmatter stripped, together with the skill's directory and the files beside the \
         body, so a relative reference in the body resolves.\n\
         A skill is loaded once per conversation: a second call answers `already_loaded` and \
         no body, because a result is carried in the conversation for the rest of the run and \
         sent again with every later turn. A body is charged to the run's budget as any other \
         result is.\n\
         Skills:\n",
    );
    for skill in skills {
        description.push_str(&format!("- {}: {}\n", skill.name, skill.description));
    }
    ToolSpec {
        name: "skill".to_string(),
        description,
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "enum": skills.iter().map(|skill| skill.name.clone()).collect::<Vec<_>>(),
                    "description": "The skill to load, from the catalog above.",
                },
            },
            "required": ["name"],
        }),
    }
}

/// What the model is handed for a load: the body in a named wrapper, and beside
/// it where the skill lives and what is in there, so `references/api.md` in the
/// body is a file the model can open with `bash`.
fn activation(skill: &Skill, body: &str) -> String {
    let files = skill.listing();
    let mut out = format!(
        "outcome: loaded\nskill: {}\ndir: {}\nfiles:\n",
        skill.name,
        skill.dir.display()
    );
    for file in files.iter().take(LISTED) {
        out.push_str(&format!("  {file}\n"));
    }
    if let Some(more) = files.len().checked_sub(LISTED).filter(|more| *more > 0) {
        out.push_str(&format!("  ({more} more)\n"));
    }
    out.push_str(&format!(
        "<skill_content name=\"{}\">\n{body}\n</skill_content>\n",
        skill.name
    ));
    out
}

/// A skill whose file could not be read: the call ran, so it is a result and
/// not a refusal of the call, and the model is told which file and why.
fn refused(call: &Call, skill: &Skill, problem: &str, started: Instant) -> ToolOutcome {
    let content = format!(
        "outcome: unreadable\nskill: {}\ndir: {}\nproblem: {problem}\n",
        skill.name,
        skill.dir.display()
    );
    ToolOutcome::new(call, content, true).with_facts(ToolFacts {
        outcome: Some("unreadable".to_string()),
        duration_ms: Some(millis(started)),
        ..ToolFacts::default()
    })
}

fn millis(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// The bodies this conversation has received. It lives with the agent and is
/// cleared with the history, because activation is a fact about a conversation
/// and not about a process: `/new` starts a history that was told nothing, and
/// a set that outlived it would say a skill is in force in a conversation that
/// never received it.
#[derive(Debug, Default)]
pub struct Loaded {
    names: Mutex<BTreeSet<String>>,
}

impl Loaded {
    /// Whether this call is the one that loads the name. The first caller wins
    /// and every other — including a twin call in the same wave of one turn —
    /// reads the name as taken, so the wave leaves one body behind.
    fn claim(&self, name: &str) -> bool {
        self.names
            .lock()
            .map(|mut names| names.insert(name.to_string()))
            .unwrap_or(false)
    }

    pub fn clear(&self) {
        if let Ok(mut names) = self.names.lock() {
            names.clear();
        }
    }

    /// What is in force in this conversation, in name order: the frame's ledger
    /// and a test read the same set the tool wrote.
    pub fn names(&self) -> Vec<String> {
        self.names
            .lock()
            .map(|names| names.iter().cloned().collect())
            .unwrap_or_default()
    }
}
