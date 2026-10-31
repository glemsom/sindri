//! The boundary a record settles on at its start and moves when a writable set
//! moves: the policy the model was handed, the tree it ran against, and the
//! dials it obeyed. Proven through `Session`, so no process is involved.

#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a session that cannot start reports itself"
)]

mod support;

use sindri_core::record::{Entry, Stored};
use sindri_core::session::Session;
use support::{commit, config, host};

/// The three questions a post-mortem opens with — could it write there, which
/// commit was that, why did it differ — answered before the first prompt.
#[test]
fn an_open_session_records_the_boundary_the_tree_and_the_dials() {
    let (_root, env) = host();
    let head = commit(&env.workspace);
    let session = Session::open(&env, &config(&[])).expect("a session opens");

    let stored = Stored::load_in(session.dir()).expect("the record reads back");
    assert_eq!(
        stored.meta.policy.as_deref(),
        Some(session.policy().describe().as_str()),
        "the record holds the description the model was handed, verbatim"
    );
    assert_eq!(
        stored.meta.workspace,
        env.workspace.display().to_string(),
        "and the tree it ran against"
    );
    let git = stored.meta.git.expect("a run in a repository records it");
    assert_eq!(git.branch, "main");
    assert_eq!(git.commit, head);
    assert!(!git.dirty, "a tree nothing touched is not dirty");
    assert_eq!(stored.meta.max_turns, 8, "the cap the run obeyed");
    assert!(!stored.meta.thinking, "and the thinking dial it obeyed");
}

#[test]
fn a_run_in_a_tree_with_changes_records_it_dirty() {
    let (_root, env) = host();
    commit(&env.workspace);
    std::fs::write(env.workspace.join("uncommitted"), "wip\n").expect("an untracked file");
    let session = Session::open(&env, &config(&[])).expect("a session opens");

    let stored = Stored::load_in(session.dir()).expect("the record reads back");
    let git = stored.meta.git.expect("the tree is a repository");
    assert!(git.dirty, "the status was read, so it is said: {git:?}");
}

/// A directory that is not a repository records no git block — not an empty
/// one — and the record still reads back.
#[test]
fn a_run_outside_a_repository_records_no_git_block_and_reads_back() {
    let (_root, env) = host();
    let session = Session::open(&env, &config(&[])).expect("a session opens");

    let stored = Stored::load_in(session.dir()).expect("the record reads back");
    assert!(
        stored.meta.git.is_none(),
        "silence, never a branch named for a repository that is not there"
    );
    assert_eq!(
        stored.meta.workspace,
        env.workspace.display().to_string(),
        "the path is still the tree it ran in"
    );
}

/// A writable set that moves under a thread says when it moved, instead of
/// claiming the first policy for all of it.
#[test]
fn a_rebase_that_moves_the_boundary_appends_the_policy_it_moved_to() {
    let (root, env) = host();
    let extra = root.path().join("scratch");
    std::fs::create_dir_all(&extra).expect("extra path");
    let extra = extra.canonicalize().expect("canonical extra");
    let mut session = Session::open(&env, &config(&[])).expect("a session opens");
    let first = session.policy().describe();

    session
        .rebase(&config(&[&extra]))
        .expect("the same session takes the new settings");

    let stored = Stored::load_in(session.dir()).expect("the record reads back");
    assert_eq!(
        stored.meta.policy.as_deref(),
        Some(session.policy().describe().as_str()),
        "the metadata settles on the boundary the run ended with"
    );
    let changes: Vec<&str> = stored
        .entries
        .iter()
        .filter_map(|entry| match entry {
            Entry::Policy { description } => Some(description.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(changes.len(), 1, "one move, one entry: {changes:?}");
    assert_ne!(changes[0], first, "and it is the boundary it moved to");
    assert!(
        changes[0].contains(&extra.display().to_string()),
        "which names the path that was added: {}",
        changes[0]
    );
}

#[test]
fn a_rebase_that_moves_nothing_appends_nothing() {
    let (_root, env) = host();
    let mut session = Session::open(&env, &config(&[])).expect("a session opens");

    session.rebase(&config(&[])).expect("the same settings");
    session.rebase(&config(&[])).expect("and again");

    let stored = Stored::load_in(session.dir()).expect("the record reads back");
    assert!(
        !stored
            .entries
            .iter()
            .any(|entry| matches!(entry, Entry::Policy { .. })),
        "a boundary that did not move is not announced: {:?}",
        stored.entries
    );
}
