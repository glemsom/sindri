//! `sindri doctor` is the honest window: it proves the sandbox, prints the
//! description the model is given, and lists every skill found with its scope.
//!
//! The proof is the one boot runs, not a `--version` probe (§6): a real
//! bubblewrap invocation over the boundary in force, borrowing a session's temp
//! for the length of the check because that is the directory a run's commands are
//! handed as `TMPDIR`. A check that fails prints the card naming it and exits 3,
//! which is §3's exit 3.
//!
//! The description is read off the tool set a run registers rather than rendered
//! here, so §5's sentence — the boundary the model is told about is the boundary
//! enforced — is held by construction: doctor has no second opinion to offer, and
//! a test compares these bytes against the `bash` tool's own.
//!
//! Everything it prints goes to stdout, because the report *is* the answer to the
//! question `doctor` was asked; §15's stderr rule is batch mode's, where stdout is
//! the answer and one newline and nothing else.

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use sindri_core::config::{Config, Process};
use sindri_core::policy::Boundary;
use sindri_core::sandbox::{self, Card, Proof};
use sindri_core::session::{self, Session};
use sindri_core::skills::{Catalog, Diagnostic, Listing};
use sindri_core::tools::{BASH, FrontEnd, ToolSet};

use crate::cli::Cli;
use crate::refuse;

/// §15's sandbox/setup failure, which is the code a failed check is reported by.
const SETUP: u8 = 3;

/// The column every fact on the report's head is named in.
const LABEL: usize = 14;

/// The column a skill's scope word is written in: `builtin` is the longest of the
/// three scopes, and the shadowing line's verb is the same width.
const SCOPE_COLUMN: usize = 9;

/// Runs `sindri doctor`: it answers where the run would stand, and it runs
/// nothing.
pub fn run(cli: &Cli) -> ExitCode {
    let config = match Config::resolve(&Process, cli.flags()) {
        Ok(config) => config,
        Err(error) => return refuse(&error.to_string(), error.exit_code()),
    };
    let workspace = match std::env::current_dir() {
        Ok(workspace) => workspace,
        Err(error) => {
            return refuse(
                &format!("cannot read the working directory: {error}"),
                SETUP,
            );
        }
    };
    let boundary = match Boundary::from_config(&config, &workspace, cli.enforcement()) {
        Ok(boundary) => boundary,
        Err(error) => return refuse(&error.to_string(), SETUP),
    };
    let catalog = Catalog::discover(&workspace, home().as_deref());
    let tools = ToolSet::new(&boundary, &catalog, FrontEnd::Batch);
    let described = tools
        .get(BASH)
        .expect("`bash` is the one tool every run registers")
        .description();

    let state_home = match session::state_home(&Process) {
        Ok(state_home) => state_home,
        Err(error) => return refuse(&error.to_string(), error.exit_code()),
    };
    let scratch = match Scratch::new(&state_home) {
        Ok(scratch) => scratch,
        Err(error) => return refuse(&error.to_string(), SETUP),
    };
    let proved = sandbox::prove(&boundary, scratch.temp());

    let report = Report {
        boundary: &boundary,
        workspace: &workspace,
        described,
        catalog: &catalog,
        proved,
    };
    if let Err(error) = write(&report.render()) {
        return refuse(&format!("cannot write the report: {error}"), SETUP);
    }
    match &report.proved {
        Ok(_) => ExitCode::SUCCESS,
        Err(card) => ExitCode::from(card.exit_code()),
    }
}

/// The home the user-scope skills come from, when the environment names one.
fn home() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(PathBuf::from)
}

/// Writes the report to stdout, and nothing else: it is what was asked for.
fn write(report: &str) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    stdout.write_all(report.as_bytes())?;
    stdout.flush()
}

/// Everything the report says, held together so the one write is of one value.
struct Report<'a> {
    /// The boundary in force, which the facts are read off.
    boundary: &'a Boundary,
    /// The one directory the run would stand in.
    workspace: &'a Path,
    /// The `bash` tool's description, verbatim.
    described: &'a str,
    /// Every skill the three scopes offer.
    catalog: &'a Catalog,
    /// What the proof made of the sandbox.
    proved: Result<Proof, Card>,
}

impl Report<'_> {
    /// The report as the bytes stdout carries: the facts about the run, the
    /// description the model is given, then the catalog.
    fn render(&self) -> String {
        let mut lines: Vec<String> = vec![String::from("sindri doctor")];
        lines.push(fact("enforcement", self.boundary.enforcement().word()));
        lines.push(fact("workspace", &self.workspace.display().to_string()));
        match &self.proved {
            Ok(proof) => lines.push(fact("sandbox", &proof.to_string())),
            Err(card) => {
                lines.push(fact("sandbox", "failed"));
                lines.push(String::new());
                lines.push(card.to_string());
            }
        }

        lines.push(String::new());
        lines.push(format!("the description the model is given for `{BASH}`"));
        lines.push(String::new());
        lines.push(self.described.to_owned());

        lines.push(String::new());
        lines.extend(skills(&self.catalog.listing()));
        if !self.catalog.diagnostics().is_empty() {
            lines.push(String::new());
            lines.extend(diagnostics(self.catalog.diagnostics()));
        }

        lines.join("\n") + "\n"
    }
}

/// The catalog as the report lists it: every skill found with the scope it came
/// from, in name order, or one line saying there are none rather than a silence a
/// reader would have to interpret.
fn skills(listing: &[Listing]) -> Vec<String> {
    let mut lines = vec![match listing.len() {
        0 => String::from("skills (none found)"),
        found => format!("skills ({found} found)"),
    }];
    let widest = listing
        .iter()
        .map(|entry| entry.name.chars().count())
        .max()
        .unwrap_or(0);
    for entry in listing {
        lines.push(
            format!(
                "  {:<SCOPE$}{:<widest$}  {}",
                entry.scope.word(),
                entry.name,
                entry.description,
                SCOPE = SCOPE_COLUMN
            )
            .trim_end()
            .to_owned(),
        );
        // A shadowed name reports the loser too, by the path that never loads, so a
        // person can open the file discovery is ignoring.
        for lost in &entry.shadows {
            let worse_off = match &lost.loser_path {
                Some(path) => format!("{} {}", lost.loser.word(), path.display()),
                None => lost.loser.word().to_owned(),
            };
            lines.push(
                format!(
                    "  {:<SCOPE$}{:<widest$}  {} wins over {worse_off}",
                    "shadows",
                    lost.name,
                    lost.winner.word(),
                    SCOPE = SCOPE_COLUMN
                )
                .trim_end()
                .to_owned(),
            );
        }
    }
    lines
}

/// What discovery refused, each by the file it is in and the line to open it at,
/// so a skill that is missing from the catalog above is explained rather than
/// missed.
fn diagnostics(diagnostics: &[Diagnostic]) -> Vec<String> {
    let mut lines = vec![format!("diagnostics ({})", diagnostics.len())];
    let widest = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.path.display().to_string().chars().count() + 2)
        .max()
        .unwrap_or(0);
    for diagnostic in diagnostics {
        let where_it_lies = format!("{}:{}", diagnostic.path.display(), diagnostic.line);
        lines.push(
            format!("  {where_it_lies:<widest$}  {}", diagnostic.message)
                .trim_end()
                .to_owned(),
        );
    }
    lines
}

/// One fact on the report's head: its label in the one column every label sits
/// in, and its value after it.
fn fact(label: &str, value: &str) -> String {
    format!("{label:<LABEL$}{value}")
}

/// The session temp the proof binds, and the session directory that goes away
/// with the report that spent it.
///
/// A run's scratch is a session's temp, so doctor borrows exactly that — the same
/// directory, spelled by the one function that spells it — and keeps nothing: a
/// diagnostic records no run, and a session nobody wrote is one no reader should
/// find.
struct Scratch {
    /// The session whose temp the proof binds.
    session: Session,
}

impl Scratch {
    /// A session under `state_home`, made only for its temp.
    fn new(state_home: &Path) -> Result<Scratch, String> {
        Session::create(state_home)
            .map(|session| Scratch { session })
            .map_err(|error| error.to_string())
    }

    /// The directory bubblewrap binds as the sandbox's `TMPDIR`.
    fn temp(&self) -> &Path {
        self.session.temp()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(self.session.directory());
    }
}
