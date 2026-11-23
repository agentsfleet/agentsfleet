//! The fleet kind's own run statements on `thread.threads` and
//! `thread.runs`: the assignment, the fence, and the row that records who owns a
//! fleet's work. The thread verbs every kind runs are [`super::shared`].
//!
//! # The assignment is the whole concurrency design
//!
//! The thread upsert inside `assign_turn_head.sql` is one conditional UPSERT
//! and it carries three jobs at once: it wins the fleet iff the thread is free or
//! its prior assignment expired,
//! it bumps the monotonic `fencing_token`, and it records the sticky-routing
//! hint. Exactly one of N racing runners takes the row. Crucially the assignment
//! PRECEDES the event read, so a loser has consumed no event and nothing is
//! orphaned — which is why `test_run_affinity_race` asserts one run row and
//! one no-work reply rather than counting retries.
//!
//! A fleet's thread is keyed by the fleet's id, so the fleet's statements here
//! read `thread.threads.fleet_id` and `thread.runs.fleet_id`, which a draft's
//! rows leave NULL and so never match.

mod row;

pub use self::row::{INSERT_RUN_WITH_TURN, RunRow};

/// Assigns to a live run the messages its fleet received after the
/// run's own, oldest first.
///
/// It stops at the first parked or task-bound message. `$1` run, `$2`
/// runner, `$3` fence, `$4` active status, `$5` read-after sequence, `$6`
/// limit.
///
/// One statement: it proves the run and its fence itself, so an assignment before
/// every model call costs one round trip. Idempotent for its run, so an
/// assignment whose answer was lost reads the same messages again.
pub const ASSIGN_INPUTS: &str = include_str!("assign_inputs.sql");

/// Settles the messages a run assigned and a completed model call read.
///
/// It runs in a succeeded report's transaction: `$1` fleet, `$2` run, `$3`
/// fence, `$4` read-through sequence. Answers each one, so its own event and
/// reply can be written; an assigned message nobody read stays for the next run.
pub const SETTLE_FOLDS: &str = include_str!("settle_folds.sql");

/// What one run authorises a mint to reach.
///
/// Every clause in the `WHERE` is an authorisation, and the reason they are all
/// in the statement is that the wire carries no workspace at all: a
/// prompt-injected child has nothing to forge,
/// because a `run_id` that is foreign, expired or cancelled resolves to no
/// row rather than to another tenant's workspace.
///
/// - `l.runner_id = $2` — Invariant 2. The runner-id scope IS the ownership
///   check; the presenting bearer decides which runs exist.
/// - `l.status = $3 AND l.run_expires_at > $4` — mint authority is bound to
///   the RUN's lifetime, not the runner's, so a compromised runner replaying
///   a stale run id cannot mint past the run it was issued for.
/// - `l.fencing_token >= COALESCE(a.fencing_token, l.fencing_token)` — and to
///   the run still HOLDING the fleet. A run row stays `active` after a
///   reassignment moves the fleet past it, until the reassignment's issue expires it; the
///   affinity row's sequence is the live authority, as the fence reads in
///   `crate::run::fence` treat it.
///
/// The fleet is joined rather than read second because the binding must come
/// from the fleet the run authorised. Two statements could return a binding
/// belonging to a fleet the run does not name.
///
/// `$1` run, `$2` runner, `$3` active status; the deadline is read on
/// `statement_timestamp()`.
pub const SELECT_RUN_SCOPE_FOR_MINT: &str = include_str!("select_run_scope_for_mint.sql");

/// Whether a workspace connected the external account a mint is about to
/// spend — the routing row the connect flow wrote beside the handle.
///
/// Keyed on all three columns, so another workspace's row for the same
/// installation answers `false` here exactly as no row does, and the refusal
/// cannot tell a caller which workspace holds it. The unique constraint on
/// `(provider, external_account_id)` makes the answer one row or none.
///
/// A reader only: the rows are written and released by the connect flow
/// (`afd_connector::sql::CLAIM_INSTALL` and its siblings), whose GitHub claim is
/// exclusive per installation.
///
/// `$1` provider, `$2` external account, `$3` workspace.
pub const SELECT_CONNECTOR_INSTALL_HELD: &str = include_str!("select_connector_install_held.sql");
