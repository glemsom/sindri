//! The values a catalog is made of: one skill as it was read, one file the scan
//! refused, one name two scopes carried.

use std::fmt;
use std::path::PathBuf;

use super::frontmatter::Frontmatter;

/// Where a skill was found, ordered by nearness. A built-in is compiled into the
/// binary and is the lowest of the three; a user's own sits above it; and the
/// repository the run stands in is nearest of all. Of two scopes carrying one
/// name, the nearer holds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Builtin,
    User,
    Repo,
}

impl Scope {
    /// The word `sindri doctor` prints for it.
    pub fn as_str(self) -> &'static str {
        match self {
            Scope::Builtin => "builtin",
            Scope::User => "user",
            Scope::Repo => "repo",
        }
    }
}

/// What a skill's content is read out of: a directory a scan walked, or bytes
/// compiled into the binary. A built-in is the one skill with no directory, so
/// where its body lives is a value rather than a path that has to be absent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Content {
    /// The directory holding the `SKILL.md`, re-read at the moment of the call.
    Directory(PathBuf),
    /// The `SKILL.md` itself, and every file its body may name, compiled in
    /// beside the module that reads them.
    Compiled {
        source: &'static str,
        files: &'static [File],
    },
}

/// One file a body may name: the path the body names it by, and its own text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct File {
    pub path: &'static str,
    pub text: &'static str,
}

/// One skill as it was read: the two fields the specification requires, the two
/// invocation flags, and where it was found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub scope: Scope,
    /// Whether the model may see it in the catalog: `disable-model-invocation`.
    pub model_invocable: bool,
    /// Whether a human may reach for it: `user-invocable`.
    pub user_invocable: bool,
    /// Where the body and the files beside it are read from.
    pub content: Content,
}

impl Skill {
    /// One skill from the frontmatter that named it and the content behind its
    /// body. A scan and the built-in list are the two ways to have both, and
    /// neither assembles a skill of its own.
    pub(crate) fn new(read: Frontmatter, scope: Scope, content: Content) -> Skill {
        Skill {
            name: read.name,
            description: read.description,
            scope,
            model_invocable: read.model_invocable,
            user_invocable: read.user_invocable,
            content,
        }
    }

    /// `model,user`, `model`, `user` or — a skill nobody may run — `none`.
    pub fn invocation(&self) -> String {
        match (self.model_invocable, self.user_invocable) {
            (true, true) => "model,user".to_string(),
            (true, false) => "model".to_string(),
            (false, true) => "user".to_string(),
            (false, false) => "none".to_string(),
        }
    }

    /// This skill as a shadow names it.
    pub fn origin(&self) -> Origin {
        Origin {
            scope: self.scope,
            name: self.name.clone(),
            dir: self.dir().map(PathBuf::from),
        }
    }
}

/// A skill the scan or the built-in table would not load, and the line that
/// broke it: such a skill is skipped out loud, never silently.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    /// Where the skill it was about came from.
    pub at: Where,
    /// The line the refusal is about, when there is one: a directory that is a
    /// broken link has no line to point at.
    pub line: Option<usize>,
    pub reason: String,
}

/// Where a refusal came from: the file it was read out of, or — a built-in's
/// `SKILL.md` is compiled in and there is no file — the scope that carries it.
/// A refusal names what a reader can go and look at, and never a path the
/// binary has not got.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Where {
    File(PathBuf),
    Scope(Scope),
}

impl fmt::Display for Where {
    /// The path where there is one, and the scope's own word where there is not.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Where::File(path) => write!(formatter, "{}", path.display()),
            Where::Scope(scope) => formatter.write_str(scope.as_str()),
        }
    }
}

/// One skill as a shadow names it: the scope it was found in, the name it
/// carried, and the directory it lives in where it has one. A built-in has
/// none, and a shadow that invented a path for one would send a reader looking
/// for a file that is not there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Origin {
    pub scope: Scope,
    pub name: String,
    pub dir: Option<PathBuf>,
}

impl fmt::Display for Origin {
    /// The directory where there is one, and the scope's own word where there
    /// is not: what a reader needs to find a skill, and never a path it has not.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.dir {
            Some(dir) => write!(formatter, "{}", dir.display()),
            None => formatter.write_str(self.scope.as_str()),
        }
    }
}

/// One name two scopes carried: the skill that lost it and the one that holds
/// it, each by the scope and name it was found under — and by its directory
/// where it has one. The name is the winner's: two skills in a shadow are one
/// name, said once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shadow {
    /// The skill that was left out.
    pub loser: Origin,
    /// The nearer skill that displaced it.
    pub winner: Origin,
}

/// Everything one scan found: what may run, what would not load, and what was
/// displaced. Sorted by name, so two scans of one tree say the same thing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Catalog {
    pub skills: Vec<Skill>,
    pub refused: Vec<Refusal>,
    pub shadowed: Vec<Shadow>,
}
