use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// The declaration order is the precedence order, so a comparison of two scopes says
/// which one wins.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Scope {
    Builtin,
    User,
    Repo,
}

impl Scope {
    pub fn word(self) -> &'static str {
        match self {
            Scope::Builtin => "builtin",
            Scope::User => "user",
            Scope::Repo => "repo",
        }
    }
}

impl std::fmt::Display for Scope {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.word())
    }
}

/// Kept positive, so no caller reads the frontmatter's negations: an absent flag means the
/// door is open, and only a plain boolean moves it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Invocation {
    model: bool,
    human: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub scope: Scope,
    pub directory: Option<PathBuf>,
    pub body: String,
    invocation: Invocation,
}

impl Skill {
    pub fn model_invocable(&self) -> bool {
        self.invocation.model
    }

    pub fn human_invocable(&self) -> bool {
        self.invocation.human
    }

    /// A directory is descended; a link is listed and never followed, because a link to
    /// a parent would walk for ever, while a link to a file is exactly what listing one
    /// is for. A builtin has no directory and so no files.
    pub fn files(&self) -> Vec<String> {
        let Some(directory) = &self.directory else {
            return Vec::new();
        };

        let mut files = Vec::new();
        let mut pending = vec![directory.clone()];
        while let Some(next) = pending.pop() {
            let entries = match std::fs::read_dir(&next) {
                Ok(entries) => entries,
                Err(_) => continue,
            };
            for entry in entries.flatten() {
                let path = entry.path();
                let relative = path
                    .strip_prefix(directory)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .into_owned();
                match entry.file_type() {
                    Ok(kind) if kind.is_dir() => pending.push(path),
                    Ok(_) => files.push(relative),
                    Err(_) => continue,
                }
            }
        }
        files.sort();
        files
    }

    /// The body in a tag naming the skill, behind a line naming the scope it came from, and
    /// the files beside its `SKILL.md` so a relative reference in the body resolves.
    pub fn wrapper(&self) -> String {
        let mut text = format!(
            "The `{}` skill was loaded, from the {} scope.\n\n<skill name=\"{}\">\n{}\n</skill>\n",
            self.name,
            self.scope,
            self.name,
            self.body.trim_end()
        );
        match &self.directory {
            Some(directory) => {
                text.push_str(&format!(
                    "\nFiles in this skill's directory, `{}`, so a relative reference in the \
                     body above resolves from there:\n",
                    directory.display()
                ));
                for file in self.files() {
                    text.push_str("- `");
                    text.push_str(&file);
                    text.push_str("`\n");
                }
            }
            None => text.push_str(
                "\nThis skill is compiled into the binary, so it has no files beside its \
                 `SKILL.md`.\n",
            ),
        }
        text
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shadowing {
    pub name: String,
    pub winner: Scope,
    pub winner_path: Option<PathBuf>,
    pub loser: Scope,
    pub loser_path: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub path: PathBuf,
    pub line: usize,
    pub message: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Catalog {
    skills: Vec<Skill>,
    shadowing: Vec<Shadowing>,
    diagnostics: Vec<Diagnostic>,
}

impl Catalog {
    pub fn discover(workspace: &Path, home: Option<&Path>) -> Catalog {
        Catalog::discover_with_builtin(workspace, home, BUILTIN_SKILLS)
    }

    pub fn discover_with_builtin(
        workspace: &Path,
        home: Option<&Path>,
        builtin: &[&'static str],
    ) -> Catalog {
        let mut catalog = Catalog::default();
        let mut kept: ByName = BTreeMap::new();

        for (scope, source) in sources(workspace, home, builtin) {
            for skill in source.offerings(scope, &mut catalog) {
                keep(skill, &mut kept, &mut catalog);
            }
        }

        catalog.skills = kept.into_values().collect();
        catalog.shadowing.sort_by(|a, b| {
            (&a.name, a.loser, &a.loser_path).cmp(&(&b.name, b.loser, &b.loser_path))
        });
        catalog
            .diagnostics
            .sort_by(|a, b| (&a.path, a.line).cmp(&(&b.path, b.line)));
        catalog
    }

    pub fn skills(&self) -> &[Skill] {
        &self.skills
    }

    pub fn shadowing(&self) -> &[Shadowing] {
        &self.shadowing
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    pub fn is_empty(&self) -> bool {
        self.skills.is_empty()
    }

    pub fn get(&self, name: &str) -> Option<&Skill> {
        self.skills.iter().find(|skill| skill.name == name)
    }

    /// The `skill` tool's description: the catalog as the model may name it. A skill whose
    /// frontmatter says `disable-model-invocation: true` is not here at all, because the
    /// description *is* the schema; `None` when no skill is left to name.
    pub fn tool_description(&self) -> Option<String> {
        let listing: Vec<Listing> = self
            .listing()
            .into_iter()
            .filter(|entry| entry.model_invocable)
            .collect();
        if listing.is_empty() {
            return None;
        }

        let mut lines = vec![
            String::from(
                "Load a skill's instructions by name. A call names one value from the catalog \
                 below, and the load returns those instructions with their frontmatter stripped.",
            ),
            String::new(),
            String::from("Skills in force:"),
        ];
        for entry in &listing {
            lines.push(String::new());
            lines.push(format!("- `{}` — {}", entry.name, entry.description));
        }
        Some(lines.join("\n"))
    }

    pub fn listing(&self) -> Vec<Listing> {
        self.skills
            .iter()
            .map(|skill| Listing {
                name: skill.name.clone(),
                description: skill.description.clone(),
                scope: skill.scope,
                model_invocable: skill.model_invocable(),
                human_invocable: skill.human_invocable(),
                shadows: self
                    .shadowing
                    .iter()
                    .filter(|shadow| shadow.name == skill.name)
                    .cloned()
                    .collect(),
            })
            .collect()
    }

    pub fn invocable_by_human(&self) -> Vec<Listing> {
        self.listing()
            .into_iter()
            .filter(|entry| entry.human_invocable)
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listing {
    pub name: String,
    pub description: String,
    pub scope: Scope,
    pub model_invocable: bool,
    pub human_invocable: bool,
    pub shadows: Vec<Shadowing>,
}

type ByName = BTreeMap<String, Skill>;

/// `include_str!` bakes the text in, so deleting the directory afterwards changes
/// nothing about the skills this build offers; the directory is where it is edited,
/// never where it is read.
const BUILTIN_SKILLS: &[&str] = &[include_str!("../skills/commit-message/SKILL.md")];

enum Source {
    Embedded(&'static str),
    Directory(PathBuf),
}

impl Source {
    fn offerings(&self, scope: Scope, catalog: &mut Catalog) -> Vec<Skill> {
        match self {
            Source::Embedded(text) => match parse(text, None, scope) {
                Ok(skill) => vec![skill],
                Err(refusal) => {
                    let path = Path::new("<builtin>");
                    catalog.diagnostics.push(refusal.diagnostic(path));
                    Vec::new()
                }
            },
            Source::Directory(directory) => directory_skills(directory, scope, catalog),
        }
    }
}

fn sources(
    workspace: &Path,
    home: Option<&Path>,
    builtin: &[&'static str],
) -> Vec<(Scope, Source)> {
    let mut sources: Vec<(Scope, Source)> = builtin
        .iter()
        .map(|text| (Scope::Builtin, Source::Embedded(text)))
        .collect();

    if let Some(home) = home {
        sources.push((Scope::User, Source::Directory(home.join(".agents/skills"))));
    }

    let root = repository_root(workspace);
    let mut directories = Vec::new();
    let mut cursor = Some(workspace);
    while let Some(directory) = cursor {
        directories.push(directory);
        if directory == root {
            break;
        }
        cursor = directory.parent();
    }
    sources.extend(directories.into_iter().rev().map(|directory| {
        (
            Scope::Repo,
            Source::Directory(directory.join(".agents/skills")),
        )
    }));
    sources
}

/// The nearest directory holding a `.git`, which is a file in a worktree and a directory
/// in a clone; a workspace in no repository is its own root, since walking further would
/// hand a project whatever it sits inside. The preamble reads `AGENTS.md` here too.
pub(crate) fn repository_root(workspace: &Path) -> PathBuf {
    let mut cursor = Some(workspace);
    while let Some(directory) = cursor {
        if directory.join(".git").exists() {
            return directory.to_path_buf();
        }
        cursor = directory.parent();
    }
    workspace.to_path_buf()
}

const SKILL_FILE: &str = "SKILL.md";

const FENCE: &str = "---";

fn directory_skills(directory: &Path, scope: Scope, catalog: &mut Catalog) -> Vec<Skill> {
    let entries = match std::fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(error) => {
            catalog.diagnostics.push(Diagnostic {
                path: directory.to_path_buf(),
                line: 1,
                message: format!("cannot be read: {error}"),
            });
            return Vec::new();
        }
    };

    let mut children: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .collect();
    children.sort();

    let mut skills = Vec::new();
    for child in children {
        if let Some(skill) = child_skill(&child, scope, catalog) {
            skills.push(skill);
        }
    }
    skills
}

fn child_skill(child: &Path, scope: Scope, catalog: &mut Catalog) -> Option<Skill> {
    let target = match std::fs::metadata(child) {
        Ok(target) => target,
        Err(error) => {
            let link = std::fs::symlink_metadata(child).is_ok_and(|meta| meta.is_symlink());
            let message = match (link, error.kind()) {
                (true, std::io::ErrorKind::NotFound) => {
                    String::from("this link leads nowhere, so there is no SKILL.md to read")
                }
                _ => format!("cannot be read: {error}"),
            };
            catalog.diagnostics.push(Diagnostic {
                path: child.to_path_buf(),
                line: 1,
                message,
            });
            return None;
        }
    };

    if !target.is_dir() {
        // A link to a file is a mistake worth naming; a plain file beside the skills — a
        // README, a licence — is simply not a skill.
        if std::fs::symlink_metadata(child).is_ok_and(|meta| meta.is_symlink()) {
            catalog.diagnostics.push(Diagnostic {
                path: child.to_path_buf(),
                line: 1,
                message: String::from("this link leads to a file, not to a skill directory"),
            });
        }
        return None;
    }

    let file = child.join(SKILL_FILE);
    let text = match std::fs::read_to_string(&file) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        Err(error) => {
            catalog.diagnostics.push(Diagnostic {
                path: file,
                line: 1,
                message: format!("cannot be read: {error}"),
            });
            return None;
        }
    };

    match parse(&text, Some(child.to_path_buf()), scope) {
        Ok(skill) => Some(skill),
        Err(refusal) => {
            catalog.diagnostics.push(refusal.diagnostic(&file));
            None
        }
    }
}

/// The frontmatter a `SKILL.md` must carry. A file that names neither flag leaves both doors
/// open, while a flag written as a string — `"yes"`, `"true"` — is a diagnostic naming the
/// line, never a value read as absent.
#[derive(Deserialize)]
struct Frontmatter {
    name: String,
    description: String,
    #[serde(default, rename = "disable-model-invocation")]
    disable_model_invocation: Option<bool>,
    #[serde(default, rename = "user-invocable")]
    user_invocable: Option<bool>,
}

impl Frontmatter {
    fn invocation(&self) -> Invocation {
        Invocation {
            model: !self.disable_model_invocation.unwrap_or(false),
            human: self.user_invocable.unwrap_or(true),
        }
    }
}

struct Refusal {
    line: usize,
    message: String,
}

impl Refusal {
    fn diagnostic(&self, path: &Path) -> Diagnostic {
        Diagnostic {
            path: path.to_path_buf(),
            line: self.line,
            message: self.message.clone(),
        }
    }
}

/// The parser's complaint without its own line and column, which count from the
/// frontmatter rather than the file and would contradict the line the diagnostic names.
fn problem(message: &str) -> String {
    match message.find(" at line ") {
        Some(offset) => message[..offset].to_owned(),
        None => message.to_owned(),
    }
}

fn parse(text: &str, directory: Option<PathBuf>, scope: Scope) -> Result<Skill, Refusal> {
    let lines: Vec<&str> = text.lines().collect();
    let refusal = |line: usize, message: String| Refusal { line, message };

    if lines.first().map(|line| line.trim_end()) != Some(FENCE) {
        return Err(refusal(
            1,
            String::from("no frontmatter: a SKILL.md opens with a `---` line"),
        ));
    }

    let closing = lines
        .iter()
        .enumerate()
        .skip(1)
        .find(|(_, line)| line.trim_end() == FENCE)
        .map(|(index, _)| index);
    let Some(closing) = closing else {
        return Err(refusal(
            1,
            String::from("the frontmatter that opens here is never closed with a `---` line"),
        ));
    };

    let yaml = lines[1..closing].join("\n");
    let frontmatter: Frontmatter = serde_norway::from_str(&yaml).map_err(|error| {
        // The frontmatter starts on the file's second line, so the parser's own
        // one-based line needs that one added to name the file's line.
        let line = error.location().map_or(2, |location| location.line() + 1);
        refusal(line, problem(&error.to_string()))
    })?;

    let body = lines[closing + 1..]
        .join("\n")
        .trim_start_matches('\n')
        .to_owned();

    Ok(Skill {
        invocation: frontmatter.invocation(),
        name: frontmatter.name,
        description: frontmatter.description,
        scope,
        directory,
        body,
    })
}

fn keep(skill: Skill, kept: &mut ByName, catalog: &mut Catalog) {
    if let Some(loser) = kept.get(&skill.name) {
        catalog.shadowing.push(Shadowing {
            name: skill.name.clone(),
            winner: skill.scope,
            winner_path: skill.directory.clone(),
            loser: loser.scope,
            loser_path: loser.directory.clone(),
        });
    }
    kept.insert(skill.name.clone(), skill);
}
