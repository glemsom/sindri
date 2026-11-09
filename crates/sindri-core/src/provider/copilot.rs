use std::fmt;
use std::path::{Path, PathBuf};

use rig_core::http_client::{DynHttpClient, Error as HttpError, HttpClientExt};
use rig_core::providers::copilot::PROVIDER_NAME;
use rig_core::providers::copilot::auth::{
    AuthError, AuthSource, Authenticator, DeviceCodeHandler, FileTokenStore, TokenStore,
};

use super::Credential;
use crate::config::{self, ConfigError, Environment};
use crate::redact::{Redacted, SecretSet};

pub const LOGIN: &str = "sindri auth login copilot";

/// Owner-only, because every byte of the record is a credential.
pub const MODE: u32 = 0o600;

const SESSION_FIELD: &str = "token";

const GITHUB_FIELD: &str = "github_token";

/// A wire refuses a credential with these: a sign-in gone bad, not a wire gone quiet.
const REFUSED: [u16; 2] = [401, 403];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceCode {
    pub verification_uri: String,
    pub user_code: String,
}

impl fmt::Display for DeviceCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "visit {} and enter {}",
            self.verification_uri, self.user_code
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SignIn {
    Absent,
    Revoked,
    Unavailable(Redacted),
    Denied(Redacted),
}

impl SignIn {
    pub fn message(&self) -> Redacted {
        match self {
            SignIn::Absent => {
                SecretSet::default().redact(format!("no copilot sign-in yet: run `{LOGIN}`"))
            }
            SignIn::Revoked => SecretSet::default().redact(format!(
                "the copilot sign-in is no longer good: run `{LOGIN}`"
            )),
            SignIn::Unavailable(reason) | SignIn::Denied(reason) => reason.clone(),
        }
    }
}

impl fmt::Display for SignIn {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message().to_string())
    }
}

#[derive(Clone)]
pub struct Store {
    /// rig's store of records: one owner-only file per provider, written whole or not
    /// at all, with a lock another process waits on rather than racing a refresh with.
    records: FileTokenStore,
    transport: DynHttpClient,
}

impl Store {
    pub fn over(transport: impl HttpClientExt + Clone + 'static, dir: impl Into<PathBuf>) -> Store {
        Store {
            records: FileTokenStore::new(dir),
            transport: DynHttpClient::new(transport),
        }
    }

    pub fn resolve(environment: &impl Environment) -> Result<Store, ConfigError> {
        Ok(Store::over(
            rig_reqwest::ReqwestClient::default(),
            store_dir(environment)?,
        ))
    }

    pub fn path(&self) -> PathBuf {
        self.records.path(PROVIDER_NAME)
    }

    /// A session to ask the wire with, refreshed when the one on file has expired: a run
    /// never starts a sign-in of its own, because it has no one to show a code to.
    pub fn session(&self) -> Result<Credential, SignIn> {
        let record = self.record();
        let secrets = secrets_of(&record);
        // Read before the resolve: rig drops a token the exchange refused, and
        // afterwards the store cannot say whether it ever held one — the
        // difference between a sign-in the wire will not take and none at all.
        let had_sign_in = holds_github_token(&record);
        let authenticator = self.authenticator(DeviceCodeHandler::default(), false);
        match futures::executor::block_on(authenticator.auth_context(&self.transport)) {
            Ok(context) => Ok(Credential::new(context.api_key.expose())),
            Err(error) => Err(checked(error, had_sign_in, &secrets)),
        }
    }

    /// Sign in with the device flow, showing the code through `show` as GitHub hands it
    /// over, and keep the session it ends with. Blocks while GitHub is polled, which is what
    /// the wait between polls is.
    pub fn login(&self, show: impl Fn(DeviceCode) + Send + Sync + 'static) -> Result<(), SignIn> {
        let secrets = self.secrets();
        let code = DeviceCodeHandler::new(move |prompt| {
            show(DeviceCode {
                verification_uri: prompt.verification_uri,
                user_code: prompt.user_code,
            });
        });
        let authenticator = self.authenticator(code, true);
        let signed_in = futures::executor::block_on(authenticator.auth_context(&self.transport));
        signed_in.map(|_| ()).map_err(|error| {
            SignIn::Denied(secrets.redact(format!("the copilot sign-in did not finish: {error}")))
        })
    }

    /// Every literal the record holds that no window may print, read out of the record
    /// as it lies on disk rather than from any field this crate knows the meaning of.
    pub fn secrets(&self) -> SecretSet {
        secrets_of(&self.record())
    }

    fn authenticator(&self, code: DeviceCodeHandler, device_flow: bool) -> Authenticator {
        Authenticator::new(AuthSource::OAuth, self.records.clone(), code, device_flow)
    }

    fn record(&self) -> Option<serde_json::Value> {
        let text = futures::executor::block_on(self.records.load(PROVIDER_NAME))
            .ok()
            .flatten()?;
        serde_json::from_str(&text).ok()
    }
}

impl fmt::Debug for Store {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Store")
            .field("path", &self.path())
            .finish_non_exhaustive()
    }
}

pub fn store_dir(environment: &impl Environment) -> Result<PathBuf, ConfigError> {
    Ok(config::config_path(environment)?.with_file_name("auth"))
}

pub fn store_row(path: &Path) -> String {
    format!(
        "copilot session   written to {}, mode {:04o}\n                  \
         and is never shown again",
        path.display(),
        MODE
    )
}

fn secrets_of(record: &Option<serde_json::Value>) -> SecretSet {
    let mut secrets = SecretSet::default();
    for field in [SESSION_FIELD, GITHUB_FIELD] {
        if let Some(secret) = field_of(record, field) {
            secrets.add(secret);
        }
    }
    secrets
}

fn holds_github_token(record: &Option<serde_json::Value>) -> bool {
    field_of(record, GITHUB_FIELD).is_some()
}

fn field_of<'record>(
    record: &'record Option<serde_json::Value>,
    field: &str,
) -> Option<&'record str> {
    record
        .as_ref()?
        .get(field)?
        .as_str()
        .filter(|value| !value.is_empty())
}

fn checked(error: AuthError, had_sign_in: bool, secrets: &SecretSet) -> SignIn {
    match &error {
        AuthError::Http(refusal) if refused(refusal) => SignIn::Revoked,
        AuthError::Message(_) if had_sign_in => SignIn::Revoked,
        AuthError::Message(_) => SignIn::Absent,
        _ => SignIn::Unavailable(
            secrets.redact(format!("the copilot sign-in cannot be checked: {error}")),
        ),
    }
}

fn refused(error: &HttpError) -> bool {
    error
        .non_success_status()
        .is_some_and(|status| REFUSED.contains(&status.as_u16()))
}
