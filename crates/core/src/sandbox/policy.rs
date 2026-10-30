//! The sandbox policy value: private fields, a validating constructor, and the
//! accessors the two derivations read (ADR-0002).

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkPosture {
    Shared,
}

impl NetworkPosture {
    pub fn describe(self) -> &'static str {
        match self {
            NetworkPosture::Shared => {
                "shared — outbound network works; the sandbox bounds the filesystem, not the network"
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Enforcement {
    Bubblewrap,
    YoloUnsafe,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WritableSet(Vec<PathBuf>);

impl WritableSet {
    /// Canonical, distinct, non-nested, never `/`. Order is mount order.
    pub fn new(paths: impl IntoIterator<Item = PathBuf>) -> Result<Self, PolicyError> {
        let mut accepted: Vec<PathBuf> = Vec::new();
        for given in paths {
            let path = canonical_writable(&given)?;
            for seen in &accepted {
                if *seen == path {
                    return Err(PolicyError::Duplicate { path });
                }
                if path.starts_with(seen) {
                    return Err(PolicyError::Nested {
                        parent: seen.clone(),
                        child: path,
                    });
                }
                if seen.starts_with(&path) {
                    return Err(PolicyError::Nested {
                        parent: path,
                        child: seen.clone(),
                    });
                }
            }
            accepted.push(path);
        }
        Ok(WritableSet(accepted))
    }

    pub fn paths(&self) -> &[PathBuf] {
        &self.0
    }
}

fn canonical_writable(given: &Path) -> Result<PathBuf, PolicyError> {
    if !given.is_absolute() {
        return Err(PolicyError::NotAbsolute {
            path: given.to_path_buf(),
        });
    }
    let canonical = given
        .canonicalize()
        .map_err(|source| PolicyError::Unreadable {
            path: given.to_path_buf(),
            source,
        })?;
    if canonical.parent().is_none() {
        return Err(PolicyError::Root);
    }
    // Component equality would accept `/a/.` and `/a/`; the raw form is the
    // thing that must already be canonical, because it is what gets mounted.
    if canonical.as_os_str() != given.as_os_str() {
        return Err(PolicyError::NotCanonical {
            given: given.to_path_buf(),
            canonical,
        });
    }
    Ok(canonical)
}

#[derive(Debug)]
pub enum PolicyError {
    Root,
    NotAbsolute { path: PathBuf },
    NotCanonical { given: PathBuf, canonical: PathBuf },
    Unreadable { path: PathBuf, source: io::Error },
    Duplicate { path: PathBuf },
    Nested { parent: PathBuf, child: PathBuf },
    InvalidSessionTemp { path: PathBuf },
}

impl fmt::Display for PolicyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PolicyError::Root => write!(f, "the whole filesystem cannot be writable: refusing `/`"),
            PolicyError::NotAbsolute { path } => {
                write!(f, "writable path {} is not absolute", path.display())
            }
            PolicyError::NotCanonical { given, canonical } => write!(
                f,
                "writable path {} is not canonical; use {}",
                given.display(),
                canonical.display()
            ),
            PolicyError::Unreadable { path, source } => {
                write!(
                    f,
                    "writable path {} cannot be read: {source}",
                    path.display()
                )
            }
            PolicyError::Duplicate { path } => {
                write!(f, "writable path {} is granted twice", path.display())
            }
            PolicyError::Nested { parent, child } => write!(
                f,
                "writable path {} is inside {}; grant one or the other",
                child.display(),
                parent.display()
            ),
            PolicyError::InvalidSessionTemp { path } => write!(
                f,
                "session temp {} must be an absolute path that is not `/`",
                path.display()
            ),
        }
    }
}

impl std::error::Error for PolicyError {}

/// The single source of both enforcement and description.
#[derive(Debug, Clone)]
pub struct SandboxPolicy {
    writable: WritableSet,
    session_temp: PathBuf,
    search_path: String,
    network: NetworkPosture,
    enforcement: Enforcement,
}

impl SandboxPolicy {
    pub fn new(
        writable: WritableSet,
        session_temp: PathBuf,
        search_path: String,
        network: NetworkPosture,
        enforcement: Enforcement,
    ) -> Result<Self, PolicyError> {
        if !session_temp.is_absolute() || session_temp.parent().is_none() {
            return Err(PolicyError::InvalidSessionTemp { path: session_temp });
        }
        Ok(SandboxPolicy {
            writable,
            session_temp,
            search_path,
            network,
            enforcement,
        })
    }

    pub fn writable(&self) -> &WritableSet {
        &self.writable
    }

    pub fn session_temp(&self) -> &Path {
        &self.session_temp
    }

    pub fn search_path(&self) -> &str {
        &self.search_path
    }

    pub fn network(&self) -> NetworkPosture {
        self.network
    }

    pub fn enforcement(&self) -> Enforcement {
        self.enforcement
    }

    pub fn is_unsafe(&self) -> bool {
        self.enforcement == Enforcement::YoloUnsafe
    }
}

pub const FALLBACK_PATH: &str = "/usr/local/bin:/usr/bin:/bin";
