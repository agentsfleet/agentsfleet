//! Per-user, per-workspace dashboard preference reads and writes, plus the one
//! round trip the onboarding checklist is derived from.
//!
//! Byte-identical to `state/user_preferences/sql.zig` and
//! `state/workspace_onboarding/sql.zig`, per this module's cutover rule.

/// The internal user a Clerk subject maps to.
///
/// The principal carries an opaque external subject; every preference row keys
/// on `core.users.id`. Two names for one person, and this is the join.
pub const SELECT_USER_ID_BY_SUBJECT: &str = "\
SELECT id::text
FROM core.users
WHERE oidc_subject = $1";

/// Every preference this user has set in this workspace, key-ordered.
///
/// Ordered so the bag is stable between reads: the response is a JSON object
/// and object key order is not load-bearing, but a stable order makes a diff
/// between two captures readable.
pub const SELECT_BAG: &str = "\
SELECT pref_key, pref_value
FROM core.user_preferences
WHERE user_id = $1::uuid AND workspace_id = $2::uuid
ORDER BY pref_key";

/// Writes one key, last-write-wins.
///
/// Arbitrates on `uq_user_preferences_user_id_workspace_id_pref_key`, which is
/// the constraint the schema declares for exactly this statement. A preference
/// is one scalar toggle, so a lost concurrent write costs one click rather than
/// authored content — which is why this is an upsert and not a transaction.
pub const UPSERT_PREF: &str = "\
INSERT INTO core.user_preferences
  (id, user_id, workspace_id, pref_key, pref_value, created_at, updated_at)
VALUES ($1::uuid, $2::uuid, $3::uuid, $4, $5, $6, $6)
ON CONFLICT (user_id, workspace_id, pref_key) DO UPDATE SET
  pref_value = EXCLUDED.pref_value,
  updated_at = EXCLUDED.updated_at";

/// Every derivable onboarding signal, in one round trip.
///
/// Five `EXISTS` subqueries rather than five requests: the planner stops at the
/// first matching row, so none of these scans a table. The workspace signals
/// key on the `workspace_id` index and the tenant-model check on
/// `tenant_model_selection`'s primary key.
///
/// `$2` is the steer-actor prefix, bound rather than inlined (RULE NSQ).
pub const SELECT_SIGNALS: &str = "\
SELECT
  EXISTS(SELECT 1 FROM core.fleets WHERE workspace_id = $1::uuid)                         AS has_fleet,
  EXISTS(SELECT 1 FROM vault.secrets WHERE workspace_id = $1::uuid)                       AS has_secret,
  EXISTS(SELECT 1 FROM core.fleet_events WHERE workspace_id = $1::uuid)                   AS has_event,
  EXISTS(SELECT 1 FROM core.fleet_events WHERE workspace_id = $1::uuid AND actor LIKE $2) AS has_steer,
  EXISTS(SELECT 1 FROM core.tenant_model_selection
         WHERE tenant_id = $3::uuid AND length(btrim(model)) > 0)                         AS tenant_model";

/// Whether an active platform default resolves to a non-empty model.
///
/// The second half of `model_configured`: a fresh tenant with no selection of
/// its own rides the platform default, so the checklist must not tell them to
/// configure a model they already have. Read as its own statement because the
/// row lives in a different table than the five signals above.
pub const SELECT_PLATFORM_DEFAULT_MODEL: &str = "\
SELECT EXISTS(
  SELECT 1 FROM core.platform_provider_defaults
  WHERE active AND length(btrim(model)) > 0
)";
