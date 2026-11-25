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
/// rather than the statement: its caller derives the workspace and the fleet
/// from ONE trusted row — the fenced run it is serving — so there is no
/// untrusted pair to reject. No endpoint takes a caller-supplied `(workspace,
/// fleet)` pair into that statement; the precondition on
/// `IntegrationGrants::request` is what keeps that true, and another caller
/// that cannot honour it must run this check itself before asking.
pub(crate) const SELECT_FLEET_IN_WORKSPACE: &str = include_str!("select_fleet_in_workspace.sql");

/// Every grant a fleet holds, newest first, its scopes as JSON text.
///
/// Unpaged and unfiltered: a fleet holds at most one grant per name — the
/// unique constraint on `(fleet_id, service)` says so — and the workspace's
/// secrets and integrations bound the page. `requested_reason` is the wire's
/// `reason`.
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
/// grant. `$1` grant, `$2` fleet, `$3` name, `$4` grant type, `$5` status, `$6` reason.
pub(crate) const ENSURE_GRANT: &str = include_str!("ensure_grant.sql");

/// Asks a person for the grant `ENSURE_GRANT` left in place, raising at most
/// one card. Locking the grant serializes requests with its approval and its
/// revocation; the unique active reference arbitrates cards. The card carries
/// the asking turn, whose run waits on it, and lapses `$15` after the database
/// clock.
///
/// `$1` fleet, `$2` name, `$3` grant type, `$4` requested scope, `$5` pending,
/// `$6` revoked, `$7` card, `$8` workspace, `$9` action, `$10` asking turn,
/// `$11` card type, `$12` proposal, `$13` evidence, `$14` radius, `$15`
/// decision window, `$16` card status.
pub(crate) const REQUEST_GRANT: &str = include_str!("request_grant.sql");
