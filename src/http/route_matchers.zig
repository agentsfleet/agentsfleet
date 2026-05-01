// Route matching helpers for the HTTP router.
//
// Extracted from router.zig to keep files under 400 lines.
// Pure functions that parse URL paths into route parameters.

const std = @import("std");
const router = @import("router.zig");

pub const ZombieTelemetryRoute = router.ZombieTelemetryRoute;

const prefix_workspaces = "/v1/workspaces/";
const prefix_agents = "/v1/agents/";

pub fn matchWorkspaceSuffix(path: []const u8, suffix: []const u8) ?[]const u8 {
    if (!std.mem.startsWith(u8, path, prefix_workspaces)) return null;
    if (!std.mem.endsWith(u8, path, suffix)) return null;
    const inner = path[prefix_workspaces.len .. path.len - suffix.len];
    if (!isSingleSegment(inner)) return null;
    return inner;
}

pub fn isSingleSegment(value: []const u8) bool {
    return value.len > 0 and std.mem.indexOfScalar(u8, value, '/') == null;
}

// matchZombieTelemetry matches /v1/workspaces/{ws_id}/zombies/{zombie_id}/telemetry.
pub fn matchZombieTelemetry(path: []const u8) ?ZombieTelemetryRoute {
    return matchWorkspaceZombieSuffix(path, "/telemetry");
}

// M24_001: generic helper for /v1/workspaces/{ws}/zombies/{id}/{suffix} routes.
// Returns ZombieTelemetryRoute (ws_id + zombie_id) for any suffix.
pub fn matchWorkspaceZombieSuffix(path: []const u8, suffix: []const u8) ?ZombieTelemetryRoute {
    const prefix = "/v1/workspaces/";
    const mid = "/zombies/";

    if (!std.mem.startsWith(u8, path, prefix)) return null;
    if (!std.mem.endsWith(u8, path, suffix)) return null;

    const inner = path[prefix.len .. path.len - suffix.len];
    const sep = std.mem.indexOf(u8, inner, mid) orelse return null;
    const ws_id = inner[0..sep];
    const zombie_id = inner[sep + mid.len ..];

    if (!isSingleSegment(ws_id)) return null;
    if (!isSingleSegment(zombie_id)) return null;
    return .{ .workspace_id = ws_id, .zombie_id = zombie_id };
}

/// Match `/v1/webhooks/{zombie_id}` and return the zombie id. The two-segment
/// `/v1/webhooks/{zombie_id}/{action}` form is matched by `matchWebhookAction`
/// per registered action (`/approval`, `/grant-approval`, `/github`, …).
pub fn matchWebhookRoute(path: []const u8) ?[]const u8 {
    const prefix = "/v1/webhooks/";
    if (!std.mem.startsWith(u8, path, prefix)) return null;
    const rest = path[prefix.len..];
    if (!isSingleSegment(rest)) return null;
    return rest;
}

// ── Tests ──────────────────────────────────────────────────────────────

// M10_001: matchRunAction test removed — function deleted.

// matchWebhookAction matches /v1/webhooks/{zombie_id}{action} and returns the zombie_id.
// `action` is the full suffix (e.g. "/approval") — M28 migration replaced the
// Google-style ":action" custom-method form with a direct subpath so public docs
// can parameterize it as /v1/webhooks/{zombie_id}/{action} without OpenAPI-validator
// rejection of the colon.
pub fn matchWebhookAction(path: []const u8, action: []const u8) ?[]const u8 {
    const prefix = "/v1/webhooks/";
    if (!std.mem.startsWith(u8, path, prefix)) return null;
    if (!std.mem.endsWith(u8, path, action)) return null;
    const inner = path[prefix.len .. path.len - action.len];
    if (!isSingleSegment(inner)) return null;
    return inner;
}

// matchWorkspaceZombieAction matches /v1/workspaces/{ws}/zombies/{zombie_id}{action}.
// `action` is the full suffix (e.g. "/steer") — M28 migration replaced
// the Google-style ":action" custom-method form with a validator-friendly subpath.
pub fn matchWorkspaceZombieAction(path: []const u8, action: []const u8) ?WorkspaceZombieRoute {
    const prefix = "/v1/workspaces/";
    const mid = "/zombies/";

    if (!std.mem.startsWith(u8, path, prefix)) return null;
    if (!std.mem.endsWith(u8, path, action)) return null;

    const inner = path[prefix.len .. path.len - action.len];
    const sep = std.mem.indexOf(u8, inner, mid) orelse return null;
    const ws_id = inner[0..sep];
    const zombie_id = inner[sep + mid.len ..];

    if (!isSingleSegment(ws_id)) return null;
    if (!isSingleSegment(zombie_id)) return null;
    return .{ .workspace_id = ws_id, .zombie_id = zombie_id };
}

// M24_001: WorkspaceZombieRoute carries workspace_id + zombie_id for /v1/workspaces/{ws}/zombies/{zombie_id}.
pub const WorkspaceZombieRoute = struct {
    workspace_id: []const u8,
    zombie_id: []const u8,
};

// M24_001: matchWorkspaceZombie matches /v1/workspaces/{ws_id}/zombies/{zombie_id}.
// Used for DELETE and (in later slices) per-zombie sub-resources.
pub fn matchWorkspaceZombie(path: []const u8) ?WorkspaceZombieRoute {
    const prefix = "/v1/workspaces/";
    const mid = "/zombies/";

    if (!std.mem.startsWith(u8, path, prefix)) return null;
    const rest = path[prefix.len..];
    const sep = std.mem.indexOf(u8, rest, mid) orelse return null;
    const ws_id = rest[0..sep];
    const zombie_id = rest[sep + mid.len ..];

    if (!isSingleSegment(ws_id)) return null;
    if (!isSingleSegment(zombie_id)) return null;
    return .{ .workspace_id = ws_id, .zombie_id = zombie_id };
}

// WorkspaceCredentialRoute carries workspace_id + credential_name for the
// per-credential DELETE endpoint.
pub const WorkspaceCredentialRoute = struct {
    workspace_id: []const u8,
    credential_name: []const u8,
};

// matchWorkspaceCredential matches /v1/workspaces/{ws}/credentials/{name}.
// Rejects /credentials/llm — that suffix is owned by the BYOK route family.
pub fn matchWorkspaceCredential(path: []const u8) ?WorkspaceCredentialRoute {
    const prefix = "/v1/workspaces/";
    const mid = "/credentials/";
    if (!std.mem.startsWith(u8, path, prefix)) return null;
    const rest = path[prefix.len..];
    const slash = std.mem.indexOf(u8, rest, mid) orelse return null;
    const workspace_id = rest[0..slash];
    if (!isSingleSegment(workspace_id)) return null;
    const credential_name = rest[slash + mid.len ..];
    if (!isSingleSegment(credential_name)) return null;
    if (std.mem.eql(u8, credential_name, "llm")) return null;
    return .{ .workspace_id = workspace_id, .credential_name = credential_name };
}

// M9_001 / M28_002 §0: WorkspaceAgentRoute carries workspace_id + agent_id for agent-key DELETE.
pub const WorkspaceAgentRoute = struct {
    workspace_id: []const u8,
    agent_id: []const u8,
};

// M9_001 / M28_002 §0: matchWorkspaceAgentDelete matches /v1/workspaces/{ws}/agent-keys/{agent_id}.
pub fn matchWorkspaceAgentDelete(path: []const u8) ?WorkspaceAgentRoute {
    const prefix = "/v1/workspaces/";
    const mid = "/agent-keys/";
    if (!std.mem.startsWith(u8, path, prefix)) return null;
    const rest = path[prefix.len..];
    const slash = std.mem.indexOf(u8, rest, mid) orelse return null;
    const workspace_id = rest[0..slash];
    if (!isSingleSegment(workspace_id)) return null;
    const agent_id = rest[slash + mid.len ..];
    if (!isSingleSegment(agent_id)) return null;
    return .{ .workspace_id = workspace_id, .agent_id = agent_id };
}

// M24_001: WorkspaceZombieGrantRoute carries ws_id + zombie_id + grant_id for grant DELETE.
pub const WorkspaceZombieGrantRoute = struct {
    workspace_id: []const u8,
    zombie_id: []const u8,
    grant_id: []const u8,
};

// M24_001: matchWorkspaceZombieGrant matches
//   /v1/workspaces/{ws}/zombies/{zombie_id}/integration-grants/{grant_id}.
pub fn matchWorkspaceZombieGrant(path: []const u8) ?WorkspaceZombieGrantRoute {
    const prefix = "/v1/workspaces/";
    const ws_mid = "/zombies/";
    const grant_mid = "/integration-grants/";

    if (!std.mem.startsWith(u8, path, prefix)) return null;
    const rest = path[prefix.len..];

    const ws_sep = std.mem.indexOf(u8, rest, ws_mid) orelse return null;
    const ws_id = rest[0..ws_sep];
    if (!isSingleSegment(ws_id)) return null;

    const after_ws = rest[ws_sep + ws_mid.len ..];
    const grant_sep = std.mem.indexOf(u8, after_ws, grant_mid) orelse return null;
    const zombie_id = after_ws[0..grant_sep];
    if (!isSingleSegment(zombie_id)) return null;

    const grant_id = after_ws[grant_sep + grant_mid.len ..];
    if (!isSingleSegment(grant_id)) return null;

    return .{ .workspace_id = ws_id, .zombie_id = zombie_id, .grant_id = grant_id };
}


// ── Approval inbox routes ──────────────────────────────────────────────

/// Colon-noun action segments for the resolve endpoints. Owned here because
/// the matcher is the single source of truth for URL shape; tests, manifest,
/// and TS clients import these.
pub const APPROVAL_ACTION_APPROVE = ":approve";
pub const APPROVAL_ACTION_DENY = ":deny";
pub const APPROVALS_PATH_SEGMENT = "/approvals/";
const WORKSPACES_PREFIX = "/v1/workspaces/";

pub const ApprovalGateRoute = struct {
    workspace_id: []const u8,
    gate_id: []const u8,
};

pub const ApprovalResolveDecision = enum { approve, deny };

pub const ApprovalResolveRoute = struct {
    workspace_id: []const u8,
    gate_id: []const u8,
    decision: ApprovalResolveDecision,
};

/// Matches /v1/workspaces/{ws}/approvals/{gate_id}:approve|:deny.
/// REST §1 colon-noun operation form.
pub fn matchWorkspaceApprovalResolve(path: []const u8) ?ApprovalResolveRoute {
    if (!std.mem.startsWith(u8, path, WORKSPACES_PREFIX)) return null;

    const action_str: []const u8 = if (std.mem.endsWith(u8, path, APPROVAL_ACTION_APPROVE))
        APPROVAL_ACTION_APPROVE
    else if (std.mem.endsWith(u8, path, APPROVAL_ACTION_DENY))
        APPROVAL_ACTION_DENY
    else
        return null;

    const decision: ApprovalResolveDecision = if (action_str.len == APPROVAL_ACTION_APPROVE.len) .approve else .deny;
    const inner = path[WORKSPACES_PREFIX.len .. path.len - action_str.len];
    const sep = std.mem.indexOf(u8, inner, APPROVALS_PATH_SEGMENT) orelse return null;
    const ws_id = inner[0..sep];
    const gate_id = inner[sep + APPROVALS_PATH_SEGMENT.len ..];

    if (!isSingleSegment(ws_id)) return null;
    if (gate_id.len == 0 or std.mem.indexOfScalar(u8, gate_id, '/') != null) return null;
    if (std.mem.indexOfScalar(u8, gate_id, ':') != null) return null;
    return .{ .workspace_id = ws_id, .gate_id = gate_id, .decision = decision };
}

/// Matches /v1/workspaces/{ws}/approvals/{gate_id} (single-resource GET).
/// Returns null when the path ends in :approve / :deny so the resolve
/// route claims those.
pub fn matchWorkspaceApprovalGate(path: []const u8) ?ApprovalGateRoute {
    if (std.mem.endsWith(u8, path, APPROVAL_ACTION_APPROVE)) return null;
    if (std.mem.endsWith(u8, path, APPROVAL_ACTION_DENY)) return null;
    if (!std.mem.startsWith(u8, path, WORKSPACES_PREFIX)) return null;

    const inner = path[WORKSPACES_PREFIX.len..];
    const sep = std.mem.indexOf(u8, inner, APPROVALS_PATH_SEGMENT) orelse return null;
    const ws_id = inner[0..sep];
    const gate_id = inner[sep + APPROVALS_PATH_SEGMENT.len ..];

    if (!isSingleSegment(ws_id)) return null;
    if (gate_id.len == 0 or std.mem.indexOfScalar(u8, gate_id, '/') != null) return null;
    if (std.mem.indexOfScalar(u8, gate_id, ':') != null) return null;
    return .{ .workspace_id = ws_id, .gate_id = gate_id };
}

test {
    _ = @import("route_matchers_test.zig");
}
