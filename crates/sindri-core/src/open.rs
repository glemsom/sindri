//! `open` hands one target to the host's browser, and every refusal lives in the
//! call: a scheme allowlist, no authority but an empty one or `localhost`, an
//! extension allowlist, containment inside the writable set plus the session
//! temp, and a regular file only.
//!
//! This module decides; it does not spawn. The sandbox has no display, so the
//! process that opens a browser is the host's and belongs to a front end, while
//! what may be opened is a decision the core makes and a front end cannot second
//! guess. [`Ground::judge`] is that decision as a value: one target in, or one
//! [`Refusal`] out that names the allowlist it broke, so the model can correct
//! itself in one turn instead of guessing what the boundary wanted.
//!
//! Containment and extension are checked **twice** — on the string the model
//! wrote, and on the path it resolves to — because those are two different
//! things. A string inside the writable set carrying an allowed extension is a
//! symlink away from anywhere on the machine: a link to `/` passes a check on the
//! string alone, which is why the resolved path is judged as well and why a
//! refusal names the path it resolved to rather than the one that was written.
//!
//! The allowlists are constants and the tool's description and the refusals are
//! both rendered from them, so the words the model is given and the words it is
//! refused with cannot drift apart.

use std::error::Error;
use std::ffi::OsString;
use std::fmt;
use std::path::{Component, Path, PathBuf};

use serde_json::Value;

use crate::policy::Boundary;

/// The schemes a `url` may carry. A web page and nothing else: `file:` would hand
/// the host's browser a local path no judgement ever looked at, which is the
/// whole boundary walked around in one word.
pub const SCHEMES: [&str; 2] = ["http", "https"];

/// The one authority a `url` may name: nothing but this machine.
pub const AUTHORITY: &str = "localhost";

/// The extensions a `path` may carry, on the string the model wrote and on the
/// path it resolves to.
pub const EXTENSIONS: [&str; 12] = [
    "html", "htm", "pdf", "png", "jpg", "jpeg", "gif", "webp", "svg", "txt", "md", "csv",
];

/// The one target a decision names, as the front end hands it to the opener.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Opening {
    /// A URL, already judged: scheme and authority are on the allowlist, and a
    /// URL has nowhere to resolve to.
    Url(String),
    /// A file, resolved: inside the writable set or the session temp, an allowed
    /// extension, and a regular file.
    File(PathBuf),
}

impl Opening {
    /// The target as the one argv word a host's opener is spawned with: never a
    /// shell string, never a word a shell could split.
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

/// Where a run stands when it asks to open something: the places a target may
/// come from — the writable set and the session temp — and the directory a
/// relative path is read from.
///
/// The places are held canonical and sorted, because a judgement compares the
/// filesystem's own spelling of a path against them; nothing here is a string
/// compare that a `..` or a symlink could walk around.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ground {
    /// The writable set plus the session temp, canonical, in the one order the
    /// refusals print them in.
    places: Vec<PathBuf>,
    /// The directory a relative target is read from.
    cwd: PathBuf,
}

impl Ground {
    /// The ground of a run: the boundary in force, the session temp beside it,
    /// and the directory the run stands in.
    ///
    /// Every path is resolved through the filesystem here rather than on each
    /// judgement, so a judgement is a decision about a target and not about
    /// where the run happens to be.
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

    /// The places a target may come from, canonical and sorted.
    pub fn places(&self) -> &[PathBuf] {
        &self.places
    }

    /// Judge one call's arguments: the one target it names, or the refusal that
    /// says which allowlist it broke and every value on it.
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

    /// A URL: the scheme on the allowlist, and no authority but an empty one or
    /// this machine.
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

    /// A file, judged twice: the string the model wrote, and the path it
    /// resolves to.
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

    /// Whether `path` is inside one of the places a target may come from.
    fn contains(&self, path: &Path) -> bool {
        self.places.iter().any(|place| path.starts_with(place))
    }

    /// The places as a refusal spells them.
    fn rendered(&self) -> Vec<String> {
        self.places
            .iter()
            .map(|place| place.display().to_string())
            .collect()
    }
}

/// The tool's words, rendered from the allowlists the refusals are rendered from
/// and from the boundary in force, so the model reads the rule it will be held to.
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

/// The value of `key`, when the call named one: an empty string, or no key at
/// all, is a target that was not named.
fn written<'a>(arguments: &'a Value, key: &str) -> Option<&'a str> {
    arguments
        .get(key)?
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// A path's extension, lowercased, so `.HTML` is `html`.
fn extension(path: &Path) -> Option<String> {
    path.extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
}

/// Whether an extension is on the allowlist.
fn allowed(extension: &Option<String>) -> bool {
    extension
        .as_ref()
        .is_some_and(|extension| EXTENSIONS.contains(&extension.as_str()))
}

/// `path` with its `.` and `..` folded away and no filesystem read, which is what
/// lets the string be judged before anything is followed.
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

/// `path` as the filesystem spells it, refused with the path it was written as
/// when it cannot be resolved.
fn resolve(path: &Path, written: &Path) -> Result<PathBuf, Refusal> {
    std::fs::canonicalize(path).map_err(|error| Refusal::Unresolvable {
        path: written.to_path_buf(),
        reason: error.to_string(),
    })
}

/// Why one target was not handed to the browser. Each one names the value that
/// was refused and the values that would have been accepted, because a model
/// that cannot read the allowlist spends a turn guessing at it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The call's arguments are not the JSON the schema promised.
    Unreadable {
        /// The arguments as they arrived.
        arguments: String,
        /// Why they could not be read.
        reason: String,
    },
    /// The path does not exist, or one of its directories cannot be walked.
    Unresolvable {
        /// The path as it was written, or as the ground names it.
        path: PathBuf,
        /// Why the filesystem would not resolve it.
        reason: String,
    },
    /// The call named neither a `url` nor a `path`.
    NoTarget,
    /// The call named both, and one target is the product.
    TwoTargets {
        /// The URL it named.
        url: String,
        /// The path it named.
        path: String,
    },
    /// The URL's scheme is not on the allowlist.
    Scheme {
        /// The URL as it was written.
        url: String,
        /// The scheme it carried, or nothing when it carried no scheme at all.
        scheme: String,
    },
    /// The URL names an authority that is neither empty nor this machine.
    Authority {
        /// The URL as it was written.
        url: String,
        /// The authority it named.
        authority: String,
    },
    /// The string the model wrote does not carry an allowed extension.
    Extension {
        /// The path as it was written.
        path: String,
        /// Its extension, or nothing when it had none.
        extension: Option<String>,
    },
    /// The path the string resolved to does not carry an allowed extension.
    ResolvedExtension {
        /// The path as it was written.
        written: String,
        /// The path it resolved to.
        path: PathBuf,
        /// The resolved path's extension, or nothing when it had none.
        extension: Option<String>,
    },
    /// The string the model wrote is outside the writable set and the session
    /// temp before anything is even followed.
    Outside {
        /// The path as it was written.
        path: String,
        /// The places a target may come from.
        places: Vec<String>,
    },
    /// The path resolved to somewhere outside the writable set and the session
    /// temp: the symlink that is the reason the check runs twice.
    Escapes {
        /// The path as it was written.
        written: String,
        /// The path it resolved to.
        path: PathBuf,
        /// The places a target may come from.
        places: Vec<String>,
    },
    /// The path resolved to something that is not a regular file.
    NotRegular {
        /// The path it resolved to.
        path: PathBuf,
    },
}

impl Refusal {
    /// The extensions a refusal names, as one phrase.
    fn allowed_extensions() -> String {
        EXTENSIONS.join(", ")
    }

    /// The schemes a refusal names, as one phrase.
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

/// An extension as the two refusals speak of it.
fn spoken(extension: &Option<String>) -> String {
    match extension {
        Some(extension) => format!("the extension `.{extension}`"),
        None => String::from("no extension"),
    }
}
