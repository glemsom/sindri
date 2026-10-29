//! Batch mode: the Unix filter front end. It reads the prompt and the stdin
//! context, asks the model and prints the answer plus exactly one newline;
//! everything else is a decision made in `sindri-core`. Exit codes, not prose,
//! tell a script what happened (ADR-0004).

use std::io::{IsTerminal, Read};

use sindri_core::config::{Config, ConfigError, Overrides};
use sindri_core::provider::{Client, Error};
use sindri_types::ExitCode;

/// The flag overrides `sindri -b` accepts, taken from the command line as
/// written.
#[derive(Debug, Default)]
pub struct Flags {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub base_url: Option<String>,
    pub max_turns: Option<u32>,
    pub timeout: Option<u64>,
}

impl From<Flags> for Overrides {
    fn from(flags: Flags) -> Self {
        Overrides {
            provider: flags.provider,
            model: flags.model,
            base_url: flags.base_url,
            max_turns: flags.max_turns,
            timeout: flags.timeout,
        }
    }
}

/// One batch run. Returns the exit code the shell sees.
pub fn run(prompt: Option<String>, flags: Flags) -> i32 {
    let prompt = match composed_prompt(prompt) {
        Ok(prompt) => prompt,
        Err(message) => return usage_error(&message),
    };

    let config = match Config::resolve(flags.into()) {
        Ok(config) => config,
        Err(error) => return config_error(&error),
    };

    let client = Client::new(&config);
    match client.ask(&prompt) {
        Ok(answer) => {
            // The answer and exactly one newline, so `| wc -l` counts answers.
            println!("{}", answer.text.trim_end());
            ExitCode::Success.code()
        }
        Err(error) => model_error(&error),
    }
}

/// The question actually asked: the prompt argument, with piped stdin appended
/// after it as context, or stdin itself when there is no argument (ADR-0004).
fn composed_prompt(prompt: Option<String>) -> Result<String, String> {
    let piped = read_stdin()?;
    let prompt = prompt.filter(|prompt| !prompt.trim().is_empty());
    let context = piped.filter(|context| !context.trim().is_empty());

    match (prompt, context) {
        (Some(prompt), Some(context)) => {
            Ok(format!("{}\n\n{}", prompt.trim_end(), context.trim_end()))
        }
        (Some(prompt), None) => Ok(prompt),
        (None, Some(context)) => Ok(context.trim_end().to_string()),
        (None, None) => Err(r#"no prompt: pass one with -b "…" or pipe one in"#.to_string()),
    }
}

/// Piped stdin, read only when there is a pipe to read: a terminal is not
/// drained, and neither is stdin when the run never reaches batch mode.
fn read_stdin() -> Result<Option<String>, String> {
    if std::io::stdin().is_terminal() {
        return Ok(None);
    }
    let mut text = String::new();
    std::io::stdin()
        .read_to_string(&mut text)
        .map_err(|error| format!("could not read stdin: {error}"))?;
    Ok(Some(text))
}

/// A wrong command line or a wrong value: exit 2.
pub fn usage_error(message: &str) -> i32 {
    eprintln!("sindri: {message}");
    ExitCode::UsageError.code()
}

/// A configuration value that is wrong is a usage error; one that is absent is
/// a model error with a hint, as the batch contract promises.
fn config_error(error: &ConfigError) -> i32 {
    eprintln!("sindri: {}", error.message());
    match error {
        ConfigError::Usage(_) => ExitCode::UsageError.code(),
        ConfigError::Missing(_) => ExitCode::ModelError.code(),
    }
}

/// A provider or transport failure: the provider's message, one line, exit 1.
fn model_error(error: &Error) -> i32 {
    match error.status() {
        Some(status) => eprintln!(
            "sindri: provider error (HTTP {status}): {}",
            error.message()
        ),
        None => eprintln!("sindri: provider error: {}", error.message()),
    }
    ExitCode::ModelError.code()
}
