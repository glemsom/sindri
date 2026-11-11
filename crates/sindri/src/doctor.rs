use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use sindri_core::config::{Config, Process};
use sindri_core::policy::Boundary;
use sindri_core::sandbox::{self, Card, Proof};
use sindri_core::session::{self, Session};
use sindri_core::skills::{Catalog, Diagnostic, Listing};
use sindri_core::tools::{BASH, FrontEnd, ToolSet};

use sindri::pen;

use crate::cli::Cli;
use crate::refuse;

/// The exit code a failed check is reported by.
const SETUP: u8 = 3;

const LABEL: u16 = 14;

/// `builtin` is the longest of the three scopes, and the shadowing line's verb is as wide.
const SCOPE_COLUMN: u16 = 9;

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

fn home() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(PathBuf::from)
}

fn write(report: &str) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    stdout.write_all(report.as_bytes())?;
    stdout.flush()
}

struct Report<'a> {
    boundary: &'a Boundary,
    workspace: &'a Path,
    described: &'a str,
    catalog: &'a Catalog,
    proved: Result<Proof, Card>,
}

impl Report<'_> {
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

fn skills(listing: &[Listing]) -> Vec<String> {
    let mut lines = vec![match listing.len() {
        0 => String::from("skills (none found)"),
        found => format!("skills ({found} found)"),
    }];
    let widest = listing
        .iter()
        .map(|entry| pen::cells(&entry.name))
        .max()
        .unwrap_or(0);
    for entry in listing {
        lines.push(
            format!(
                "  {}{}  {}",
                pen::pad(entry.scope.word(), SCOPE_COLUMN),
                pen::pad(&entry.name, widest),
                entry.description,
            )
            .trim_end()
            .to_owned(),
        );
        // A shadowed name reports the loser by the path that never loads, so a person can
        // open the file discovery is ignoring.
        for lost in &entry.shadows {
            let worse_off = match &lost.loser_path {
                Some(path) => format!("{} {}", lost.loser.word(), path.display()),
                None => lost.loser.word().to_owned(),
            };
            lines.push(
                format!(
                    "  {}{}  {} wins over {worse_off}",
                    pen::pad("shadows", SCOPE_COLUMN),
                    pen::pad(&lost.name, widest),
                    lost.winner.word(),
                )
                .trim_end()
                .to_owned(),
            );
        }
    }
    lines
}

fn diagnostics(diagnostics: &[Diagnostic]) -> Vec<String> {
    let mut lines = vec![format!("diagnostics ({})", diagnostics.len())];
    let widest = diagnostics
        .iter()
        .map(|diagnostic| pen::cells(&diagnostic.path.display().to_string()) + 2)
        .max()
        .unwrap_or(0);
    for diagnostic in diagnostics {
        let where_it_lies = format!("{}:{}", diagnostic.path.display(), diagnostic.line);
        lines.push(
            format!(
                "  {}  {}",
                pen::pad(&where_it_lies, widest),
                diagnostic.message
            )
            .trim_end()
            .to_owned(),
        );
    }
    lines
}

fn fact(label: &str, value: &str) -> String {
    format!("{}{value}", pen::pad(label, LABEL))
}

/// A run's scratch is a session's temp, so doctor borrows exactly that and keeps nothing: a
/// diagnostic records no run, and a session nobody wrote is one no reader should find.
struct Scratch {
    session: Session,
}

impl Scratch {
    fn new(state_home: &Path) -> Result<Scratch, String> {
        Session::create(state_home)
            .map(|session| Scratch { session })
            .map_err(|error| error.to_string())
    }

    fn temp(&self) -> &Path {
        self.session.temp()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(self.session.directory());
    }
}
