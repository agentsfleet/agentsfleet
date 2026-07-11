//! GitHub connector persistence statements. Kept outside the callback handler
//! so the routing-index writes and App-ingress reads have one schema-qualified
//! GitHub owner.

/// Remove the prior GitHub reverse-routing row for a workspace before a
/// reconnect stores its current installation.
pub const DELETE_WORKSPACE_INSTALLS =
    \\DELETE FROM core.connector_installs
    \\WHERE provider = $1 AND workspace_id = $2::uuid
;

/// Store or refresh the installation-to-workspace reverse-routing row. A
/// conflict owned by another workspace returns no row and the transaction
/// rolls back; installation transfer requires a separately verified flow.
pub const UPSERT_INSTALL =
    \\INSERT INTO core.connector_installs
    \\  (uid, provider, external_account_id, workspace_id, installed_by, scopes, created_at, updated_at)
    \\VALUES ($1::uuid, $2, $3, $4::uuid, $5, $6::text[], $7, $7)
    \\ON CONFLICT (provider, external_account_id) DO UPDATE SET
    \\  workspace_id = EXCLUDED.workspace_id,
    \\  installed_by = EXCLUDED.installed_by,
    \\  scopes = EXCLUDED.scopes,
    \\  updated_at = EXCLUDED.updated_at
    \\WHERE core.connector_installs.workspace_id = EXCLUDED.workspace_id
    \\RETURNING workspace_id::text
;

pub const SELECT_INSTALL =
    \\SELECT workspace_id::text, installed_by, cardinality(scopes)
    \\FROM core.connector_installs
    \\WHERE provider = $1 AND external_account_id = $2
;

/// Serialize final GitHub install persistence for one workspace. The callback
/// consumes the latest-state marker under this transaction-level advisory lock
/// before deleting/replacing the vault handle and reverse-routing row.
pub const LOCK_INSTALL_PERSISTENCE =
    \\SELECT pg_advisory_xact_lock(hashtext($1), hashtext($2))
;

/// Resolve a GitHub App installation id to its connected workspace.
pub const SELECT_WORKSPACE_BY_INSTALLATION =
    \\SELECT workspace_id::text FROM core.connector_installs
    \\WHERE provider = $1 AND external_account_id = $2
    \\LIMIT 1
;

/// Select active, granted fleets with an explicit GitHub repository and event
/// binding. The caller requests one row beyond its fan-out ceiling and rejects
/// the delivery before queue writes when that sentinel row exists.
pub const SELECT_APP_INGRESS_TARGETS =
    \\SELECT f.id::text, f.workspace_id::text
    \\FROM core.fleets f
    \\JOIN core.integration_grants g ON g.fleet_id = f.id
    \\WHERE f.workspace_id = $1::uuid
    \\  AND f.status = $2
    \\  AND g.service = $3
    \\  AND g.status = $4
    \\  AND EXISTS (
    \\    SELECT 1
    \\    FROM jsonb_array_elements(COALESCE(f.config_json->'x-agentsfleet'->'triggers', '[]'::jsonb)) AS trigger
    \\    WHERE trigger->>'type' = 'webhook'
    \\      AND trigger->>'source' = $3
    \\      AND EXISTS (
    \\        SELECT 1
    \\        FROM jsonb_array_elements_text(COALESCE(trigger->'repositories', '[]'::jsonb)) AS repo_name(value)
    \\        WHERE lower(repo_name.value) = lower($5)
    \\      )
    \\      AND (NOT (trigger ? 'events') OR trigger->'events' ? $6)
    \\  )
    \\ORDER BY f.id
    \\LIMIT $7
;

test "GitHub connector statements use the core schema" {
    const std = @import("std");
    try std.testing.expect(std.mem.indexOf(u8, DELETE_WORKSPACE_INSTALLS, "core.connector_installs") != null);
    try std.testing.expect(std.mem.indexOf(u8, UPSERT_INSTALL, "core.connector_installs") != null);
    try std.testing.expect(std.mem.indexOf(u8, LOCK_INSTALL_PERSISTENCE, "pg_advisory_xact_lock") != null);
    try std.testing.expect(std.mem.indexOf(u8, SELECT_WORKSPACE_BY_INSTALLATION, "core.connector_installs") != null);
    try std.testing.expect(std.mem.indexOf(u8, SELECT_APP_INGRESS_TARGETS, "core.fleets") != null);
    try std.testing.expect(std.mem.indexOf(u8, SELECT_APP_INGRESS_TARGETS, "core.integration_grants") != null);
}
