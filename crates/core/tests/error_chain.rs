//! The chain a core failure answers: an error that holds another hands it back
//! from `source()`, so a caller that would act on a failure reads the cause
//! rather than the sentence a human is served it in.

use std::error::Error as _;
use std::io::{self, ErrorKind};
use std::path::PathBuf;

use sindri_core::doctor::DoctorError;
use sindri_core::record::ShowError;
use sindri_core::sandbox::PolicyError;

/// The kind of the `io::Error` in the chain, found by walking it the way a
/// caller that acts on a failure does rather than by knowing its shape: a link
/// that will not hand its cause back stops the walk with nothing.
fn io_kind(error: &(dyn std::error::Error + 'static)) -> Option<ErrorKind> {
    let mut link = Some(error);
    while let Some(current) = link {
        if let Some(io) = current.downcast_ref::<io::Error>() {
            return Some(io.kind());
        }
        link = current.source();
    }
    None
}

#[test]
fn a_writable_path_that_cannot_be_read_hands_its_io_error_to_the_chain() {
    let cause = io::Error::from(ErrorKind::PermissionDenied);
    let error = PolicyError::Unreadable {
        path: PathBuf::from("/state/workspace"),
        source: cause,
    };

    assert_eq!(
        io_kind(&error),
        Some(ErrorKind::PermissionDenied),
        "a reader that acts on the failure finds the reason, not only the record"
    );
}

#[test]
fn a_record_that_cannot_be_read_hands_its_io_error_to_the_chain() {
    let cause = io::Error::from(ErrorKind::NotFound);
    let error = ShowError::Unreadable {
        path: PathBuf::from("/state/sessions/4f1c8f2e/transcript.jsonl"),
        source: cause,
    };

    assert_eq!(
        io_kind(&error),
        Some(ErrorKind::NotFound),
        "the walk reaches the file that was not there"
    );
    let said = error.to_string();
    assert!(
        said.starts_with("/state/sessions/4f1c8f2e/transcript.jsonl cannot be read: "),
        "the sentence a human reads is the one it always was: {said}"
    );
    assert_eq!(
        said.matches(&io::Error::from(ErrorKind::NotFound).to_string())
            .count(),
        1,
        "and the cause is said once: {said}"
    );
}

#[test]
fn an_environment_probe_that_fails_hands_back_the_failure_that_stopped_it() {
    let setup = DoctorError::Setup(io::Error::from(ErrorKind::OutOfMemory));
    assert_eq!(
        io_kind(&setup),
        Some(ErrorKind::OutOfMemory),
        "the probe's own failure is the first link"
    );

    let policy = DoctorError::Policy(PolicyError::Unreadable {
        path: PathBuf::from("/state/workspace"),
        source: io::Error::from(ErrorKind::PermissionDenied),
    });
    let first = policy.source().expect("the policy is the link below");
    assert!(
        first.downcast_ref::<PolicyError>().is_some(),
        "a policy is handed back as the policy it is, not flattened into words"
    );
    assert_eq!(
        io_kind(&policy),
        Some(ErrorKind::PermissionDenied),
        "and the walk reaches the io::Error two links down"
    );
}
