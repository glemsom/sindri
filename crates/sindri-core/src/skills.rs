//! Skills are discovered across three scopes and resolved into one catalog, with
//! every shadowing reported rather than silently resolved.
//!
//! The scopes are ordered lowest precedence first: `builtin` skills compiled into
//! the binary, `user` skills under `~/.agents/skills`, and `repo` skills from
//! every `.agents/skills` between the workspace and the repository root. A nearer
//! scope takes a name from a further one and the loser is named in the report, so
//! a person can see which of two `SKILL.md` files never loads.
//!
//! Discovery never walks above the repository root — the directory holding
//! `.git` — so a checkout inside another project cannot inherit that project's
//! skills. A directory holding a `SKILL.md` is a skill and a directory symlink is
//! followed, because sharing one is what a symlink is for; a link whose target is
//! gone is refused by its own path. A `SKILL.md` that will not parse is a
//! diagnostic naming the line, never a failed catalog and never silence: the rest
//! of the catalog still loads.
//!
//! Who may reach a skill is two YAML booleans in its frontmatter, `§5`'s
//! `disable-model-invocation` and `user-invocable`, and both are read as plain
//! `true`/`false`: a flag written as a string is a diagnostic, never a default.
//! The catalog keeps every skill; the model's view of it and the person's view
//! are derived from those flags, so neither view decides again.
//!
//! The catalog is data, not a printed line: `doctor` and the rail both read it.
//! It is empty rather than absent when nothing is found, and a skill with no path
//! is one that was compiled in.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Where a skill came from, lowest precedence first. The declaration order is the
/// precedence order, so a comparison of two scopes says which one wins.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Scope {
    /// Compiled into the binary, so it cannot be absent, stale or unwritable.
    Builtin,
    /// `~/.agents/skills`: the person's own, whatever the workspace.
    User,
    /// A `.agents/skills` between the workspace and the repository root.
    Repo,
}

impl Scope {
    /// The word for this scope: what `doctor` prints and a record holds.
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

/// Who may reach one skill, read from §5's two Claude-compatible YAML booleans
/// and kept positive: the frontmatter states what a door is closed to, and the
/// rest of the catalog asks what a door is open to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Invocation {
    /// The model may call it: `disable-model-invocation` absent or false.
    model: bool,
    /// A person may ask for it: `user-invocable` absent or true.
    human: bool,
}

/// One skill the catalog holds: what a call names, what it is for, where it came
/// from, and the instructions a load hands back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Skill {
    /// The name a call uses, read from the frontmatter rather than the directory.
    pub name: String,
    /// The one line the catalog shows beside the name.
    pub description: String,
    /// Where the skill came from, which is what precedence is decided by.
    pub scope: Scope,
    /// The directory holding its `SKILL.md`, or `None` for a builtin, which has
    /// no path at all.
    pub directory: Option<PathBuf>,
    /// The instructions: the `SKILL.md` with its frontmatter stripped.
    pub body: String,
    /// Who may reach this skill, decided once when the frontmatter was read and
    /// readable only through [`Skill::model_invocable`] and
    /// [`Skill::human_invocable`], so no caller reads the YAML's negations.
    invocation: Invocation,
}

impl Skill {
    /// Whether the model may call this skill: `disable-model-invocation: true`
    /// takes it off the model's side, and the tool's description — which is the
    /// schema — is then built without it (§5, §2's absence over refusal).
    pub fn model_invocable(&self) -> bool {
        self.invocation.model
    }

    /// Whether a person may ask for this skill by name: `user-invocable: false`
    /// keeps it out of what the frame offers for completion, while the model
    /// still calls it. This is where that answer is decided, so the frame reads
    /// a decision instead of making one.
    pub fn human_invocable(&self) -> bool {
        self.invocation.human
    }

    /// Every file beside this skill's `SKILL.md`, as a path relative to its
    /// directory, in path order: what a load lists so a relative reference in
    /// the body resolves.
    ///
    /// The walk starts at the directory discovery found the skill in, so the
    /// list and the skill cannot disagree about which `SKILL.md` was read. A
    /// directory is descended; a link is listed and never followed, because a
    /// link to a parent would walk for ever, while a link to a file is exactly
    /// what listing one is for. A builtin has no directory and so no files.
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

    /// The wrapper a load hands the model: the body with its frontmatter
    /// stripped, inside a tag naming the skill, and the files beside its
    /// `SKILL.md` listed so a relative reference in the body resolves (§5).
    ///
    /// The body is handed over once and never again: a tool result is re-sent
    /// with every later turn, so a second load answers `already_loaded` instead
    /// of repeating what the model already has (§5).
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

/// A name two skills offer, and which of them the catalog kept.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shadowing {
    /// The name both skills offer.
    pub name: String,
    /// The scope whose skill took the name.
    pub winner: Scope,
    /// Where the winning skill lies, or `None` when it was compiled in.
    pub winner_path: Option<PathBuf>,
    /// The scope whose skill lost the name and never loads.
    pub loser: Scope,
    /// Where the losing skill lies, or `None` when it was compiled in.
    pub loser_path: Option<PathBuf>,
}

/// Something discovery refused, named by the file it is in and the line it is on,
/// so a person can go and fix it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    /// The file the trouble is in: a `SKILL.md`, or a link that leads nowhere.
    pub path: PathBuf,
    /// The line to open the file at, counting from one.
    pub line: usize,
    /// What is wrong, in the words a person would use.
    pub message: String,
}

/// Every skill the three scopes offer, resolved to one skill per name, with the
/// losers and the refusals kept beside them as data.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Catalog {
    skills: Vec<Skill>,
    shadowing: Vec<Shadowing>,
    diagnostics: Vec<Diagnostic>,
}

impl Catalog {
    /// Discovers the skills `workspace` and `home` offer, the way a run does.
    pub fn discover(workspace: &Path, home: Option<&Path>) -> Catalog {
        Catalog::discover_with_builtin(workspace, home, BUILTIN_SKILLS)
    }

    /// Discovers the same three scopes, with the built-ins handed in rather than
    /// compiled in. This is the seam a build fills with `include_str!` artefacts
    /// and the seam a test proves the builtin scope's precedence at.
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

    /// Every skill in force, one per name, in name order.
    pub fn skills(&self) -> &[Skill] {
        &self.skills
    }

    /// Every name a nearer scope took from a further one.
    pub fn shadowing(&self) -> &[Shadowing] {
        &self.shadowing
    }

    /// Everything discovery refused, in path order.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Whether the catalog holds no skill at all, for either door: an empty
    /// catalog is what `doctor` prints as none found.
    pub fn is_empty(&self) -> bool {
        self.skills.is_empty()
    }

    /// The skill a name names, or `None` when the catalog does not hold it.
    pub fn get(&self, name: &str) -> Option<&Skill> {
        self.skills.iter().find(|skill| skill.name == name)
    }

    /// The `skill` tool's description: the catalog in force as the model may name
    /// it, rendered from `listing()` — the very value `doctor` prints — so the two
    /// renderings differ by the rows a flag withholds and never by a word of
    /// wording.
    ///
    /// A skill whose frontmatter says `disable-model-invocation: true` is not
    /// here at all, because the description *is* the schema (§5, §2). `None` when
    /// no skill is left to name: a tool with nothing to name is not registered at
    /// all, rather than registered to say it has nothing.
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

    /// The catalog as lines, in name order: the shape `doctor` prints, so the
    /// command owns the printing and this owns what is printed. A builtin has no
    /// path to show, which is why `scope` is the word that names it.
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

    /// The rows a person may ask for by name, in name order: what the frame
    /// offers for `/` completion and what `/skill:name` accepts, decided here from
    /// `user-invocable: false` so the frame is handed a decision rather than the
    /// flags to decide with.
    pub fn invocable_by_human(&self) -> Vec<Listing> {
        self.listing()
            .into_iter()
            .filter(|entry| entry.human_invocable)
            .collect()
    }
}

/// One line of the catalog as `doctor` prints it: what a call names, the line
/// shown beside it, the scope it came from, and every skill it took the name
/// from, so a displaced skill is visible rather than merely gone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listing {
    /// The name a call uses.
    pub name: String,
    /// The one line shown beside the name.
    pub description: String,
    /// Where the skill in force came from.
    pub scope: Scope,
    /// Whether the model may call this skill, so a row says which door it is
    /// behind without the reader going back to the frontmatter.
    pub model_invocable: bool,
    /// Whether a person may ask for this skill by name.
    pub human_invocable: bool,
    /// Every skill this one took the name from, in the order discovery reported
    /// them, each naming the scope and the path that lost.
    pub shadows: Vec<Shadowing>,
}

/// The map a resolution is built in, kept in one place so every scope inserts the
/// same way.
type ByName = BTreeMap<String, Skill>;

/// The `SKILL.md` texts compiled into the binary, and the only way a builtin
/// skill exists: no path, because a path can be absent, stale or unwritable.
/// The directory each one is written in is where it is edited, never where it is
/// read: `include_str!` bakes the text in, so deleting the directory afterwards
/// changes nothing about the skills this build offers.
const BUILTIN_SKILLS: &[&str] = &[
    // One commit message, written from the diff in the repository's own style.
    include_str!("../skills/commit-message/SKILL.md"),
];

/// One place discovery reads skills from, as the precedence order visits it:
/// lowest first, so a later skill takes a name from an earlier one.
enum Source {
    /// A `SKILL.md` compiled into the binary, which has no directory.
    Embedded(&'static str),
    /// A `.agents/skills` directory, whose children may be skills.
    Directory(PathBuf),
}

impl Source {
    /// Every skill this source offers, recording in `catalog` whatever it had to
    /// refuse. A directory that is not there offers nothing: a scope nobody has
    /// used is not a fault.
    fn offerings(&self, scope: Scope, catalog: &mut Catalog) -> Vec<Skill> {
        match self {
            Source::Embedded(text) => match parse(text, None, scope) {
                Ok(skill) => vec![skill],
                Err(refusal) => {
                    // A built-in has no file to point at, so the refusal names
                    // the artefact instead.
                    let path = Path::new("<builtin>");
                    catalog.diagnostics.push(refusal.diagnostic(path));
                    Vec::new()
                }
            },
            Source::Directory(directory) => directory_skills(directory, scope, catalog),
        }
    }
}

/// The sources, in increasing precedence: the built-ins, then the home, then the
/// repository's `.agents/skills` directories from the root down to the workspace,
/// which leaves the nearest one to win.
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

/// The repository root a workspace sits under: the nearest directory holding a
/// `.git`, which is a file in a worktree and a directory in a clone. A workspace
/// in no repository at all is its own root, because walking to the filesystem
/// root would hand a project the skills of whatever it happens to sit inside.
///
/// The preamble reads the repository's `AGENTS.md` at this same root, so §7's
/// scopes and §8's instructions stop at one directory rather than two.
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

/// The file a skill directory is recognised by, and the only one discovery reads.
const SKILL_FILE: &str = "SKILL.md";

/// The fence a `SKILL.md` opens with and closes its frontmatter with.
const FENCE: &str = "---";

/// Every skill the children of one `.agents/skills` directory offer, in name
/// order so two runs agree.
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

/// The skill one child of a `.agents/skills` directory holds, or `None` when it
/// holds none — with whatever was refused recorded in `catalog`.
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
        // A link to a file is a mistake worth naming; a plain file beside the
        // skills — a README, a licence — is simply not a skill.
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

/// The frontmatter a `SKILL.md` must carry. Unknown keys are left alone: a skill
/// is free to hold keys this build has no use for.
///
/// The two permission flags are typed as the booleans §5 says they are, so a
/// value written as a string — `"yes"`, `"true"` — is a diagnostic naming the
/// line rather than a flag silently read as absent. Absent is the permissive
/// default: a `SKILL.md` that says nothing about them is both doors open.
#[derive(Deserialize)]
struct Frontmatter {
    /// The name a call uses.
    name: String,
    /// The one line the catalog shows beside the name.
    description: String,
    /// §5's user-only flag, taken away from the model when it is `true`.
    #[serde(default, rename = "disable-model-invocation")]
    disable_model_invocation: Option<bool>,
    /// §5's model-only flag, taken away from the person when it is `false`.
    #[serde(default, rename = "user-invocable")]
    user_invocable: Option<bool>,
}

impl Frontmatter {
    /// Who may reach the skill this frontmatter describes: an absent flag leaves
    /// its door open, and only a plain boolean moves it.
    fn invocation(&self) -> Invocation {
        Invocation {
            model: !self.disable_model_invocation.unwrap_or(false),
            human: self.user_invocable.unwrap_or(true),
        }
    }
}

/// A `SKILL.md` discovery would not read, and the line to open it at.
struct Refusal {
    line: usize,
    message: String,
}

impl Refusal {
    /// The refusal as a diagnostic about `path`.
    fn diagnostic(&self, path: &Path) -> Diagnostic {
        Diagnostic {
            path: path.to_path_buf(),
            line: self.line,
            message: self.message.clone(),
        }
    }
}

/// The parser's complaint without its own line and column, which count from the
/// frontmatter rather than the file and would contradict the line the diagnostic
/// already names.
fn problem(message: &str) -> String {
    match message.find(" at line ") {
        Some(offset) => message[..offset].to_owned(),
        None => message.to_owned(),
    }
}

/// Reads one `SKILL.md`: its frontmatter names and describes it, and everything
/// under the closing fence is the body a load hands back.
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
        // one-based line needs that one line added to name the file's line.
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

/// Keeps `skill`, taking the name from whatever held it before and reporting the
/// loser. Sources are visited lowest precedence first, so the skill arriving
/// later is the nearer one and the one that wins.
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
