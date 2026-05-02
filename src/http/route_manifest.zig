// Canonical route manifest — the full (method, path) surface the server dispatches.
// Kept in sync with match() in router.zig AND with public/openapi.json via the
// sync gate (scripts/check_openapi_sync.py). A drift between any two of these
// three sources fails `make openapi`.
//
// Paths use {param} placeholders (OpenAPI style). Methods are uppercase.
//
// To add / rename / remove a route:
//   1. Update match() in router.zig so the path actually dispatches.
//   2. Update this manifest.
//   3. Update the corresponding YAML under public/openapi/paths/<tag>.yaml.
//   4. `make openapi` must pass before commit.

pub const Entry = struct {
    method: []const u8,
    path: []const u8,
};

pub const entries = [_]Entry{
    // Health
    .{ .method = "GET", .path = "/healthz" },
    .{ .method = "GET", .path = "/readyz" },
    .{ .method = "GET", .path = "/metrics" },

    // Authentication
    .{ .method = "POST", .path = "/v1/auth/sessions" },
    .{ .method = "GET", .path = "/v1/auth/sessions/{session_id}" },
    .{ .method = "PATCH", .path = "/v1/auth/sessions/{session_id}" },

    // Admin
    .{ .method = "GET", .path = "/v1/admin/platform-keys" },
    .{ .method = "PUT", .path = "/v1/admin/platform-keys" },
    .{ .method = "DELETE", .path = "/v1/admin/platform-keys/{provider}" },

    // Workspaces
    .{ .method = "POST", .path = "/v1/workspaces" },
    .{ .method = "PATCH", .path = "/v1/workspaces/{workspace_id}" },

    // Tenant billing (plan + balance snapshot)
    .{ .method = "GET", .path = "/v1/tenants/me/billing" },

    // Tenant-scoped workspace list (backs the dashboard workspace switcher)
    .{ .method = "GET", .path = "/v1/tenants/me/workspaces" },

    // Tenant-scoped doctor block — provider posture + resolver state.
    .{ .method = "GET", .path = "/v1/tenants/me/diagnostics" },

    // Credentials
    .{ .method = "GET", .path = "/v1/workspaces/{workspace_id}/credentials" },
    .{ .method = "POST", .path = "/v1/workspaces/{workspace_id}/credentials" },
    .{ .method = "DELETE", .path = "/v1/workspaces/{workspace_id}/credentials/{credential_name}" },
    .{ .method = "GET", .path = "/v1/workspaces/{workspace_id}/credentials/llm" },
    .{ .method = "PUT", .path = "/v1/workspaces/{workspace_id}/credentials/llm" },
    .{ .method = "DELETE", .path = "/v1/workspaces/{workspace_id}/credentials/llm" },

    // Zombies
    .{ .method = "GET", .path = "/v1/workspaces/{workspace_id}/zombies" },
    .{ .method = "POST", .path = "/v1/workspaces/{workspace_id}/zombies" },
    .{ .method = "PATCH", .path = "/v1/workspaces/{workspace_id}/zombies/{zombie_id}" },
    .{ .method = "POST", .path = "/v1/workspaces/{workspace_id}/zombies/{zombie_id}/messages" },
    .{ .method = "GET", .path = "/v1/workspaces/{workspace_id}/zombies/{zombie_id}/events" },
    .{ .method = "GET", .path = "/v1/workspaces/{workspace_id}/zombies/{zombie_id}/events/stream" },
    .{ .method = "GET", .path = "/v1/workspaces/{workspace_id}/events" },
    .{ .method = "DELETE", .path = "/v1/workspaces/{workspace_id}/zombies/{zombie_id}/current-run" },

    // Approval inbox
    .{ .method = "GET", .path = "/v1/workspaces/{workspace_id}/approvals" },
    .{ .method = "GET", .path = "/v1/workspaces/{workspace_id}/approvals/{gate_id}" },
    .{ .method = "POST", .path = "/v1/workspaces/{workspace_id}/approvals/{gate_id}:approve" },
    .{ .method = "POST", .path = "/v1/workspaces/{workspace_id}/approvals/{gate_id}:deny" },

    // Zombie webhook ingest
    .{ .method = "POST", .path = "/v1/webhooks/{zombie_id}" },

    // Webhooks (clerk + approval + grant-approval + svix + github)
    .{ .method = "POST", .path = "/v1/webhooks/clerk" },
    .{ .method = "POST", .path = "/v1/webhooks/svix/{zombie_id}" },
    .{ .method = "POST", .path = "/v1/webhooks/{zombie_id}/approval" },
    .{ .method = "POST", .path = "/v1/webhooks/{zombie_id}/grant-approval" },
    .{ .method = "POST", .path = "/v1/webhooks/{zombie_id}/github" },

    // Telemetry
    .{ .method = "GET", .path = "/internal/v1/telemetry" },
    .{ .method = "GET", .path = "/v1/workspaces/{workspace_id}/zombies/{zombie_id}/telemetry" },

    // Memory — workspace-scoped /memories collection (GET list-or-search,
    // POST store) + /memories/{key} (DELETE idempotent 204).
    .{ .method = "GET", .path = "/v1/workspaces/{workspace_id}/zombies/{zombie_id}/memories" },
    .{ .method = "POST", .path = "/v1/workspaces/{workspace_id}/zombies/{zombie_id}/memories" },
    .{ .method = "DELETE", .path = "/v1/workspaces/{workspace_id}/zombies/{zombie_id}/memories/{memory_key}" },

    // Execute
    .{ .method = "POST", .path = "/v1/execute" },

    // Integration Grants
    .{ .method = "POST", .path = "/v1/workspaces/{workspace_id}/zombies/{zombie_id}/integration-requests" },
    .{ .method = "GET", .path = "/v1/workspaces/{workspace_id}/zombies/{zombie_id}/integration-grants" },
    .{ .method = "DELETE", .path = "/v1/workspaces/{workspace_id}/zombies/{zombie_id}/integration-grants/{grant_id}" },

    // Agent keys (workspace-scoped)
    .{ .method = "POST", .path = "/v1/workspaces/{workspace_id}/agent-keys" },
    .{ .method = "GET", .path = "/v1/workspaces/{workspace_id}/agent-keys" },
    .{ .method = "DELETE", .path = "/v1/workspaces/{workspace_id}/agent-keys/{agent_id}" },

    // Tenant API keys
    .{ .method = "POST", .path = "/v1/api-keys" },
    .{ .method = "GET", .path = "/v1/api-keys" },
    .{ .method = "PATCH", .path = "/v1/api-keys/{id}" },
    .{ .method = "DELETE", .path = "/v1/api-keys/{id}" },
};
