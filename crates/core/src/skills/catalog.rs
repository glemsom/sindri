//! The values a catalog is made of: one skill as it was read, one file the scan
//! refused, one name both scopes carried.

use std::path::PathBuf;

/// Where a skill was found. The two scopes are the only two places a scan
/// looks, and a repo skill wins a name they both carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    User,
    Repo,
}

impl Scope {
    /// The word `sindri doctor` prints for it.
    pub fn as_str(self) -> &'static str {
        match self {
            Scope::User => "user",
            Scope::Repo => "repo",
        }
    }
}

/// One skill directory as it was read: the two fields the specification
/// requires, the two invocation flags, and where it lives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub scope: Scope,
    /// Whether the model may see it in the catalog: `disable-model-invocation`.
    pub model_invocable: bool,
    /// Whether a human may reach for it: `user-invocable`.
    pub user_invocable: bool,
    pub dir: PathBuf,
}

impl Skill {
    /// `model,user`, `model`, `user` or — a skill nobody may run — `none`.
    pub fn invocation(&self) -> String {
        match (self.model_invocable, self.user_invocable) {
            (true, true) => "model,user".to_string(),
            (true, false) => "model".to_string(),
            (false, true) => "user".to_string(),
            (false, false) => "none".to_string(),
        }
    }
}

/// A file the scan would not load, and the line that broke it: a skill is
/// skipped out loud, never silently.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub path: PathBuf,
    /// The line the refusal is about, when there is one: a directory that is a
    /// broken link has no line to point at.
    pub line: Option<usize>,
    pub reason: String,
}

/// One name two scopes carried: the skill that lost it and the one that holds
/// it, as the directories they live in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shadow {
    pub name: String,
    /// The skill that was left out.
    pub loser: PathBuf,
    /// The nearer skill that displaced it.
    pub winner: PathBuf,
}

/// Everything one scan found: what may run, what would not load, and what was
/// displaced. Sorted by name, so two scans of one tree say the same thing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Catalog {
    pub skills: Vec<Skill>,
    pub refused: Vec<Refusal>,
    pub shadowed: Vec<Shadow>,
}
