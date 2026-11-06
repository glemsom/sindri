//! The command line as clap sees it: only the flags this front end honours today,
//! with `--format` and `-v` accepted globally so a batch run and a TUI launch
//! share one parser rather than growing two.
//!
//! `-b` is what makes a launch batch mode (§3), and its value is optional because
//! a pipe can be the prompt instead. `--format` is one of two words and nothing
//! else: a third spelling would be a format no run can honour, so the parser is
//! where it is refused, at exit 2.
//!
//! `--route` names the completion endpoint a wire that serves two is sent on,
//! in the two words the core accepts, so a misbehaving route can be got round
//! without renaming the model (Appendix A10).
//!
//! `auth login` is a subcommand rather than a flag because it is the one thing
//! this binary does that runs nothing (§3): it signs in and exits. The words it
//! offers are the words the core accepts, so a wire that cannot sign in has no
//! word here to be typed.

use clap::{Parser, Subcommand, ValueEnum};
use sindri_core::config::Flags;
use sindri_core::policy::Enforcement;
use sindri_core::provider::Route;

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

    /// The completion endpoint to take, for a wire that serves two: `chat` or
    /// `responses`, so a route that misbehaves can be got round by name (A10).
    /// Replaces a `route` the config file holds, as a repeated flag does.
    #[arg(long, value_name = "ROUTE", value_enum)]
    pub route: Option<RouteWord>,

    /// Run without a sandbox at all: the one escape hatch (§6).
    ///
    /// It has no config key and no environment variable, so it dies with the
    /// invocation that asked for it: a persisted unsafe setting is one the user
    /// forgets. Nothing it enables is persisted either — the record, the JSON and
    /// the rail say `unsafe` and a later run says `bubblewrap` again.
    #[arg(long = "yolo-unsafe")]
    pub yolo_unsafe: bool,

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

    /// What to do instead of running. Today, that is one thing: sign in.
    #[command(subcommand)]
    pub command: Option<Command>,
}

/// The one command that signs in rather than runs (§3).
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Sign in to a provider rather than run one.
    Auth {
        /// What to do with the sign-in.
        #[command(subcommand)]
        action: AuthAction,
    },
}

/// What `sindri auth` can do. Signing out arrives with the ticket that needs it.
#[derive(Debug, Subcommand)]
pub enum AuthAction {
    /// Sign in, showing the code the device flow asks a person to type.
    Login {
        /// The wire to sign in to: `copilot` is the only one that signs in.
        #[arg(value_name = "WIRE", value_enum)]
        wire: LoginWire,
    },
}

/// The wires that sign in. `anthropic` and `openai` are asked with a key, which
/// is set rather than signed in for (§10), so the parser has no word for them
/// and cannot be handed one.
#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum LoginWire {
    /// GitHub Copilot, behind its own OAuth session.
    Copilot,
}

/// The routes a launch may name, as clap sees them: the two words the core
/// accepts, so no third spelling can be typed, and nothing here decides which
/// endpoint a name is served by — that is rig's (Appendix A10).
#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum RouteWord {
    /// `POST /chat/completions`.
    Chat,
    /// `POST /responses`.
    Responses,
}

impl RouteWord {
    /// The core's route this word names.
    pub fn route(self) -> Route {
        match self {
            RouteWord::Chat => Route::Chat,
            RouteWord::Responses => Route::Responses,
        }
    }
}

impl Cli {
    /// The flag layer the core layers above the file and the environment: only
    /// what this front end parses, which is the key and the route.
    pub fn flags(&self) -> Flags {
        Flags {
            api_key: self.api_key.clone(),
            route: self.route.map(RouteWord::route),
            ..Flags::default()
        }
    }

    /// Whether the run is to be narrated on stderr.
    pub fn is_verbose(&self) -> bool {
        self.verbose > 0
    }

    /// How this launch's commands are confined, as the two-valued fact it is.
    ///
    /// This is the only route to `unsafe` there is: the flag is this parser's,
    /// the boundary is built from this value, and no other file in this crate
    /// names that variant at all — the core names it only where the fact is
    /// defined, enforced and proved, and nowhere a file or a variable could
    /// reach it. `tests/sandbox.rs` scans the tree and holds that sentence.
    pub fn enforcement(&self) -> Enforcement {
        if self.yolo_unsafe {
            Enforcement::Unsafe
        } else {
            Enforcement::Bubblewrap
        }
    }
}
