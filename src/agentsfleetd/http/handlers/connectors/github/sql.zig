//! GitHub connector persistence statements. Kept outside the callback handler
//! so the routing-index writes have one schema-qualified owner.

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

test "GitHub connector statements use the core schema" {
    const std = @import("std");
    try std.testing.expect(std.mem.indexOf(u8, DELETE_WORKSPACE_INSTALLS, "core.connector_installs") != null);
    try std.testing.expect(std.mem.indexOf(u8, UPSERT_INSTALL, "core.connector_installs") != null);
}
