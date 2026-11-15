use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use sindri_core::config::{Config, Process};
use sindri_core::launch::{Launch, Machine};
use sindri_core::policy::Enforcement;
use sindri_core::skills::{Diagnostic, Listing};
use sindri_core::tools::{BASH, FrontEnd};

use sindri::pen;

use crate::cli::Cli;
use crate::refuse;

/// The exit code a report that could not be written is reported by.
const SETUP: u8 = 3;

const LABEL: u16 = 14;

/// `builtin` is the longest of the three scopes, and the shadowing line's verb is as wide.
const SCOPE_COLUMN: u16 = 9;

pub fn run(cli: &Cli) -> ExitCode {
    let config = match Config::resolve(&Process, cli.flags()) {
        Ok(config) => config,
        Err(error) => return refuse(&error.to_string(), error.exit_code()),
    };
    let launch = match Launch::open(
        &Machine::this(),
        &config,
        cli.enforcement(),
        FrontEnd::Batch,
    ) {
        Ok(launch) => launch,
        Err(refusal) => return refuse(refusal.message(), refusal.code()),
    };

    let report = Report::of(&launch);
    launch.discard();
    if let Err(error) = write(&report.render()) {
        return refuse(&format!("cannot write the report: {error}"), SETUP);
    }
    ExitCode::SUCCESS
}

fn write(report: &str) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    stdout.write_all(report.as_bytes())?;
    stdout.flush()
}

/// Every fact is read off the launch the run would be made from.
struct Report {
    enforcement: Enforcement,
    workspace: PathBuf,
    proof: String,
    described: String,
    listing: Vec<Listing>,
    diagnostics: Vec<Diagnostic>,
}

impl Report {
    fn of(launch: &Launch) -> Report {
        let figure = launch.figure();
        Report {
            enforcement: figure.enforcement,
            workspace: figure.workspace,
            proof: figure.proof,
            described: launch.bash_description().to_owned(),
            listing: launch.listing(),
            diagnostics: launch.diagnostics().to_vec(),
        }
    }

    fn render(self) -> String {
        let mut lines: Vec<String> = vec![String::from("sindri doctor")];
        lines.push(fact("enforcement", self.enforcement.word()));
        lines.push(fact("workspace", &self.workspace.display().to_string()));
        lines.push(fact("sandbox", &self.proof));

        lines.push(String::new());
        lines.push(format!("the description the model is given for `{BASH}`"));
        lines.push(String::new());
        lines.push(self.described);

        lines.push(String::new());
        lines.extend(skills(&self.listing));
        if !self.diagnostics.is_empty() {
            lines.push(String::new());
            lines.extend(diagnostics(&self.diagnostics));
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
    let where_it_lies: Vec<String> = diagnostics.iter().map(where_it_lies).collect();
    let widest = where_it_lies
        .iter()
        .map(|at| pen::cells(at))
        .max()
        .unwrap_or(0);
    for (at, diagnostic) in where_it_lies.iter().zip(diagnostics) {
        lines.push(
            format!("  {}  {}", pen::pad(at, widest), diagnostic.message)
                .trim_end()
                .to_owned(),
        );
    }
    lines
}

fn where_it_lies(diagnostic: &Diagnostic) -> String {
    format!("{}:{}", diagnostic.path.display(), diagnostic.line)
}

fn fact(label: &str, value: &str) -> String {
    format!("{}{value}", pen::pad(label, LABEL))
}
