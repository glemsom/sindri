//! The `sindri` binary: the batch front end. Argument parsing and output only;
//! every decision lives in `sindri-core`.

mod batch;
mod doctor;
mod sandbox;
mod verbose;

use std::io::IsTerminal;

use clap::{CommandFactory, Parser, Subcommand};

#[derive(Parser)]
#[command(name = "sindri", version = sindri_types::VERSION_LONG, about = "A Unix filter with an agent behind it")]
struct Cli {
    /// Batch mode: ask the model and print the answer. The prompt is the
    /// argument; piped stdin is appended after it as context. With no
    /// argument, stdin is the prompt.
    #[arg(short = 'b', long = "batch", value_name = "PROMPT", num_args = 0..=1)]
    batch: Option<Option<String>>,
    /// Override the configured provider (`anthropic`).
    #[arg(long, value_name = "NAME")]
    provider: Option<String>,
    /// Override the configured model.
    #[arg(long, value_name = "NAME")]
    model: Option<String>,
    /// Override the configured provider base URL.
    #[arg(long, value_name = "URL")]
    base_url: Option<String>,
    /// Override how many turns the run may take.
    #[arg(long, value_name = "N")]
    max_turns: Option<u32>,
    /// Grant an extra directory read-write inside the sandbox. Repeatable;
    /// replaces the configured extras rather than adding to them.
    #[arg(long, value_name = "PATH")]
    writable: Vec<String>,
    /// Override how long one command may run, in seconds.
    #[arg(long, value_name = "SECONDS")]
    timeout: Option<u64>,
    /// Pay for the model's reasoning. Off by default, because cheap tasks
    /// should stay cheap.
    #[arg(long, conflicts_with = "no_thinking")]
    thinking: bool,
    /// Do not pay for the model's reasoning, whatever the config or the
    /// environment says.
    #[arg(long = "no-thinking")]
    no_thinking: bool,
    /// Report the run on stderr when it is over: reasoning, one line per tool
    /// call, and the session it was recorded in. stdout is untouched.
    #[arg(short = 'v', long = "verbose")]
    verbose: bool,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Prove the sandbox works and print the description the model sees.
    Doctor,
    /// Diagnostics for the sandbox itself, with no model in the loop.
    Sandbox {
        #[command(subcommand)]
        command: SandboxCommand,
    },
}

#[derive(Subcommand)]
enum SandboxCommand {
    /// Run one command inside the sandbox and print the structured outcome.
    Exec {
        /// Seconds the command may run before its process group is signalled.
        #[arg(long, default_value_t = sindri_core::exec::DEFAULT_TIMEOUT_SECS)]
        timeout: u64,
        /// The command and its arguments.
        #[arg(last = true, required = true, num_args = 1..)]
        command: Vec<String>,
    },
}

fn main() {
    let cli = Cli::parse();
    if cli.batch.is_some() && cli.command.is_some() {
        std::process::exit(batch::usage_error(
            "batch mode takes no subcommand: `sindri -b \"your prompt\"`",
        ));
    }
    let code = match cli.command {
        Some(Command::Doctor) => doctor::run(),
        Some(Command::Sandbox {
            command: SandboxCommand::Exec { timeout, command },
        }) => match sindri_core::exec::timeout_from_secs(timeout) {
            Ok(timeout) => sandbox::exec(timeout, &command),
            Err(message) => batch::usage_error(&message),
        },
        None => match cli.batch {
            Some(prompt) => batch::run(
                prompt,
                batch::Flags {
                    provider: cli.provider,
                    model: cli.model,
                    base_url: cli.base_url,
                    max_turns: cli.max_turns,
                    timeout: cli.timeout,
                    writable: (!cli.writable.is_empty()).then_some(cli.writable),
                    thinking: match (cli.thinking, cli.no_thinking) {
                        (true, _) => Some(true),
                        (_, true) => Some(false),
                        _ => None,
                    },
                    verbose: cli.verbose,
                },
            ),
            None => refuse_stdin(),
        },
    };
    std::process::exit(code);
}

/// A non-batch launch with a pipe is refused rather than drained: data piped
/// into a program that ignores it is data lost without a word (ADR-0004).
fn refuse_stdin() -> i32 {
    if std::io::stdin().is_terminal() {
        // A bare invocation is a usage error, so its help goes where errors go:
        // stdout stays empty for a script that ran the wrong thing.
        let help = Cli::command().render_help();
        eprint!("{help}");
        return sindri_types::ExitCode::UsageError.code();
    }
    batch::usage_error(r#"piped stdin needs -b: `… | sindri -b "your prompt"`"#)
}
