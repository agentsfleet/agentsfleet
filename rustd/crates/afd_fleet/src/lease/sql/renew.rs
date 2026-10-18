//! The renewal's scoped read. The renewal itself is
//! [`super::meter::METER_LEASE`], which extends both deadline rows and meters
//! the slice between them.
//!
//! # Why BOTH rows move, in one statement
//!
//! Reclaimability and the kill deadline live on different rows.
//! `runner.fleet_slots.lease_expires_at` is what the claim checks, and
//! `runner.leases.lease_expires_at` is when the runner stops its child.
//! Advancing one without the other gets a healthy run reclaimed at the TTL
//! while its runner still believes it holds the lease — so both move to the
//! same clamped instant under the same fence, or neither does.
//!
//! The probe locks both rows (`FOR UPDATE OF l, a`) before either moves, so
//! no reclaim can take the slot between them: the slot update matches
//! whenever the lease update does.

/// The lease a renewal is about, scoped to the runner presenting it.
///
/// `status` IS selected here, unlike [`super::report::SELECT_LEASE_FOR_REPORT`],
/// and the asymmetry is deliberate. A report has one refusal for a lease that
/// is not `active` — the fence — so reading the status early would only add a
/// second way to say it. A renewal has two, and they are different answers to
/// the runner: a lease that is no longer active is LOST and the child dies,
/// where a lease that is active but out of credit is a refusal it can report
/// against. Telling them apart needs the status before the money gates run.
///
/// `$1` lease, `$2` runner.
pub const SELECT_LEASE_FOR_RENEW: &str = include_str!("select_lease_for_renew.sql");
