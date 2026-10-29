//! The `sindri` binary: the batch front end. Argument parsing and output only;
//! every decision lives in `sindri-core`.

mod doctor;

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
}

fn main() {
    let cli = Cli::parse();
    let code = match cli.command {
        Command::Doctor => doctor::run(),
    };
    std::process::exit(code);
}
