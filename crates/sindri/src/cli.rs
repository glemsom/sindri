//! The command line as clap sees it: only the flags this front end honours today,
//! with `--format` and `-v` accepted globally so a batch run and a TUI launch
//! share one parser rather than growing two.
//!
//! What a first run needs is the credential; everything else the run will take as
//! a flag — a prompt, `--format`'s meaning, `-v`'s — belongs to the tickets that
//! own the run loop and the batch front end (#17, #19).

use clap::Parser;
use sindri_core::config::Flags;

/// The one command line both front ends share.
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

#[derive(Debug, Parser)]
#[command(
    name = "sindri",
    version = VERSION,
    about = "An agent that is a Unix filter and a terminal UI"
)]
pub struct Cli {
    /// The credential to run with, instead of one the environment exports.
    ///
    /// A key given here is visible to anything that can read this process's
    /// arguments, so the environment and a pasted key are the quieter two ways in.
    #[arg(long, value_name = "KEY")]
    pub api_key: Option<String>,

    /// The output format. Accepted and ignored until the run loop lands (§3).
    #[arg(long, global = true, value_name = "FORMAT", default_value = "text")]
    pub format: String,

    /// Put the run on stderr. Accepted and ignored until the run loop lands (§3).
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
}
