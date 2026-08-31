//! `core.fleets`, read the one way an unauthenticated delivery may read it.
//!
//! # What the Zig asked Postgres, and why this asks less
//!
//! `serve_webhook_lookup.zig` runs two statements over the same row, each
//! walking `config_json` with `jsonb_array_elements` to pull one field out of
//! the FIRST webhook trigger — `source` and `credential_name` in one, the
//! whole `signature` object in the other. That is a document reader written in
//! SQL, and it exists because the Zig had no typed reader for a stored fleet
//! document at that layer.
//!
//! Rust has one. [`afd_fleet_runtime::FleetConfig::stored`] already parses this
//! exact column on the claim path, applies the schema bounds, and completes a
//! trigger's signature block from the provider registry — so the statement here
//! selects the column and stops. **Verdict: replaced** (M183). What that buys
//! beyond one round trip instead of two: the `LIMIT 1` "first webhook trigger"
//! rule stops being a property of a sub-select nobody can test in isolation,
//! the two statements can no longer disagree about WHICH trigger they read, and
//! a document that fails its bounds is refused here rather than half-read.

/// Everything an ingress needs to decide about one fleet.
///
/// `$1` fleet. Three columns: the owning workspace, the status, and the stored
/// document. No filter beyond the identifier, and that is correct rather than
/// lax — the caller is a provider holding no principal, so there is no scope to
/// narrow by. What authorizes the delivery is the signature checked against the
/// secret this row leads to, and never the row's own visibility.
///
/// `status` is read even though nothing in the trigger set mentions it: a
/// delivery to a paused fleet is answered 200 and dropped rather than run, and
/// a query that did not return the status would have no way to know.
pub const SELECT_FLEET_INGRESS: &str = "\
SELECT workspace_id::text, status, config_json::text
FROM core.fleets
WHERE id = $1::uuid";

/// The workspace an App installation was connected to.
///
/// `$1` provider, `$2` the provider's own account identifier — GitHub's
/// `installation.id`, Slack's `team_id`. `core.connector_installs` carries
/// `UNIQUE (provider, external_account_id)`, so this reads at most one row and
/// the `LIMIT` the Zig writes is the constraint restated rather than a rule.
///
/// This is the whole reason the table exists: a signed App delivery arrives
/// addressed only by the PROVIDER's identifier and carries no workspace, no
/// fleet and no principal. Without this index there is nothing to route it by.
/// **Verdict: left** (M183) — a two-column lookup on a unique index is what a
/// database is for, and there is no document in it to read.
pub const SELECT_INSTALL_WORKSPACE: &str = "\
SELECT workspace_id::text
FROM core.connector_installs
WHERE provider = $1 AND external_account_id = $2";

/// Every fleet in a workspace that could take this provider's App delivery.
///
/// `$1` workspace, `$2` the status a fleet must hold, `$3` the granted service,
/// `$4` the grant status. Four columns per row, which is exactly
/// [`Binding::read_for_source`]'s input.
///
/// # What this asks, and what it deliberately does not
///
/// `SELECT_APP_INGRESS_TARGETS` walks `config_json` in SQL to match the
/// repository and the event, with `jsonb_array_elements` inside two nested
/// `EXISTS` clauses. That is the same document-reader-written-in-SQL
/// [`SELECT_FLEET_INGRESS`] already replaced, one query over. **Verdict:
/// replaced** (M183): this statement asks only the RELATIONAL half — which
/// fleets are in the workspace, running, and hold an approved grant — and the
/// document half is answered by [`Binding::serves_repository`] and
/// [`Binding::admits`], where a test can reach both.
///
/// What that costs and why it is worth paying: the candidate rows crossing the
/// wire are no longer pre-filtered by repository, so a workspace pays for its
/// active granted fleets rather than its subscribed ones. The set is bounded by
/// the same ceiling the fan-out is — a workspace with more matching fleets than
/// [`crate::MAX_FANOUT`] has a delivery this daemon refuses either way — and in
/// exchange the subscription rule becomes three lines with tests instead of a
/// sub-select that can only be exercised through a live Postgres.
///
/// Ordered by id so a truncated fan-out is the same set every replica sees.
pub const SELECT_APP_SUBSCRIBERS: &str = "\
SELECT f.id::text, f.workspace_id::text, f.status, f.config_json::text
FROM core.fleets f
JOIN core.integration_grants g ON g.fleet_id = f.id
WHERE f.workspace_id = $1::uuid
  AND f.status = $2
  AND g.service = $3
  AND g.status = $4
ORDER BY f.id";
