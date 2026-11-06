//! The command line as clap sees it: only the flags this front end honours today,
//! with `--format` and `-v` accepted globally so a batch run and a TUI launch
//! share one parser rather than growing two.
//!
//! `-b` is what makes a launch batch mode (§3), and its value is optional because
//! a pipe can be the prompt instead. `--format` is one of two words and nothing
//! else: a third spelling would be a format no run can honour, so the parser is
//! where it is refused, at exit 2. Everything else the run will take as a flag
//! belongs to the tickets that own the rest of the front end.

use clap::{Parser, ValueEnum};
use sindri_core::config::Flags;

/// What `--version` reports: the `git describe` of the commit this binary was
/// built at and that commit's own date, both fixed at compile time by `build.rs`
/// and never read from a clock. A build standing on no commit — a source
/// tarball — reports the package version and says it knows no date.
pub const VERSION: &str = concat!(
    env!("SINDRI_DESCRIBE"),
    " (",
    env!("SINDRI_COMMIT_DATE"),
    ")"
);

/// What §15 lets stdout be, from the closed set of two: the answer and one
/// newline, or one compact object on one line.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Format {
    /// The answer, and exactly one newline after it.
    Text,
    /// §15's object: one line, fixed field order, absent fields left out.
    Json,
}

/// The one command line both front ends share.
#[derive(Debug, Parser)]
#[command(
    name = "sindri",
    version = VERSION,
    about = "An agent that is a Unix filter and a terminal UI"
)]
pub struct Cli {
    /// The prompt to answer: this launch is batch mode, and its stdout is the
    /// answer and nothing else (§3).
    ///
    /// With no value the pipe is the prompt; with a value the pipe is context
    /// appended after it.
    #[arg(short = 'b', long = "batch", value_name = "PROMPT", num_args = 0..=1)]
    pub batch: Option<Option<String>>,

    /// The credential to run with, instead of one the environment exports.
    ///
    /// A key given here is visible to anything that can read this process's
    /// arguments, so the environment and a pasted key are the quieter two ways in.
    #[arg(long, value_name = "KEY")]
    pub api_key: Option<String>,

    /// The output format, global so either front end accepts it (§3, A5).
    #[arg(long, global = true, value_name = "FORMAT", default_value = "text")]
    pub format: Format,

    /// Put the run on stderr, ending with the session GUID (§15).
    #[arg(
        short = 'v',
        long = "verbose",
        global = true,
        action = clap::ArgAction::Count
    )]
    pub verbose: u8,
}

impl Cli {
    /// The flag layer the core layers above the file and the environment: only
    /// what a first run can be given, which today is the key.
    pub fn flags(&self) -> Flags {
        Flags {
            api_key: self.api_key.clone(),
            ..Flags::default()
        }
    }

    /// Whether the run is to be narrated on stderr.
    pub fn is_verbose(&self) -> bool {
        self.verbose > 0
    }
}
