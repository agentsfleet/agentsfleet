//! `runner.fleet_slots` and `runner.leases` — the claim, the fence,
//! and the row that records who owns a fleet's work.
//!
//! # The claim is the whole concurrency design
//!
//! [`CLAIM_FLEET_SLOT`] is one conditional UPSERT and it carries three jobs
//! at once: it wins the fleet iff the slot is free or its prior claim expired,
//! it bumps the monotonic `fencing_token`, and it records the sticky-routing
//! hint. Exactly one of N racing runners takes the row. Crucially the claim
//! PRECEDES the event read, so a loser has consumed no event and nothing is
//! orphaned — which is why `test_lease_affinity_race` asserts one lease row and
//! one no-work reply rather than counting retries.

mod row;

pub use self::row::{INSERT_LEASE_WITH_EVENT, LeaseRow};

/// Claim a fleet's lease slot, bumping the monotonic fence.
///
/// `fleet_id` is the whole primary key (schema/630), so the conflict target IS
/// the table's only unique index — two runners racing a brand-new fleet's slot
/// take the update arm rather than colliding on an index this statement does
/// not name.
///
/// Answers no row when a live runner still holds the slot — that absence is the
/// `.taken` verdict, not an error.
///
/// # A held fleet is its holder's first
///
/// While `hold_expires_at` is in the future and its holder, `last_runner_id`, can
/// still lease — `$5` (active) and not degraded, and beaten within `$4`
/// (`RUNNER_OFFLINE_AFTER`) — only the holder wins: its sandbox carries the
/// fleet's last run, and the next event should run there. A holder that
/// cannot lease binds nobody: a degraded runner is answered no work, and one
/// drained or revoked is refused before its poll or its beat reaches here.
/// A hold is a head start at the slot, never the slot. The holder still claims
/// through this statement and its fence, so one fleet still has one live
/// holder. The holder's own claim keeps `hold_expires_at`, so a claim that finds no
/// event leaves the hold in place for the next one; its report then records
/// the hold anew or clears it. Any other runner's claim clears it, since the
/// sandbox it named no longer carries the fleet's latest run.
///
/// `prior` reads the slot before the statement changes it, so the caller can
/// tell a claim on a held fleet from one on an unheld fleet. A CTE sees the
/// statement's starting snapshot on every Postgres version.
///
/// `$1` fleet, `$2` runner, `$3` the lease lifetime, `$4` how long a silent
/// runner stays live, `$5` the runner status that may lease. One
/// `clock_timestamp()` reading decides every deadline: an upsert cannot lock
/// before it reads, so the instant is taken first, which only ever makes a
/// lock wait refuse a claim or shorten a lease, never extend one.
pub const CLAIM_FLEET_SLOT: &str = include_str!("claim_fleet_slot.sql");

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

/// [`RELEASE_FLEET_SLOT`] for a claim that leased nothing. `$1` fleet, `$2` fence.
///
/// It drops the sticky hint too, or a fleet whose pass keeps stopping sorts
/// first for that runner on every poll of its partition, starving the rest.
/// A fleet still held keeps its hint: the hint names the holder, and the hold
/// is what steers the fleet's next event there. Every fleet nobody holds is
/// unhinted as before, so the starvation guard still covers it.
pub const RELEASE_UNLEASED_SLOT: &str = include_str!("release_unleased_slot.sql");

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
