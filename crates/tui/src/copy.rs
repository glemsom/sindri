//! Copying text out of the frame: OSC 52 to the terminal, a shell tool when
//! the terminal cannot carry it, and a refusal said as a refusal
//! (ADR-0011). Nothing here assumes a local clipboard: the frame runs over
//! SSH and inside `tmux`.

use std::io::{IsTerminal, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Mutex;

pub const TOOLS: &[(&str, &[&str])] = &[("wl-copy", &[]), ("xclip", &["-selection", "clipboard"])];

/// The most a terminal is asked to carry through one OSC 52 sequence. Past
/// this, a copy goes through a shell tool instead: an escape of a megabyte is
/// a hang, not a clipboard.
pub const OSC52_LIMIT: usize = 100_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Copied {
    /// Written to the terminal as OSC 52. A terminal does not answer, so
    /// this says the text was *sent*; saying "copied" would claim what
    /// nobody can check.
    Sent,
    Tool(&'static str),
    Refused(String),
}

pub trait Clipboard: Send {
    fn copy(&self, text: &str) -> Copied;
}

pub struct System {
    terminal: bool,
    path: String,
    out: Mutex<Box<dyn Write + Send>>,
}

impl System {
    pub fn detect() -> System {
        System {
            terminal: std::io::stdout().is_terminal(),
            path: std::env::var("PATH").unwrap_or_default(),
            out: Mutex::new(Box::new(std::io::stdout())),
        }
    }

    pub fn new(terminal: bool, path: impl Into<String>, out: Box<dyn Write + Send>) -> System {
        System {
            terminal,
            path: path.into(),
            out: Mutex::new(out),
        }
    }

    /// OSC 52: `ESC ] 52 ; c ; base64 BEL`, the selection being the
    /// clipboard.
    fn oscill52(&self, text: &str) -> std::io::Result<()> {
        let Ok(mut out) = self.out.lock() else {
            return Err(std::io::Error::other("the clipboard is busy"));
        };
        out.write_all(b"\x1b]52;c;")?;
        out.write_all(base64(text.as_bytes()).as_bytes())?;
        out.write_all(b"\x07")?;
        out.flush()
    }

    fn through_tools(&self, text: &str) -> Copied {
        let mut tried: Vec<String> = Vec::new();
        for (name, args) in TOOLS {
            let Some(program) = self.locate(name) else {
                continue;
            };
            let child = Command::new(&program)
                .args(*args)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn();
            let mut child = match child {
                Ok(child) => child,
                Err(error) => {
                    tried.push(format!("{name}: {error}"));
                    continue;
                }
            };
            let wrote = child
                .stdin
                .as_mut()
                .is_some_and(|stdin| stdin.write_all(text.as_bytes()).is_ok());
            let status = child.wait();
            if wrote && status.is_ok_and(|status| status.success()) {
                return Copied::Tool(name);
            }
            tried.push(format!("{name} did not take the text"));
        }
        if tried.is_empty() {
            Copied::Refused(format!(
                "nothing can carry a copy here: the terminal does not answer for one and no tool on PATH took it ({})",
                TOOLS
                    .iter()
                    .map(|(name, _)| *name)
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        } else {
            Copied::Refused(tried.join("; "))
        }
    }

    fn locate(&self, name: &str) -> Option<PathBuf> {
        self.path
            .split(':')
            .filter(|dir| !dir.is_empty())
            .map(|dir| PathBuf::from(dir).join(name))
            .find(|candidate| candidate.is_file())
    }
}

impl Clipboard for System {
    fn copy(&self, text: &str) -> Copied {
        if text.is_empty() {
            return Copied::Refused("there was nothing selected to copy".to_string());
        }
        if self.terminal && text.len() <= OSC52_LIMIT {
            match self.oscill52(text) {
                Ok(()) => return Copied::Sent,
                // The terminal refused the bytes: the tools are the fallback.
                Err(error) => {
                    let tool = self.through_tools(text);
                    return match tool {
                        Copied::Refused(why) => Copied::Refused(format!(
                            "the terminal took no OSC 52 copy ({error}); {why}"
                        )),
                        other => other,
                    };
                }
            }
        }
        self.through_tools(text)
    }
}

/// Base64, because an OSC 52 payload is base64 and one alphabet is not
/// worth a dependency.
pub fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let triple = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for index in 0..4 {
            if index <= chunk.len() {
                let at = (triple >> (18 - 6 * index)) & 0x3f;
                out.push(ALPHABET[at as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{Clipboard, Copied, System, base64};
    use std::io::Write;
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct Seen(Arc<Mutex<Vec<u8>>>);

    impl Write for Seen {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if let Ok(mut seen) = self.0.lock() {
                seen.extend_from_slice(bytes);
            }
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn base64_matches_the_alphabet_every_terminal_expects() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
        assert_eq!(base64(b"hello"), "aGVsbG8=");
    }

    #[test]
    fn a_copy_with_a_terminal_is_an_osc_52_escape_and_nothing_else() {
        let seen = Seen::default();
        let system = System::new(true, String::new(), Box::new(seen.clone()));
        assert_eq!(system.copy("hello"), Copied::Sent);
        let bytes = seen.0.lock().map(|seen| seen.clone()).unwrap_or_default();
        assert_eq!(
            bytes,
            b"\x1b]52;c;aGVsbG8=\x07".to_vec(),
            "the escape carries the text base64'd, on the clipboard selection"
        );
    }

    #[test]
    fn without_a_terminal_a_tool_takes_the_text_and_is_named() {
        let dir = tempfile::tempdir().expect("a temp directory");
        let out = dir.path().join("clipboard");
        let tool = dir.path().join("wl-copy");
        std::fs::write(&tool, format!("#!/bin/sh\ncat > {}\n", out.display()))
            .expect("write the fake tool");
        let mut permissions = std::fs::metadata(&tool).expect("metadata").permissions();
        std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o755);
        std::fs::set_permissions(&tool, permissions).expect("make it executable");

        let system = System::new(
            false,
            dir.path().display().to_string(),
            Box::new(Seen::default()),
        );
        assert_eq!(system.copy("selected rows"), Copied::Tool("wl-copy"));
        assert_eq!(
            std::fs::read_to_string(&out).unwrap_or_default(),
            "selected rows",
            "the tool saw exactly the selection"
        );
    }

    #[test]
    fn a_copy_that_nothing_takes_is_refused_as_a_refusal() {
        let system = System::new(false, String::new(), Box::new(Seen::default()));
        let refusal = system.copy("some text");
        match refusal {
            Copied::Refused(why) => assert!(
                why.contains("wl-copy") && why.contains("xclip"),
                "the refusal names what was missing: {why}"
            ),
            other => panic!("a copy nobody took is a refusal, not {other:?}"),
        }
    }
}
