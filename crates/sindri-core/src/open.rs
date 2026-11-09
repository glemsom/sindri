use std::error::Error;
use std::ffi::OsString;
use std::fmt;
use std::path::{Component, Path, PathBuf};

use serde_json::Value;

use crate::policy::Boundary;

/// `file:` is absent on purpose: it would hand the host's browser a local path no
/// judgement ever looked at.
pub const SCHEMES: [&str; 2] = ["http", "https"];

pub const AUTHORITY: &str = "localhost";

pub const EXTENSIONS: [&str; 12] = [
    "html", "htm", "pdf", "png", "jpg", "jpeg", "gif", "webp", "svg", "txt", "md", "csv",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Opening {
    Url(String),
    File(PathBuf),
}

impl Opening {
    /// The target as one argv word: never a shell string, never a word a shell could split.
    pub fn word(&self) -> OsString {
        match self {
            Opening::Url(url) => OsString::from(url),
            Opening::File(path) => path.as_os_str().to_os_string(),
        }
    }
}

impl fmt::Display for Opening {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Opening::Url(url) => formatter.write_str(url),
            Opening::File(path) => write!(formatter, "{}", path.display()),
        }
    }
}

/// Where a run stands when it asks to open something: the writable set, the session temp
/// and the directory a relative path is read from — the places held canonical and sorted,
/// so a judgement compares the filesystem's spelling, not a string a `..` can walk past.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ground {
    places: Vec<PathBuf>,
    cwd: PathBuf,
}

impl Ground {
    pub fn new(boundary: &Boundary, session_temp: &Path, cwd: &Path) -> Result<Ground, Refusal> {
        let mut places: Vec<PathBuf> = boundary.writable().paths().to_vec();
        places.push(resolve(session_temp, session_temp)?);
        places.sort();
        places.dedup();

        Ok(Ground {
            places,
            cwd: resolve(cwd, cwd)?,
        })
    }

    pub fn places(&self) -> &[PathBuf] {
        &self.places
    }

    pub fn judge(&self, arguments: &str) -> Result<Opening, Refusal> {
        let named: Value =
            serde_json::from_str(arguments.trim()).map_err(|error| Refusal::Unreadable {
                arguments: arguments.to_owned(),
                reason: error.to_string(),
            })?;

        let url = written(&named, "url");
        let path = written(&named, "path");
        match (url, path) {
            (Some(url), None) => self.url(url),
            (None, Some(path)) => self.file(path),
            (Some(url), Some(path)) => Err(Refusal::TwoTargets {
                url: url.to_owned(),
                path: path.to_owned(),
            }),
            (None, None) => Err(Refusal::NoTarget),
        }
    }

    fn url(&self, url: &str) -> Result<Opening, Refusal> {
        let (scheme, rest) = url.split_once(':').ok_or_else(|| Refusal::Scheme {
            url: url.to_owned(),
            scheme: String::new(),
        })?;
        if !SCHEMES
            .iter()
            .any(|allowed| scheme.eq_ignore_ascii_case(allowed))
        {
            return Err(Refusal::Scheme {
                url: url.to_owned(),
                scheme: scheme.to_owned(),
            });
        }

        let authority = match rest.strip_prefix("//") {
            Some(tail) => tail.split(['/', '?', '#']).next().unwrap_or_default(),
            None => "",
        };
        let host = authority
            .rsplit('@')
            .next()
            .unwrap_or_default()
            .split(':')
            .next()
            .unwrap_or_default();
        if !(host.is_empty() || host.eq_ignore_ascii_case(AUTHORITY)) {
            return Err(Refusal::Authority {
                url: url.to_owned(),
                authority: authority.to_owned(),
            });
        }

        Ok(Opening::Url(url.to_owned()))
    }

    fn file(&self, written: &str) -> Result<Opening, Refusal> {
        let path = Path::new(written);

        let as_written = extension(path);
        if !allowed(&as_written) {
            return Err(Refusal::Extension {
                path: written.to_owned(),
                extension: as_written,
            });
        }
        let absolute = if path.is_absolute() {
            lexical(path)
        } else {
            lexical(&self.cwd.join(path))
        };
        if !self.contains(&absolute) {
            return Err(Refusal::Outside {
                path: written.to_owned(),
                places: self.rendered(),
            });
        }

        // Resolved second: this is where a symlink to `/` stops being a `.html`
        // file inside the workspace.
        let resolved = resolve(&absolute, Path::new(written))?;
        if !self.contains(&resolved) {
            return Err(Refusal::Escapes {
                written: written.to_owned(),
                path: resolved,
                places: self.rendered(),
            });
        }
        let as_resolved = extension(&resolved);
        if !allowed(&as_resolved) {
            return Err(Refusal::ResolvedExtension {
                written: written.to_owned(),
                path: resolved,
                extension: as_resolved,
            });
        }
        let regular = std::fs::metadata(&resolved).is_ok_and(|meta| meta.is_file());
        if !regular {
            return Err(Refusal::NotRegular { path: resolved });
        }

        Ok(Opening::File(resolved))
    }

    fn contains(&self, path: &Path) -> bool {
        self.places.iter().any(|place| path.starts_with(place))
    }

    fn rendered(&self) -> Vec<String> {
        self.places
            .iter()
            .map(|place| place.display().to_string())
            .collect()
    }
}

pub fn description(boundary: &Boundary) -> String {
    let mut lines: Vec<String> = vec![
        String::from("Open one target in the browser on this machine."),
        String::new(),
        String::from("Name exactly one target; an empty value is the same as naming none."),
        format!(
            "  url  — a web page. The scheme must be {}, and the authority must be empty or `{AUTHORITY}`.",
            SCHEMES.join(" or ")
        ),
        String::from(
            "  path — a file. It must be a regular file inside a writable path or the session temp,",
        ),
        format!("         with extension {}.", EXTENSIONS.join(", ")),
        String::new(),
        String::from(
            "A path is read relative to where the run stands and then resolved, so a symlink that \
             points outside the writable paths is refused rather than followed.",
        ),
        String::new(),
        String::from("The writable paths in force are:"),
    ];
    for place in boundary.writable().paths() {
        lines.push(format!("  {}", place.display()));
    }
    lines.join("\n")
}

fn written<'a>(arguments: &'a Value, key: &str) -> Option<&'a str> {
    arguments
        .get(key)?
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn extension(path: &Path) -> Option<String> {
    path.extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
}

fn allowed(extension: &Option<String>) -> bool {
    extension
        .as_ref()
        .is_some_and(|extension| EXTENSIONS.contains(&extension.as_str()))
}

/// `path` with its `.` and `..` folded away and no filesystem read, so the string is
/// judged before anything is followed.
fn lexical(path: &Path) -> PathBuf {
    let mut folded = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                folded.pop();
            }
            Component::CurDir => {}
            other => folded.push(other),
        }
    }
    folded
}

fn resolve(path: &Path, written: &Path) -> Result<PathBuf, Refusal> {
    std::fs::canonicalize(path).map_err(|error| Refusal::Unresolvable {
        path: written.to_path_buf(),
        reason: error.to_string(),
    })
}

/// A refusal names the value it refused, and the allowlist where one applies, because a
/// model that cannot read the allowlist spends a turn guessing at it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    Unreadable {
        arguments: String,
        reason: String,
    },
    Unresolvable {
        path: PathBuf,
        reason: String,
    },
    NoTarget,
    TwoTargets {
        url: String,
        path: String,
    },
    Scheme {
        url: String,
        scheme: String,
    },
    Authority {
        url: String,
        authority: String,
    },
    Extension {
        path: String,
        extension: Option<String>,
    },
    ResolvedExtension {
        written: String,
        path: PathBuf,
        extension: Option<String>,
    },
    Outside {
        path: String,
        places: Vec<String>,
    },
    Escapes {
        written: String,
        path: PathBuf,
        places: Vec<String>,
    },
    NotRegular {
        path: PathBuf,
    },
}

impl Refusal {
    fn allowed_extensions() -> String {
        EXTENSIONS.join(", ")
    }

    fn allowed_schemes() -> String {
        SCHEMES.join(" and ")
    }
}

impl fmt::Display for Refusal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Refusal::Unreadable { reason, .. } => write!(
                formatter,
                "refused: the arguments are not the JSON this tool takes ({reason}); write \
                 `{{\"url\": \"…\"}}` or `{{\"path\": \"…\"}}`"
            ),
            Refusal::Unresolvable { path, reason } => write!(
                formatter,
                "refused: `{}` cannot be resolved: {reason}",
                path.display()
            ),
            Refusal::NoTarget => write!(
                formatter,
                "refused: this call named no target; name exactly one of `url` or `path`"
            ),
            Refusal::TwoTargets { url, path } => write!(
                formatter,
                "refused: this call named both the url `{url}` and the path `{path}`; name exactly \
                 one of them"
            ),
            Refusal::Scheme { url, scheme } => {
                let carried = match scheme.is_empty() {
                    true => String::from("no scheme"),
                    false => format!("the scheme `{scheme}`"),
                };
                write!(
                    formatter,
                    "refused: `{url}` carries {carried}; the allowed schemes are {}",
                    Refusal::allowed_schemes()
                )
            }
            Refusal::Authority { url, authority } => write!(
                formatter,
                "refused: `{url}` names the authority `{authority}`; the authority must be empty \
                 or `{AUTHORITY}`, because the browser is on this machine"
            ),
            Refusal::Extension { path, extension } => write!(
                formatter,
                "refused: `{path}` has {}; the allowed extensions are {}",
                spoken(extension),
                Refusal::allowed_extensions()
            ),
            Refusal::ResolvedExtension {
                written,
                path,
                extension,
            } => write!(
                formatter,
                "refused: `{written}` resolves to `{}`, which has {}; the allowed extensions are {}",
                path.display(),
                spoken(extension),
                Refusal::allowed_extensions()
            ),
            Refusal::Outside { path, places } => write!(
                formatter,
                "refused: `{path}` is outside the writable paths and the session temp, which are \
                 the only places a target may come from: {}",
                places.join(", ")
            ),
            Refusal::Escapes {
                written,
                path,
                places,
            } => write!(
                formatter,
                "refused: `{written}` resolves to `{}`, which is outside the writable paths and the \
                 session temp, which are the only places a target may come from: {}",
                path.display(),
                places.join(", ")
            ),
            Refusal::NotRegular { path } => write!(
                formatter,
                "refused: `{}` is not a regular file; a target is a file and not a directory, a \
                 device or a socket",
                path.display()
            ),
        }
    }
}

impl Error for Refusal {}

fn spoken(extension: &Option<String>) -> String {
    match extension {
        Some(extension) => format!("the extension `.{extension}`"),
        None => String::from("no extension"),
    }
}
