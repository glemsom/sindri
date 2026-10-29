//! The `sindri` binary: the batch front end. Argument parsing and output only;
//! every decision lives in `sindri-core`.

mod doctor;
mod sandbox;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "sindri", version = sindri_types::VERSION_LONG, about = "A Unix filter with an agent behind it")]
struct Cli {
    #[command(subcommand)]
    command: Command,
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
    let code = match cli.command {
        Command::Doctor => doctor::run(),
        Command::Sandbox {
            command: SandboxCommand::Exec { timeout, command },
        } => match sindri_core::exec::timeout_from_secs(timeout) {
            Ok(timeout) => sandbox::exec(timeout, &command),
            Err(message) => usage_error(&message),
        },
    };
    std::process::exit(code);
}

/// Rejects a bad argument the way the exit-code contract requires: exit 2, one
/// line on stderr.
fn usage_error(message: &str) -> i32 {
    eprintln!("sindri: {message}");
    sindri_types::ExitCode::UsageError.code()
}
