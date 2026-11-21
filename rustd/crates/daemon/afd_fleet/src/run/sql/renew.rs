//! The renewal's scoped read. The renewal itself is
//! [`super::meter::METER_RUN`], which extends both deadline rows and meters
//! the slice between them.
//!
//! # Why BOTH rows move, in one statement
//!
//! Reassignability and the kill deadline live on different rows.
//! `thread.threads.run_expires_at` is what the assignment checks, and
//! `thread.runs.run_expires_at` is when the runner stops its child.
//! Advancing one without the other gets a healthy run reassigned at the TTL
//! while its runner still believes it holds the run — so both move to the
//! same clamped instant under the same fence, or neither does.
//!
//! The probe locks both rows (`FOR UPDATE OF l, a`) before either moves, so
//! no reassignment can take the thread between them: the thread update matches
//! whenever the run update does.

/// The run a renewal is about, scoped to the runner presenting it.
///
/// `status` IS selected here, unlike [`super::report::SELECT_RUN_FOR_REPORT`],
/// and the asymmetry is deliberate. A report has one refusal for a run that
/// is not `active` — the fence — so reading the status early would only add a
/// second way to say it. A renewal has two, and they are different answers to
/// the runner: a run that is no longer active is LOST and the child dies,
/// where a run that is active but out of credit is a refusal it can report
/// against. Telling them apart needs the status before the money gates run.
///
/// `$1` run, `$2` runner.
pub const SELECT_RUN_FOR_RENEW: &str = include_str!("select_run_for_renew.sql");

/// A host's runs before its renewal statement: pricing facts, payer, task,
/// cancel mark, start, head revision and catalogue version.
pub const SELECT_RENEWALS: &str = include_str!("select_renewals.sql");

/// Every renewal one host's frame names, in one statement.
pub const RENEW_BATCH: &str = include_str!("renew_batch.sql");

/// Setup documents for the revisions the definitions cache missed.
pub const SELECT_REVISION_DOCUMENTS: &str = include_str!("select_revision_documents.sql");
