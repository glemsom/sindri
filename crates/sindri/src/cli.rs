use clap::{Parser, Subcommand, ValueEnum};
use sindri_core::config::{Flags, WINDOW_REFUSAL};
use sindri_core::policy::Enforcement;
use sindri_core::provider::Route;

/// The `git describe` and commit date `build.rs` fixes at compile time, never read from a
/// clock. A build standing on no commit — a source tarball — says it knows no date.
pub const VERSION: &str = concat!(
    env!("SINDRI_DESCRIBE"),
    " (",
    env!("SINDRI_COMMIT_DATE"),
    ")"
);

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Format {
    /// The answer, and exactly one newline after it.
    Text,
    /// One line, fixed field order, absent fields left out.
    Json,
}

#[derive(Debug, Parser)]
#[command(
    name = "sindri",
    version = VERSION,
    about = "An agent that is a Unix filter and a terminal UI"
)]
pub struct Cli {
    /// The prompt to answer, which makes this launch batch mode.
    ///
    /// With no value the pipe is the prompt; with a value the pipe is context after it.
    #[arg(short = 'b', long = "batch", value_name = "PROMPT", num_args = 0..=1)]
    pub batch: Option<Option<String>>,

    /// The credential to run with, instead of one the environment exports.
    ///
    /// Visible to anything that can read this process's arguments.
    #[arg(long, value_name = "KEY")]
    pub api_key: Option<String>,

    /// The completion endpoint to take, for a wire that serves two.
    ///
    /// Replaces a `route` the config file holds, as a repeated flag does.
    #[arg(long, value_name = "ROUTE", value_enum)]
    pub route: Option<RouteWord>,

    /// How many tokens a conversation may fill, replacing the `context_window` in force.
    #[arg(long = "context-window", value_name = "TOKENS", value_parser = window_tokens)]
    pub context_window: Option<u64>,

    /// Run without a sandbox at all: no config key and no environment variable, since a
    /// persisted unsafe setting is one the user forgets.
    #[arg(long = "yolo-unsafe")]
    pub yolo_unsafe: bool,

    /// The output format, global so either front end accepts it.
    #[arg(long, global = true, value_name = "FORMAT", default_value = "text")]
    pub format: Format,

    /// Put the run on stderr, ending with the session GUID.
    #[arg(
        short = 'v',
        long = "verbose",
        global = true,
        action = clap::ArgAction::Count
    )]
    pub verbose: u8,

    /// What to do instead of running.
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Sign in to a provider rather than run one.
    Auth {
        #[command(subcommand)]
        action: AuthAction,
    },
    /// Prove the sandbox and say what the model is given, rather than run.
    Doctor,
    /// Read past runs back rather than start one.
    Sessions {
        /// What to do with the recorded sessions.
        #[command(subcommand)]
        action: SessionsAction,
    },
}

#[derive(Debug, Subcommand)]
pub enum SessionsAction {
    /// List every recorded session, newest first.
    List,
    /// Read one recorded session back: the newest when no id is given.
    Show {
        /// A whole GUID, or a prefix of exactly one.
        #[arg(value_name = "ID")]
        id: Option<String>,
        /// Print the session's files in their own shape, for `jq`.
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum AuthAction {
    /// Sign in, showing the code the device flow asks a person to type.
    Login {
        /// The wire to sign in to: `copilot` is the only one that signs in.
        #[arg(value_name = "WIRE", value_enum)]
        wire: LoginWire,
    },
}

/// `anthropic` and `openai` are asked with a key rather than signed in to, so the parser has
/// no word for them and cannot be handed one.
#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum LoginWire {
    Copilot,
}

fn window_tokens(text: &str) -> Result<u64, String> {
    let tokens: u64 = text
        .parse::<u64>()
        .map_err(|error: std::num::ParseIntError| error.to_string())?;
    if tokens == 0 {
        return Err(String::from(WINDOW_REFUSAL));
    }
    Ok(tokens)
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum RouteWord {
    /// `POST /chat/completions`.
    Chat,
    /// `POST /responses`.
    Responses,
}

impl RouteWord {
    pub fn route(self) -> Route {
        match self {
            RouteWord::Chat => Route::Chat,
            RouteWord::Responses => Route::Responses,
        }
    }
}

impl Cli {
    pub fn flags(&self) -> Flags {
        Flags {
            api_key: self.api_key.clone(),
            route: self.route.map(RouteWord::route),
            context_window: self.context_window,
            ..Flags::default()
        }
    }

    pub fn is_verbose(&self) -> bool {
        self.verbose > 0
    }

    /// The only route to `unsafe` there is: `tests/sandbox.rs` scans the tree and holds that
    /// no other file in this crate names the variant.
    pub fn enforcement(&self) -> Enforcement {
        if self.yolo_unsafe {
            Enforcement::Unsafe
        } else {
            Enforcement::Bubblewrap
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_context_window_flag_is_a_count_of_tokens_the_run_is_handed() {
        let cli = Cli::try_parse_from(["sindri", "--context-window", "200000"])
            .expect("a window of tokens is a flag this parser holds");

        assert_eq!(
            cli.flags().context_window,
            Some(200_000),
            "a window named on the command line is read above the file and the environment, \
             which is how every other run dial gets in"
        );
    }

    #[test]
    fn a_context_window_of_nothing_on_the_command_line_is_refused() {
        let error = Cli::try_parse_from(["sindri", "--context-window", "0"])
            .expect_err("a window of no tokens is a share nothing can be taken of");

        assert_eq!(
            error.exit_code(),
            2,
            "a value that will not do is a usage error"
        );
        assert!(
            error.to_string().contains("at least one token"),
            "the refusal says the smallest window that would do, and it says: {error}"
        );
    }
}
