//! Schema-qualified reads for generic App-webhook routing.

/// Resolve a provider installation/account to its workspace.
pub const SELECT_WORKSPACE =
    \\SELECT workspace_id::text FROM core.connector_installs
    \\WHERE provider = $1 AND external_account_id = $2
    \\LIMIT 1
;

/// Select active, granted fleets with an exact repository and event binding.
/// The caller requests one row beyond its fan-out ceiling and rejects the
/// delivery before queue writes when that sentinel row exists.
pub const SELECT_TARGETS =
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

test "App ingress statements use only core routing tables" {
    const std = @import("std");
    try std.testing.expect(std.mem.indexOf(u8, SELECT_WORKSPACE, "core.connector_installs") != null);
    try std.testing.expect(std.mem.indexOf(u8, SELECT_TARGETS, "core.fleets") != null);
    try std.testing.expect(std.mem.indexOf(u8, SELECT_TARGETS, "core.integration_grants") != null);
}
