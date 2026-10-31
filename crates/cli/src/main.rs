//! The `sindri` binary: the batch front end. Argument parsing and output only;
//! every decision lives in `sindri-core`.

mod batch;
mod boot;
mod doctor;
mod failure;
mod frame;
mod notice;
mod sandbox;
mod sessions;
mod signal;
mod verbose;

use std::io::IsTerminal;

use clap::{Parser, Subcommand};
use sindri_core::config::Overrides;

#[derive(Parser)]
#[command(name = "sindri", version = sindri_types::VERSION_LONG, about = "A Unix filter with an agent behind it")]
struct Cli {
    /// Batch mode: ask the model and print the answer. The prompt is the
    /// argument; piped stdin is appended after it as context. With no
    /// argument, stdin is the prompt.
    #[arg(short = 'b', long = "batch", value_name = "PROMPT", num_args = 0..=1)]
    batch: Option<Option<String>>,
    #[arg(long, value_name = "NAME")]
    provider: Option<String>,
    #[arg(long, value_name = "NAME")]
    theme: Option<String>,
    #[arg(long, value_name = "NAME")]
    model: Option<String>,
    #[arg(long, value_name = "URL")]
    base_url: Option<String>,
    #[arg(long, value_name = "N")]
    max_turns: Option<u32>,
    /// Grant an extra directory read-write inside the sandbox. Repeatable;
    /// replaces the configured extras rather than adding to them.
    #[arg(long, value_name = "PATH")]
    writable: Vec<String>,
    /// Send a header with every provider request, as `Name: value`. Repeatable;
    /// replaces the configured headers rather than adding to them. A gateway
    /// that routes on a header of its own needs one.
    #[arg(long, value_name = "NAME: VALUE")]
    header: Vec<String>,
    #[arg(long, value_name = "SECONDS")]
    timeout: Option<u64>,
    #[arg(long, conflicts_with = "no_thinking")]
    thinking: bool,
    /// Do not pay for the model's reasoning, whatever the config or the
    /// environment says.
    #[arg(long = "no-thinking")]
    no_thinking: bool,
    /// Leave the sandbox: commands run with your own privileges, outside bubblewrap.
    #[arg(long = "yolo-unsafe", global = true)]
    yolo_unsafe: bool,
    /// Shape of the run's output: `text` is the answer plus one newline,
    /// `json` is one compact object for a script.
    #[arg(long, value_name = "FORMAT", default_value = "text")]
    format: batch::OutputFormat,
    #[arg(short = 'v', long = "verbose")]
    verbose: bool,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    Doctor,
    Sandbox {
        #[command(subcommand)]
        command: SandboxCommand,
    },
    /// The record a run left behind.
    Sessions {
        #[command(subcommand)]
        command: SessionsCommand,
    },
}

#[derive(Subcommand)]
enum SessionsCommand {
    /// One row per record, newest first.
    List {
        /// Print at most N rows. No default: a list cuts nothing unless told.
        #[arg(long, value_name = "N")]
        limit: Option<usize>,
        /// One compact object per line, the fields in the row's order.
        #[arg(long)]
        json: bool,
    },
    /// One record whole: the newest run when no id is named.
    Show {
        /// A session id, or a leading prefix of one.
        id: Option<String>,
        /// The record as one compact object: its metadata, its entries and,
        /// when the run left any, bubblewrap's own documents.
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum SandboxCommand {
    Exec {
        #[arg(long, default_value_t = sindri_core::exec::DEFAULT_TIMEOUT_SECS)]
        timeout: u64,
        #[arg(last = true, required = true, num_args = 1..)]
        command: Vec<String>,
    },
}

fn main() {
    let cli = Cli::parse();
    if cli.batch.is_some() && cli.command.is_some() {
        std::process::exit(failure::usage_error(
            "batch mode takes no subcommand: `sindri -b \"your prompt\"`",
        ));
    }
    let code = match cli.command {
        Some(Command::Doctor) => doctor::run(Overrides {
            writable: (!cli.writable.is_empty()).then_some(cli.writable),
            yolo_unsafe: cli.yolo_unsafe,
            ..Overrides::default()
        }),
        Some(Command::Sessions { command }) => match command {
            SessionsCommand::List { limit, json } => sessions::list(limit, json),
            SessionsCommand::Show { id, json } => sessions::show(id.as_deref(), json),
        },
        Some(Command::Sandbox {
            command: SandboxCommand::Exec { timeout, command },
        }) => match sindri_core::exec::timeout_from_secs(timeout) {
            Ok(timeout) => sandbox::exec(
                timeout,
                Overrides {
                    writable: (!cli.writable.is_empty()).then_some(cli.writable),
                    yolo_unsafe: cli.yolo_unsafe,
                    ..Overrides::default()
                },
                &command,
            ),
            Err(message) => failure::usage_error(&message),
        },
        None => match cli.batch {
            Some(prompt) => batch::run(
                prompt,
                batch::Flags {
                    format: cli.format,
                    provider: cli.provider,
                    theme: cli.theme,
                    model: cli.model,
                    base_url: cli.base_url,
                    max_turns: cli.max_turns,
                    timeout: cli.timeout,
                    writable: (!cli.writable.is_empty()).then_some(cli.writable),
                    extra_headers: (!cli.header.is_empty()).then_some(cli.header),
                    yolo_unsafe: cli.yolo_unsafe,
                    thinking: match (cli.thinking, cli.no_thinking) {
                        (true, _) => Some(true),
                        (_, true) => Some(false),
                        _ => None,
                    },
                    verbose: cli.verbose,
                },
            ),
            None => {
                let overrides = Overrides {
                    provider: cli.provider,
                    theme: cli.theme,
                    model: cli.model,
                    base_url: cli.base_url,
                    max_turns: cli.max_turns,
                    timeout: cli.timeout,
                    thinking: match (cli.thinking, cli.no_thinking) {
                        (true, _) => Some(true),
                        (_, true) => Some(false),
                        _ => None,
                    },
                    writable: (!cli.writable.is_empty()).then_some(cli.writable),
                    extra_headers: (!cli.header.is_empty()).then_some(cli.header),
                    yolo_unsafe: cli.yolo_unsafe,
                };
                interactive(overrides)
            }
        },
    };
    std::process::exit(code);
}

/// A non-batch launch: the terminal UI on a terminal, and a refusal otherwise.
/// A pipe without `-b` is refused rather than drained — data piped into a
/// program that ignores it is data lost without a word.
fn interactive(overrides: Overrides) -> i32 {
    if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
        return frame::run(overrides);
    }
    failure::usage_error(r#"piped stdin needs -b: `… | sindri -b "your prompt"`"#)
}
