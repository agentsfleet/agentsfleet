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

/// The fleets in a workspace a chat mention could reach, for
/// [`crate::slack::subscribed`] to read.
///
/// The relational half only, as [`SELECT_APP_SUBSCRIBERS`] is: whether a
/// fleet's `mention` trigger names the channel is a question about its
/// document, answered where a test can reach it. No grant join — a fleet
/// answering a mention mints no chat credential; the daemon posts for it.
///
/// Ordered by id so every replica routes the same set the same way.
///
/// `$1` workspace, `$2` the statuses a subscriber can be read in.
pub const SELECT_MENTION_CANDIDATES: &str = "\
SELECT f.id::text, f.status, f.config_json::text
FROM core.fleets f
WHERE f.workspace_id = $1::uuid
  AND f.status = ANY($2::text[])
ORDER BY f.id";

/// The fleet bound as a chat channel's resident in one workspace, and its
/// status, when one is.
///
/// Scoped to the workspace the mention resolved to. A Slack team can move to
/// another workspace (`InstallClaim::Repoint`) and its binding row does not
/// move with it; read by channel alone, a mention in the new workspace would
/// run the old workspace's fleet on that workspace's budget and memory.
///
/// `$1` provider, `$2` the provider's account (a Slack team), `$3` the channel,
/// `$4` the binding kind, `$5` the workspace.
pub const SELECT_RESIDENT: &str = "\
SELECT c.fleet_id::text, f.status
FROM core.connector_channels c
JOIN core.fleets f ON f.id = c.fleet_id
WHERE c.provider = $1 AND c.external_account_id = $2 AND c.external_channel_id = $3
  AND c.kind = $4 AND f.workspace_id = $5::uuid";

/// Binds a channel's resident once per workspace, answering whichever fleet
/// is bound there.
///
/// Insert-once under the channel's unique constraint: a concurrent first mention
/// that lost the race writes nothing, and the `UNION ALL` reads back the row
/// the winner wrote, so both callers answer the same fleet. A row whose fleet
/// sits in another workspace is a team that moved (see [`SELECT_RESIDENT`]),
/// and is re-pointed rather than kept. The read-back is scoped the same way, so
/// this never answers another workspace's fleet.
///
/// `$1` binding id, `$2` provider, `$3` account, `$4` channel, `$5` fleet,
/// `$6` kind, `$7` created at, `$8` the workspace.
pub const INSERT_RESIDENT: &str = "\
WITH bound AS (
  INSERT INTO core.connector_channels AS c
    (id, provider, external_account_id, external_channel_id, fleet_id, kind, created_at)
  VALUES ($1::uuid, $2, $3, $4, $5::uuid, $6, $7)
  ON CONFLICT (provider, external_account_id, external_channel_id) DO UPDATE
    SET fleet_id = EXCLUDED.fleet_id, kind = EXCLUDED.kind, created_at = EXCLUDED.created_at
    WHERE NOT EXISTS (
      SELECT 1 FROM core.fleets f WHERE f.id = c.fleet_id AND f.workspace_id = $8::uuid
    )
  RETURNING fleet_id
)
SELECT fleet_id::text FROM bound
UNION ALL
SELECT c.fleet_id::text
FROM core.connector_channels c
JOIN core.fleets f ON f.id = c.fleet_id AND f.workspace_id = $8::uuid
WHERE c.provider = $2 AND c.external_account_id = $3 AND c.external_channel_id = $4
LIMIT 1";

/// The fleet a workspace holds under a resident's name, and its status.
///
/// The third column says whether it IS that resident: its stored configuration
/// equals the one the daemon writes, compared as JSON so formatting cannot
/// make a difference.
///
/// `$1` workspace, `$2` name, `$3` the resident's configuration.
pub const SELECT_RESIDENT_NAMED: &str = "\
SELECT id::text, status, config_json = $3::jsonb
FROM core.fleets WHERE workspace_id = $1::uuid AND name = $2";
