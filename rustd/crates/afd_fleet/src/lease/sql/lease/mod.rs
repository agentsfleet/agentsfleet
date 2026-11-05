//! `runner.fleet_slots` and `runner.leases` — the fence's releases and the
//! row that records who owns a fleet's work.
//!
//! The claim itself is [`super::queue::CLAIM_HEAD`]: one statement that takes
//! the oldest ready slot with `SKIP LOCKED`, bumps the monotonic
//! `fencing_token`, records the sticky-routing hint and opens the event. Exactly
//! one of N racing runners takes a slot, and a loser writes nothing.

mod row;

pub use self::row::{INSERT_LEASE_WITH_EVENT, LeaseRow};

/// Release the slot — fencing-guarded, so only the current holder can free it —
/// and record whether that holder keeps the fleet's sandbox.
///
/// `$1` fleet, `$2` fence, `$3` when the runner asked its hold to lapse (null
/// for none), `$4` the longest hold. The hold written is the ask capped at one
/// `$4` window from `clock_timestamp()`, and none when the ask is already past,
/// so a runner cannot steer a fleet to itself for longer than a hold lasts.
///
/// The freed deadline is written one millisecond behind the clock: a
/// `TIMESTAMPTZ(3)` column rounds, and a deadline rounded up past the instant a
/// claim reads a moment later would leave a released slot looking held.
///
/// The guard is load-bearing rather than defensive: a holder superseded by a
/// reclaim would otherwise free the CURRENT holder's slot and hand one fleet to
/// two runners, and would record a hold on a sandbox no lease will reach.
/// Idempotent — a no-op when the row is gone or the token has been bumped past
/// this one.
pub const RELEASE_FLEET_SLOT: &str = include_str!("release_fleet_slot.sql");

/// [`RELEASE_FLEET_SLOT`] for a claim that leased nothing.
///
/// `$1` fleet, `$2` fence, `$3` how long the fleet stays out of claims: zero
/// for an ending that frees it for another runner, the retry backoff for one
/// the next poll would likely meet again.
///
/// It drops the sticky hint too, or a fleet whose pass keeps stopping sorts
/// first for that runner on every poll of its partition, starving the rest.
/// A fleet still held keeps its hint: the hint names the holder, and the hold
/// is what steers the fleet's next event there. Every fleet nobody holds is
/// unhinted as before, so the starvation guard still covers it.
pub const RELEASE_UNLEASED_SLOT: &str = include_str!("release_unleased_slot.sql");

/// Name `$2` as `$1`'s last runner, creating the slot with no lease when the
/// fleet has none, so that runner's next claim takes this fleet first.
///
/// Test fixtures only: a suite sharing the lane database steers its own
/// runner to its own fleet, as a finished run there would.
#[cfg(feature = "test-util")]
pub const PREFER_RUNNER: &str = include_str!("prefer_runner.sql");

/// What one lease authorises a mint to reach.
///
/// Every clause in the `WHERE` is an authorisation, and the reason they are all
/// in the statement is that the wire carries no workspace at all: a
/// prompt-injected child has nothing to forge,
/// because a `lease_id` that is foreign, expired or cancelled resolves to no
/// row rather than to another tenant's workspace.
///
/// - `l.runner_id = $2` — Invariant 2. The runner-id scope IS the ownership
///   check; the presenting bearer decides which leases exist.
/// - `l.status = $3 AND l.lease_expires_at > $4` — mint authority is bound to
///   the LEASE's lifetime, not the runner's, so a compromised runner replaying
///   a stale lease id cannot mint past the run it was issued for.
/// - `l.fencing_token >= COALESCE(a.fencing_token, l.fencing_token)` — and to
///   the lease still HOLDING the fleet. A lease row stays `active` after a
///   reclaim moves the fleet past it, until the reclaim's issue expires it; the
///   affinity row's sequence is the live authority, as the fence reads in
///   `crate::lease::fence` treat it.
///
/// The fleet is joined rather than read second because the binding must come
/// from the fleet the lease authorised. Two statements could return a binding
/// belonging to a fleet the lease does not name.
///
/// `$1` lease, `$2` runner, `$3` active status; the deadline is read on
/// `statement_timestamp()`.
pub const SELECT_LEASE_SCOPE_FOR_MINT: &str = include_str!("select_lease_scope_for_mint.sql");

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
