//! Explicit sign-in for provider credentials, kept outside a model run.

use rig_core::providers::copilot::auth::{
    AuthSource, Authenticator, DeviceCodeHandler, FileTokenStore,
};
use rig_reqwest::ReqwestClient;

/// Start the GitHub Copilot device-code flow and persist its credentials.
pub fn login(provider: &str) -> i32 {
    if provider != "copilot" {
        eprintln!("sindri: `auth login` supports `copilot`");
        return 2;
    }
    let state = match sindri_core::paths::state_dir() {
        Ok(state) => state,
        Err(error) => {
            eprintln!("sindri: could not locate credential storage: {error}");
            return 1;
        }
    };
    let authenticator = Authenticator::new(
        AuthSource::OAuth,
        FileTokenStore::new(state.join("copilot")),
        DeviceCodeHandler::new(|prompt| {
            eprintln!(
                "GitHub Copilot sign-in: visit {} and enter {}",
                prompt.verification_uri, prompt.user_code
            );
        }),
        true,
    );
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("sindri: could not start the sign-in runtime: {error}");
            return 1;
        }
    };
    match runtime.block_on(authenticator.auth_context(&ReqwestClient::default())) {
        Ok(_) => {
            eprintln!("sindri: GitHub Copilot sign-in complete");
            0
        }
        Err(error) => {
            eprintln!("sindri: GitHub Copilot sign-in failed: {error}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn auth_login_refuses_an_unknown_provider_without_starting_oauth() {
        assert_eq!(super::login("openai"), 2);
    }
}
