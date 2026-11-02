//! The `SKILL.md` frontmatter, read as a YAML value tree rather than a schema:
//! the specification's two required fields are taken, two extensions are
//! honoured, and everything else a skill carries is ignored instead of refused.

use yaml_rust::yaml::Hash;
use yaml_rust::{ScanError, Yaml, YamlLoader};

/// What the rest of the tree needs from a `SKILL.md`: the fields the catalog
/// says, and nothing of the body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frontmatter {
    pub name: String,
    pub description: String,
    pub model_invocable: bool,
    pub user_invocable: bool,
}

/// Why a `SKILL.md` was not a skill, and the line to look at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub line: usize,
    pub reason: String,
}

const FENCE: &str = "---";
/// The file's first line, and the one a problem with no line of its own is
/// blamed on: the fence is where the frontmatter is.
const FENCE_LINE: usize = 1;
const NAME: &str = "name";
const DESCRIPTION: &str = "description";
const DISABLE_MODEL_INVOCATION: &str = "disable-model-invocation";
const USER_INVOCABLE: &str = "user-invocable";
/// The block is what sits between the two fences, and it opens on the file's
/// second line: every line inside it is counted from there.
const BLOCK_OFFSET: usize = 2;

/// The frontmatter of a whole `SKILL.md`. A file that does not open with a
/// fence, whose fence is never closed, or whose block is not YAML is a problem
/// and never a panic: none of that is under our control.
pub fn parse(text: &str) -> Result<Frontmatter, Problem> {
    let lines: Vec<&str> = text.lines().collect();
    if lines.first().map(|line| line.trim_end()) != Some(FENCE) {
        return Err(problem(
            FENCE_LINE,
            "a skill opens with a `---` frontmatter fence, and this one does not",
        ));
    }
    let close = lines
        .iter()
        .skip(1)
        .position(|line| line.trim_end() == FENCE)
        .map(|at| at + 1)
        .ok_or_else(|| problem(FENCE_LINE, "the frontmatter is opened and never closed"))?;
    let block = &lines[1..close];

    let documents = YamlLoader::load_from_str(&block.join("\n"))
        .map_err(|error| yaml_problem(&error, close))?;
    let document = documents.into_iter().next().unwrap_or(Yaml::Null);
    let Yaml::Hash(fields) = document else {
        let reason = match document {
            Yaml::Null => "the frontmatter is empty",
            _ => "the frontmatter is not a mapping of keys",
        };
        return Err(problem(BLOCK_OFFSET, reason));
    };

    Ok(Frontmatter {
        name: required(&fields, NAME, block)?,
        description: required(&fields, DESCRIPTION, block)?,
        // The two extensions the specification does not have, and the reason
        // they are read as flags: a skill the model must not see is still a
        // skill a human may run, and the other way round. A flag the file does
        // not say plainly leaves the permission it grants in place.
        model_invocable: flag(&fields, DISABLE_MODEL_INVOCATION) != Some(true),
        user_invocable: flag(&fields, USER_INVOCABLE) != Some(false),
    })
}

/// The body under the fence: everything after the block's closing `---`, with
/// the blank line that separates the two and the file's trailing whitespace
/// taken off. `None` when the file has no fence that closes, which [`parse`]
/// has already refused — this is the second read of a file, not a second parse.
pub fn body(text: &str) -> Option<&str> {
    let mut lines = text.split_inclusive('\n');
    let first = lines.next()?;
    if first.trim_end() != FENCE {
        return None;
    }
    // Every line passed is counted, so what comes back is the file's own text
    // from the closing fence's end onwards, trimmed of the blank line between.
    let mut offset = first.len();
    for line in lines {
        offset += line.len();
        if line.trim_end() == FENCE {
            return Some(text.get(offset..)?.trim());
        }
    }
    None
}

/// A field the specification requires, as a string that says something. A key
/// that is there and says nothing is blamed on its own line; a key that is
/// absent is blamed on the fence the frontmatter opens at, which is the first
/// line a reader looks at when the answer is `there is no such key`.
fn required(fields: &Hash, key: &str, block: &[&str]) -> Result<String, Problem> {
    let at = line_of(block, key).unwrap_or(FENCE_LINE);
    match fields.get(&Yaml::String(key.to_string())) {
        Some(Yaml::String(value)) if !value.trim().is_empty() => Ok(value.trim().to_string()),
        Some(Yaml::String(_)) => Err(problem(
            at,
            &format!("`{key}` says nothing, and a skill is found by what it says"),
        )),
        Some(_) => Err(problem(at, &format!("`{key}` is not a string"))),
        None => Err(problem(
            FENCE_LINE,
            &format!("no `{key}`, which every skill must have"),
        )),
    }
}

/// A flag's value, and only when it is a boolean: absent, a string or a nested
/// map is `None` and leaves the default, rather than failing a skill the
/// specification would accept. A string `"true"` is not a boolean here because
/// a reader who wrote quotes asked for a quotation, not a switch.
fn flag(fields: &Hash, key: &str) -> Option<bool> {
    match fields.get(&Yaml::String(key.to_string())) {
        Some(Yaml::Boolean(value)) => Some(*value),
        _ => None,
    }
}

/// The file line that opens `key:` at the top level, so a diagnostic points
/// where a reader can look.
fn line_of(block: &[&str], key: &str) -> Option<usize> {
    let opening = format!("{key}:");
    block
        .iter()
        .position(|line| line.starts_with(&opening))
        .map(|at| at + BLOCK_OFFSET)
}

fn yaml_problem(error: &ScanError, close: usize) -> Problem {
    // yaml-rust states its own position inside the block it was handed; the
    // diagnostic states the file's line, so that suffix is dropped rather
    // than said twice.
    let shown = error.to_string();
    let reason = match shown.find(" at line ") {
        Some(at) => shown[..at].to_string(),
        None => shown,
    };
    let line = (error.marker().line() + BLOCK_OFFSET).min(close);
    problem(line, &format!("the frontmatter is not YAML: {reason}"))
}

fn problem(line: usize, reason: &str) -> Problem {
    Problem {
        line,
        reason: reason.to_string(),
    }
}
