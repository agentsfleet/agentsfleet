//! Every statement the GRANT half of this crate runs, and nothing else.
//!
//! Split from [`super::sql`] for the reason `afd_gate` already splits its own
//! pair: `fleet.approval_gates` records a human answering about ONE event and
//! `connector.integration_grants` records a human answering about a fleet's
//! relationship with a third party, once, for every event after it. Two
//! authorities, two statement sets, so a reader looking for the grant half does
//! not read past the gate half to find it.

/// Whether `$1` is a fleet that `$2` holds.
///
/// A yes-or-no the predicate answers itself, so no workspace row comes back
/// for a caller to compare.
///
/// The two TENANT-FACING verbs run it FIRST, because both must tell "no such
/// fleet here" from their own absent row, and the two carry different codes. A
/// fleet in another workspace answers no rows — never a 403 — so the endpoint
/// cannot be an oracle for which fleet identifiers are real.
///
/// [`REQUEST_GRANT`] deliberately does NOT run it, and the reason is the caller
/// rather than the statement: both of its callers derive the workspace and the
/// fleet from ONE trusted row — the install from the fleet it just wrote into
/// the authenticated workspace, the park from the `Acquired` lease it is
/// serving — so there is no untrusted pair to reject, and the park path would
/// pay the round trip once per second to re-answer a question its own lease
/// row already settled. No endpoint takes a caller-supplied `(workspace,
/// fleet)` pair into that statement; the precondition on
/// `IntegrationGrants::request` is what keeps that true, and a third caller
/// that cannot honour it must run this check itself before asking.
pub(crate) const SELECT_FLEET_IN_WORKSPACE: &str = include_str!("select_fleet_in_workspace.sql");

/// Every grant a fleet holds, newest first.
///
/// Unpaged and unfiltered: a fleet holds at most one grant per service — the
/// unique constraint on `(fleet_id, service)` says so — and the
/// supported-service count is what bounds the page. `requested_reason` is the
/// wire's `reason`.
///
/// `$1` fleet.
pub(crate) const SELECT_FLEET_GRANTS: &str = include_str!("select_fleet_grants.sql");

/// Revokes one grant, scoped to the workspace that holds its fleet, stamped on
/// the database clock.
///
/// The join to `fleet.fleets` repeats what [`SELECT_FLEET_IN_WORKSPACE`] has
/// already decided, and that redundancy is load-bearing: if the fleet-scope
/// read is ever dropped from this crate, the statement still refuses a
/// cross-workspace revoke.
///
/// `g.status != $1` is what makes a second revoke report nothing rather than
/// re-stamping `revoked_at`, so the caller can tell "I revoked it" from
/// "it was already gone" without a read-then-write.
///
/// `$1` revoked status, `$2` grant, `$3` fleet, `$4` workspace.
pub(crate) const REVOKE_GRANT: &str = include_str!("revoke_grant.sql");

/// Ensures the grant exists before the card statement takes its snapshot.
/// Both statements run in one transaction: failure to raise rolls back the
/// grant. `$1` grant, `$2` fleet, `$3` service, `$4` status, `$5` reason.
pub(crate) const ENSURE_GRANT: &str = include_str!("ensure_grant.sql");

/// Raises a card using the existing grant's identity, including when a
/// concurrent insert won. Locking the grant serializes requests with its
/// approval/revocation; the unique active reference arbitrates cards without a
/// status-dependent index. A NULL `event_id` prevents approval from enqueueing
/// duplicate delivery work. The card lapses `$13` after the database clock.
///
/// `$1` fleet, `$2` service, `$3` grant status, `$4` card, `$5` workspace,
/// `$6` action, `$7` tool, `$8` action name, `$9` type, `$10` proposal,
/// `$11` evidence, `$12` radius, `$13` decision window, `$14` card status.
pub(crate) const REQUEST_GRANT: &str = include_str!("request_grant.sql");

/// The fleet row a setup request holds while it writes, and its status.
///
/// The Kill takes the same row exclusively, which is what serialises a late
/// setup retry against the cancel. `$1` fleet, `$2` workspace.
pub(crate) const LOCK_FLEET_FOR_SETUP: &str = include_str!("lock_fleet_for_setup.sql");

/// Closes a fleet's open grant cards, on the database clock.
///
/// `$1` fleet, `$2` closed status, `$3` resolver, `$4` detail, `$5` the grant
/// gate type.
pub(crate) const CANCEL_SETUP_CARDS: &str = include_str!("cancel_setup_cards.sql");
