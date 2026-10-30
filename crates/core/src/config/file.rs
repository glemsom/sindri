//! The config file's own shape, its bytes, and where it lands: the TOML the
//! schema names, read and written in one place.

use std::collections::BTreeMap;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::ConfigError;

/// Every key optional, every key a string or a count. A key the file does not
/// name is left unset; a key the schema does not name is an error, because a
/// typo'd `max_turn` would otherwise run on defaults the user never chose.
#[derive(Debug, Default, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FileConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) max_turns: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) timeout: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) thinking: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) extra_writable_paths: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) extra_headers: Option<BTreeMap<String, String>>,
}

pub(super) fn read() -> Result<FileConfig, ConfigError> {
    read_at(Path::new(&super::file_path()))
}

/// One config file, as it is on disk. An absent file is an empty one; an
/// unreadable or unparsable file is a usage error, never silently ignored.
pub(super) fn read_at(path: &Path) -> Result<FileConfig, ConfigError> {
    match std::fs::read_to_string(path) {
        Ok(text) => toml::from_str(&text)
            .map_err(|error| ConfigError::Usage(parse_error(path, &text, &error))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(FileConfig::default()),
        Err(error) => Err(ConfigError::Usage(format!("{}: {error}", path.display()))),
    }
}

/// The same document, written back. The bytes land in a temporary file beside
/// the real one and are renamed over it, so a reader never sees half a file;
/// the temporary is created readable only by its owner, because this file may
/// carry a credential.
pub(super) fn write(path: &Path, values: &FileConfig) -> Result<(), ConfigError> {
    let text = toml::to_string(values).map_err(|error| {
        ConfigError::Usage(format!("{}: cannot be written: {error}", path.display()))
    })?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|error| ConfigError::Usage(format!("{}: {error}", dir.display())))?;
    }
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("config.toml");
    let temp = path.with_file_name(format!(".{name}.tmp"));
    let written = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&temp)
        .and_then(|mut file| {
            file.write_all(text.as_bytes())?;
            file.sync_all()
        });
    if let Err(error) = written {
        let _ = std::fs::remove_file(&temp);
        return Err(ConfigError::Usage(format!("{}: {error}", temp.display())));
    }
    std::fs::rename(&temp, path)
        .map_err(|error| ConfigError::Usage(format!("{}: {error}", path.display())))
}

/// A parse error, without the source snippet TOML would print: that snippet is
/// the file's own text, and the credential may well be on that line. The
/// message and the position are kept, because those are what a user acts on.
fn parse_error(path: &Path, text: &str, error: &toml::de::Error) -> String {
    let shown = path.display();
    let Some(span) = error.span() else {
        return format!("{shown}: {}", error.message());
    };
    let before = &text[..span.start.min(text.len())];
    let line = before.matches('\n').count() + 1;
    let column = before.rsplit('\n').next().map_or(0, str::len) + 1;
    format!(
        "{shown}: {} at line {line}, column {column}",
        error.message()
    )
}
