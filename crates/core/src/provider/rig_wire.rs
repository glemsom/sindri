//! The only module that knows the provider library exists (ADR-0003): the
//! Anthropic Messages wire, our own HTTP client with explicit timeouts, and
//! the translation of the library's vocabulary into ours.
//!
//! Nothing rig-shaped leaves this file: callers see [`Answer`] and [`Error`].

use std::time::Duration;

use rig_core::completion::CompletionRequest;
use rig_core::error::ProviderError;
use rig_core::providers::anthropic::AnthropicConfig;
use rig_reqwest::ReqwestClient;
use rig_reqwest::reqwest;

use super::{Answer, Client, Error, Usage};

/// How long the provider may take to accept a connection.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// How long one request may take, streaming a long answer included.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(180);
/// The most tokens one answer may use. A model the provider library does not
/// know has no default, and an unbounded answer is an unbounded bill.
const MAX_OUTPUT_TOKENS: u64 = 8192;
/// The instructions every batch turn carries. The sandbox's own description
/// joins them once the `bash` tool exists (ADR-0002).
const PREAMBLE: &str = "You are Sindri, an assistant answering a developer at a Unix \
terminal. Answer the question you were given, using the context after it when there is \
some. Reply with the answer itself: no preamble, no closing offer of further help.";

/// One completion, straight through: no retries, no redaction, no policy.
pub(super) fn complete(client: &Client, prompt: &str) -> Result<Answer, Error> {
    let (api_key, base_url, model) = client.parts();

    let http = reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|error| Error::transport(format!("could not build the HTTP client: {error}")))?;

    let mut config = AnthropicConfig::new(api_key);
    if let Some(base_url) = base_url {
        config = config.with_base_url(base_url);
    }
    let endpoint = config.connect(ReqwestClient::from(http));
    let model = endpoint.completion(model);

    let request = CompletionRequest::new(prompt)
        .preamble(PREAMBLE)
        .max_tokens(MAX_OUTPUT_TOKENS);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| Error::transport(format!("could not start the runtime: {error}")))?;
    let response = runtime.block_on(model.call(request)).map_err(convert)?;

    Ok(Answer {
        text: response.text(),
        model: response.model().map(str::to_owned),
        usage: Usage {
            input_tokens: response.usage.input_tokens,
            output_tokens: response.usage.output_tokens,
            cached_input_tokens: response.usage.cached_input_tokens,
            total_tokens: response.usage.total_tokens,
        },
    })
}

/// The library's failure, as ours: the provider's own message, and the status
/// that decides whether a retry could help.
fn convert(error: ProviderError) -> Error {
    let report = error.report();
    Error::answered(report.http_status, report.message)
}
