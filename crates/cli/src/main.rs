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
    /// Which wire to speak: `anthropic`, or `openai` for OpenAI itself and the
    /// servers that speak its shape — Ollama, vLLM, LM Studio, OpenRouter.
    #[arg(long, value_name = "NAME")]
    provider: Option<String>,
    /// Which palette to draw with: `anvil`, the one there is. `SINDRI_THEME`
    /// names the same key, and the flag wins over the environment.
    #[arg(long, value_name = "NAME")]
    theme: Option<String>,
    /// The model to ask. There is no default, so a run without one is an error
    /// naming the wire it is missing a model for.
    #[arg(long, value_name = "NAME")]
    model: Option<String>,
    /// Another API root: a gateway, a proxy, or a local server. Left out, the
    /// provider's own default is used.
    #[arg(long, value_name = "URL")]
    base_url: Option<String>,
    /// How many turns one run may take before it stops: the turn cap, and exit
    /// 100 when it is reached.
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
    /// How long one command may run inside the sandbox, in seconds (1–3600).
    #[arg(long, value_name = "SECONDS")]
    timeout: Option<u64>,
    /// Pay for the model's reasoning, whatever the config or the environment
    /// says. `--no-thinking` is the other end of the same key.
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
    /// Say what the run did on stderr, after it ran. stdout is the answer only,
    /// so a verbose run pipes exactly like a silent one.
    #[arg(short = 'v', long = "verbose")]
    verbose: bool,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Check the sandbox before a run: bubblewrap, the binaries it needs and
    /// the mount plan, and print the description the model reads.
    Doctor,
    /// The sandbox itself, without the model: run one command in it under the
    /// policy a run would use.
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
    /// Run a command inside the sandbox and print what it wrote.
    Exec {
        /// How long the command may run, in seconds (1–3600).
        #[arg(long, default_value_t = sindri_core::exec::DEFAULT_TIMEOUT_SECS)]
        timeout: u64,
        /// The command and its arguments, after `--`: everything that follows
        /// belongs to the command.
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
    // Read before the match, so the arms share the one value rather than each
    // narrowing the flags its own way.
    let overrides = cli.overrides();
    let code = match cli.command {
        Some(Command::Doctor) => doctor::run(policy_only(&overrides)),
        Some(Command::Sessions { command }) => match command {
            SessionsCommand::List { limit, json } => sessions::list(limit, json),
            SessionsCommand::Show { id, json } => sessions::show(id.as_deref(), json),
        },
        Some(Command::Sandbox {
            command: SandboxCommand::Exec { timeout, command },
        }) => match sindri_core::exec::timeout_from_secs(timeout) {
            Ok(timeout) => sandbox::exec(timeout, policy_only(&overrides), &command),
            Err(message) => failure::usage_error(&message),
        },
        None => match cli.batch {
            Some(prompt) => batch::run(
                prompt,
                batch::Flags {
                    format: cli.format,
                    verbose: cli.verbose,
                    overrides,
                },
            ),
            None => interactive(overrides),
        },
    };
    std::process::exit(code);
}

impl Cli {
    /// The one translation from the parsed command line to the overrides boot
    /// reads: every arm calls it, so a flag is added in one place. A repeatable
    /// flag left empty overrides nothing — it must never replace a configured
    /// set with an empty one. Clap refuses both thinking flags at once, so the
    /// ladder below is one rule rather than one per arm.
    fn overrides(&self) -> Overrides {
        Overrides {
            provider: self.provider.clone(),
            theme: self.theme.clone(),
            model: self.model.clone(),
            base_url: self.base_url.clone(),
            max_turns: self.max_turns,
            timeout: self.timeout,
            thinking: match (self.thinking, self.no_thinking) {
                (true, _) => Some(true),
                (_, true) => Some(false),
                _ => None,
            },
            writable: (!self.writable.is_empty()).then(|| self.writable.clone()),
            extra_headers: (!self.header.is_empty()).then(|| self.header.clone()),
            yolo_unsafe: self.yolo_unsafe,
        }
    }
}

/// The narrower value the doors take — `doctor` and `sandbox exec` are a
/// diagnostic of the sandbox, not a run of the model: they shape the policy and
/// none of the model's dials. Every field is named, so a setting added to
/// `Overrides` is a decision here rather than an omission that silently leaks
/// into a door.
fn policy_only(overrides: &Overrides) -> Overrides {
    Overrides {
        provider: None,
        theme: None,
        model: None,
        base_url: None,
        max_turns: None,
        timeout: None,
        thinking: None,
        writable: overrides.writable.clone(),
        extra_headers: None,
        yolo_unsafe: overrides.yolo_unsafe,
    }
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

#[cfg(test)]
mod tests {
    #![allow(
        clippy::expect_used,
        reason = "a panicking test is how a broken seam is reported"
    )]

    use super::*;

    fn cli(args: &[&str]) -> Cli {
        let mut argv = vec!["sindri"];
        argv.extend_from_slice(args);
        Cli::try_parse_from(argv).expect("a command line the parser takes")
    }

    #[test]
    fn a_parsed_command_line_carries_every_flag_into_the_overrides() {
        let overrides = cli(&[
            "--provider",
            "anthropic",
            "--theme",
            "anvil",
            "--model",
            "claude-sonnet-4-6",
            "--base-url",
            "http://gateway",
            "--max-turns",
            "7",
            "--timeout",
            "9",
            "--writable",
            "/one",
            "--writable",
            "/two",
            "--header",
            "X-Route: beta",
            "--yolo-unsafe",
        ])
        .overrides();

        assert_eq!(overrides.provider.as_deref(), Some("anthropic"));
        assert_eq!(overrides.theme.as_deref(), Some("anvil"));
        assert_eq!(overrides.model.as_deref(), Some("claude-sonnet-4-6"));
        assert_eq!(overrides.base_url.as_deref(), Some("http://gateway"));
        assert_eq!(overrides.max_turns, Some(7));
        assert_eq!(overrides.timeout, Some(9));
        assert_eq!(
            overrides.writable,
            Some(vec!["/one".to_string(), "/two".to_string()])
        );
        assert_eq!(
            overrides.extra_headers,
            Some(vec!["X-Route: beta".to_string()])
        );
        assert!(overrides.yolo_unsafe);
    }

    #[test]
    fn a_flag_left_off_the_command_line_overrides_nothing() {
        let overrides = cli(&[]).overrides();

        assert_eq!(overrides.provider, None);
        assert_eq!(overrides.theme, None);
        assert_eq!(overrides.model, None);
        assert_eq!(overrides.base_url, None);
        assert_eq!(overrides.max_turns, None);
        assert_eq!(overrides.timeout, None);
        assert_eq!(overrides.thinking, None);
        assert_eq!(overrides.writable, None, "no grant replaces no grant");
        assert_eq!(overrides.extra_headers, None);
        assert!(!overrides.yolo_unsafe);
    }

    #[test]
    fn thinking_is_asked_for_by_one_flag_and_refused_by_the_other() {
        assert_eq!(cli(&[]).overrides().thinking, None, "the config decides");
        assert_eq!(
            cli(&["--thinking"]).overrides().thinking,
            Some(true),
            "--thinking asks for it"
        );
        assert_eq!(
            cli(&["--no-thinking"]).overrides().thinking,
            Some(false),
            "--no-thinking refuses it"
        );
    }

    #[test]
    fn the_doors_take_the_sandbox_flags_and_name_the_model_dials_absent() {
        let overrides = policy_only(
            &cli(&[
                "--provider",
                "gemini",
                "--theme",
                "anvil",
                "--model",
                "m",
                "--base-url",
                "http://gateway",
                "--max-turns",
                "7",
                "--timeout",
                "9",
                "--thinking",
                "--writable",
                "/granted",
                "--header",
                "X-Route: beta",
                "--yolo-unsafe",
            ])
            .overrides(),
        );

        assert_eq!(overrides.writable, Some(vec!["/granted".to_string()]));
        assert!(overrides.yolo_unsafe);
        assert_eq!(overrides.provider, None);
        assert_eq!(overrides.theme, None);
        assert_eq!(overrides.model, None);
        assert_eq!(overrides.base_url, None);
        assert_eq!(overrides.max_turns, None);
        assert_eq!(overrides.timeout, None);
        assert_eq!(overrides.thinking, None);
        assert_eq!(overrides.extra_headers, None);
    }
}
