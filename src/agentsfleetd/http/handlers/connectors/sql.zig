//! Connector-resource persistence shared by connect and Disconnect handlers.

/// Serialize writes for one provider binding in one workspace.
pub const LOCK_WORKSPACE_CONNECTOR =
    \\SELECT pg_advisory_xact_lock(hashtext($1), hashtext($2))
;

/// Remove reverse-routing rows for one provider binding in one workspace.
pub const DELETE_WORKSPACE_INSTALLS =
    \\DELETE FROM core.connector_installs
    \\WHERE provider = $1 AND workspace_id = $2::uuid
;

test "connector resource statements are schema-qualified and serialized" {
    const std = @import("std");
    try std.testing.expect(std.mem.indexOf(u8, LOCK_WORKSPACE_CONNECTOR, "pg_advisory_xact_lock") != null);
    try std.testing.expect(std.mem.indexOf(u8, DELETE_WORKSPACE_INSTALLS, "core.connector_installs") != null);
}
