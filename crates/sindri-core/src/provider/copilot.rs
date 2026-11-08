//! The Copilot sign-in: rig's OAuth device flow, the code a person is asked to
//! enter, and the owner-only file the session it keeps is written to.
//!
//! A Copilot session is not an API key and is never read back as one (§10): it
//! lives in `$XDG_CONFIG_HOME/sindri/auth/copilot.json`, owner-only, while §11's
//! `config.toml` stays the only file `api_key` is ever read from. Nothing here
//! prints a record: [`Store::secrets`] holds every credential the record keeps,
//! and it is the set a `Redacted` scrubs through, so the session and the GitHub
//! token behind it leave through the mask or not at all (ADR-0011).
//!
//! The flow runs through seams a test owns — the transport a sign-in's requests
//! travel over, the file the record is kept in, and the callback that shows the
//! code — so the code, the wait between polls, the `0600` file, a transparent
//! refresh and a revoked session are all proved with no socket and no account.
//!
//! What a resolve failure means is decided here rather than by the library's
//! wording: a store holding nothing, a sign-in the wire will not take, and a
//! store that will not open are three different next steps, and each is said in
//! Sindri's own words so a run can print it and go no further (§10, §15).

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

/// The command that makes any of this fixable, spelled once so the failure a run
/// prints and the row a sign-in ends with cannot drift apart.
pub const LOGIN: &str = "sindri auth login copilot";

/// The one mode a sign-in is kept at: owner-only, because every byte of the
/// record is a credential. The store writes it; a test asserts it against the
/// file on disk rather than trusting this number.
pub const MODE: u32 = 0o600;

/// The record's Copilot session, as the record rig keeps spells the field.
const SESSION_FIELD: &str = "token";

/// The GitHub token that session was exchanged for, in the same record.
const GITHUB_FIELD: &str = "github_token";

/// The one status a wire refuses a credential with, which is a sign-in gone bad
/// rather than a wire gone quiet.
const REFUSED: [u16; 2] = [401, 403];

/// The code a person is asked to enter, and where to enter it: the one thing a
/// sign-in shows while it waits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceCode {
    /// Where the person goes.
    pub verification_uri: String,
    /// What they type there.
    pub user_code: String,
}

impl fmt::Display for DeviceCode {
    /// One line a person can follow, code included.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "visit {} and enter {}",
            self.verification_uri, self.user_code
        )
    }
}

/// Why a Copilot session could not be resolved, as a fact a person can act on
/// rather than as a library's error: a sign-in that never happened, one the wire
/// will not take, and a store that will not open are three different next steps.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SignIn {
    /// The store holds no sign-in at all.
    Absent,
    /// A sign-in is on file and the wire would not take it, so a new one is what
    /// fixes it and no retry will.
    Revoked,
    /// The sign-in could not be checked: the store would not open, or GitHub
    /// could not be reached.
    Unavailable(Redacted),
    /// A sign-in was attempted and did not finish: the code expired, the person
    /// denied it, or the exchange was refused.
    Denied(Redacted),
}

impl SignIn {
    /// One line naming what happened and the command that fixes it, safe to
    /// print because a reason is carried scrubbed.
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

/// The sign-in itself: the record rig keeps on disk, and the transport its
/// requests travel over.
#[derive(Clone)]
pub struct Store {
    /// rig's store of records: one owner-only file per provider, written whole
    /// or not at all, with a lock another process waits on rather than racing
    /// a refresh with.
    records: FileTokenStore,
    /// How a sign-in's requests travel: the network in a run, a replay in a test.
    transport: DynHttpClient,
}

impl Store {
    /// A store over the files in `dir`, whose requests travel through
    /// `transport`: the network, or a replay a test substitutes so the flow is
    /// proved with no socket.
    pub fn over(transport: impl HttpClientExt + Clone + 'static, dir: impl Into<PathBuf>) -> Store {
        Store {
            records: FileTokenStore::new(dir),
            transport: DynHttpClient::new(transport),
        }
    }

    /// The store the environment points at, sending over the network.
    pub fn resolve(environment: &impl Environment) -> Result<Store, ConfigError> {
        Ok(Store::over(
            rig_reqwest::ReqwestClient::default(),
            store_dir(environment)?,
        ))
    }

    /// The one file the record is kept in, for a row to name and a test to stat.
    pub fn path(&self) -> PathBuf {
        self.records.path(PROVIDER_NAME)
    }

    /// A session to ask the wire with, refreshed when the one on file has
    /// expired: a run never sees an expiry, and never starts a sign-in of its own
    /// because a run has no one to show a code to.
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

    /// Sign in with the device flow, showing the code through `show` as GitHub
    /// hands it over, and keep the session it ends with. Blocks until GitHub
    /// answers, which is what the wait between polls is.
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

    /// Every literal the record holds that no window may print: the session and
    /// the GitHub token behind it, read out of the record as it lies on disk
    /// rather than from any field this crate happens to know the meaning of.
    pub fn secrets(&self) -> SecretSet {
        secrets_of(&self.record())
    }

    /// rig's authenticator over this store's file and transport, showing a code
    /// through `code` when the device flow is allowed.
    fn authenticator(&self, code: DeviceCodeHandler, device_flow: bool) -> Authenticator {
        Authenticator::new(AuthSource::OAuth, self.records.clone(), code, device_flow)
    }

    /// The record as the store holds it, or `None` when nothing has signed in.
    /// Read here only to know which credentials to scrub, never to keep.
    fn record(&self) -> Option<serde_json::Value> {
        let text = futures::executor::block_on(self.records.load(PROVIDER_NAME))
            .ok()
            .flatten()?;
        serde_json::from_str(&text).ok()
    }
}

impl fmt::Debug for Store {
    /// The file, and never a byte of what sits in it.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Store")
            .field("path", &self.path())
            .finish_non_exhaustive()
    }
}

/// The directory a Copilot sign-in is kept in: `$XDG_CONFIG_HOME/sindri/auth`,
/// beside §11's `config.toml` and a directory of its own, so the file that holds
/// a key is never the file that holds a sign-in.
pub fn store_dir(environment: &impl Environment) -> Result<PathBuf, ConfigError> {
    Ok(config::config_path(environment)?.with_file_name("auth"))
}

/// The row a sign-in ends with: the file the session was written to, the mode of
/// that file, and that it is never shown again — a value rather than a line
/// printed from here, so a front end paints the same bytes §11's API-key row is
/// painted with and a test asserts on them.
pub fn store_row(path: &Path) -> String {
    format!(
        "copilot session   written to {}, mode {:04o}\n                  \
         and is never shown again",
        path.display(),
        MODE
    )
}

/// The credentials `record` holds, as the set a `Redacted` scrubs through.
fn secrets_of(record: &Option<serde_json::Value>) -> SecretSet {
    let mut secrets = SecretSet::default();
    for field in [SESSION_FIELD, GITHUB_FIELD] {
        if let Some(secret) = field_of(record, field) {
            secrets.add(secret);
        }
    }
    secrets
}

/// Whether `record` holds a GitHub token: a store with one has signed in before,
/// so a resolve that fails afterwards failed on a sign-in the wire will not take.
fn holds_github_token(record: &Option<serde_json::Value>) -> bool {
    field_of(record, GITHUB_FIELD).is_some()
}

/// One string field of `record`, when the record holds it and it is not empty.
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

/// Why a resolve failed, from the error and from what the store held before it.
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

/// Whether `error` is a wire refusing the credential rather than a wire failing.
fn refused(error: &HttpError) -> bool {
    error
        .non_success_status()
        .is_some_and(|status| REFUSED.contains(&status.as_u16()))
}
