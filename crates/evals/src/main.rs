//! `sindri-evals`: the two promises a user cannot verify by reading output,
//! made verifiable — the sandbox blocks what it claims, and the turn cap stops a
//! loopy run. Eight scripted batch runs against a fixture repository, asserted on
//! exit codes, JSON fields and the files on disk, never on model prose.

#![allow(
    clippy::expect_used,
    reason = "a harness that cannot set up its own run has nothing to report but a panic"
)]

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Parser;
use sindri_evals::case::{Case, all};
use sindri_evals::report::{Verdict, exit_code, line};
use sindri_evals::workspace::Workspace;
use sindri_evals::{guard, run};

#[derive(Parser)]
#[command(
    name = "sindri-evals",
    about = "The eval suite: eight scripted runs that catch silent degradation"
)]
struct Args {
    /// Ask a real provider instead of replaying recorded fixtures. Uses your
    /// own credential and costs money; see the README.
    #[arg(long)]
    live: bool,
    #[arg(long, value_name = "TEXT")]
    only: Option<String>,
}

fn main() -> ExitCode {
    let args = Args::parse();
    let selected = select(&args);
    if selected.is_empty() {
        eprintln!(
            "sindri-evals: no eval matches `{}`",
            args.only.as_deref().unwrap_or_default()
        );
        return ExitCode::from(2);
    }
    match suite(&args, &selected) {
        Ok(failed) => ExitCode::from(failed),
        // 3, not 1: the suite could not run, or the sandbox is not enforcing —
        // never a verdict about the product.
        Err(problem) => {
            eprintln!("eval: {problem}");
            ExitCode::from(3)
        }
    }
}

fn select(args: &Args) -> Vec<Case> {
    all()
        .into_iter()
        .filter(|case| match &args.only {
            Some(text) => case.name.contains(text.as_str()),
            None => true,
        })
        .collect()
}

fn suite(args: &Args, selected: &[Case]) -> Result<u8, String> {
    let bin = binary()?;
    guard::sandbox_is_safe(&bin, &Workspace::new()?)?;

    let mut verdicts = Vec::new();
    for case in selected {
        let workspace = Workspace::new()?;
        let outcome = run::execute(&bin, &workspace, case, args.live)?;
        let verdict = Verdict {
            name: case.name,
            problems: (case.check)(&outcome),
        };
        println!("{}", line(&verdict));
        for problem in verdict.problems.iter().skip(1) {
            eprintln!("      also: {problem}");
        }
        verdicts.push(verdict);
    }

    let failed = verdicts.iter().filter(|verdict| !verdict.passed()).count();
    let mode = if args.live { "live" } else { "recorded" };
    println!(
        "eval: {} {} of {} passed ({mode} fixtures)",
        verdicts.len(),
        verdicts.len() - failed,
        verdicts.len()
    );
    Ok(u8::try_from(exit_code(&verdicts)).unwrap_or(1))
}

fn binary() -> Result<PathBuf, String> {
    if let Ok(path) = std::env::var("SINDRI_BIN") {
        return Ok(PathBuf::from(path));
    }
    let exe = std::env::current_exe().map_err(|error| format!("current exe: {error}"))?;
    let beside = exe
        .parent()
        .map(Path::new)
        .ok_or("the suite has no directory to look beside")?
        .join("sindri");
    if beside.is_file() {
        return Ok(beside);
    }
    Err(format!(
        "{} is not built: run `make eval` or set SINDRI_BIN",
        beside.display()
    ))
}
