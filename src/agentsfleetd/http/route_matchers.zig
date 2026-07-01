// Route matching helpers for the HTTP router.
//
// All matchers operate on a canonical `Path` view — a stack-allocated array
// of non-empty segment slices parsed once at the dispatch boundary. Matchers
// compare by segment count + segment[i] equality. Disambiguation is shape-
// driven, not order-driven; reserved segments live as explicit predicates
// inside catch-all matchers so any two matchers are mutually exclusive
// regardless of evaluation order.
//
// See `docs/REST_API_DESIGN_GUIDELINES.md` §7 (Matcher style — segment-based).

const std = @import("std");
const webhook = @import("route_matchers_webhook.zig");
const billing = @import("route_matchers_billing.zig");
const fleet = @import("route_matchers_fleet.zig");
const runner_m = @import("route_matchers_runner.zig");
const connectors = @import("route_matchers_connectors.zig");

const S_APPROVALS = "approvals";
const S_WORKSPACES = "workspaces";
const S_FLEETS = "fleets";
const S_BUNDLES = "bundles";
const S_AUTH = "auth";
const S_SESSIONS = "sessions";
const S_ALL = "all";
const S_APPROVE = "approve";
const S_VERIFY = "verify";
pub const PATH_MAX_SEGMENTS: usize = 16;

const APPROVAL_ACTION_APPROVE = ":approve";
const APPROVAL_ACTION_DENY = ":deny";

/// Canonical view of an HTTP path as a slice of segments.
///
/// The leading `/` is treated as a path marker (not a segment). Every other
/// run of bytes between `/` separators becomes a segment, including empty
/// runs from `//` or trailing slashes. Matchers MUST use `param()` (not
/// direct indexing) when extracting an ID slot so empty segments are
/// rejected at the matcher boundary, not the handler.
///
/// The dispatcher in `router.zig::match()` strips the API-version prefix
/// (e.g. `v1`) once via `tail(1)` and hands the rest to matchers. No "v1"
/// literal lives in any matcher body.
pub const Path = struct {
    const Self = @This();

    segs: []const []const u8,

    pub fn parse(path: []const u8, buf: *[PATH_MAX_SEGMENTS][]const u8) Path {
        if (path.len == 0) return .{ .segs = buf[0..0] };
        const start: usize = if (path[0] == '/') 1 else 0;
        if (start >= path.len) return .{ .segs = buf[0..0] };

        var n: usize = 0;
        var seg_start: usize = start;
        var i: usize = start;
        while (i < path.len) : (i += 1) {
            if (path[i] == '/') {
                if (n >= buf.len) return .{ .segs = buf[0..0] };
                buf[n] = path[seg_start..i];
                n += 1;
                seg_start = i + 1;
            }
        }
        // Always emit the final segment (may be empty if path ended in '/').
        if (n >= buf.len) return .{ .segs = buf[0..0] };
        buf[n] = path[seg_start..i];
        n += 1;
        return .{ .segs = buf[0..n] };
    }

    pub fn eq(self: Self, idx: usize, literal: []const u8) bool {
        return idx < self.segs.len and std.mem.eql(u8, self.segs[idx], literal);
    }

    /// Return the segment at `idx` if present and non-empty. Use this for
    /// path-parameter slots (workspace_id, fleet_id, etc.) — empty segments
    /// from `//` or trailing slashes get rejected at the matcher.
    pub fn param(self: Self, idx: usize) ?[]const u8 {
        if (idx >= self.segs.len) return null;
        if (self.segs[idx].len == 0) return null;
        return self.segs[idx];
    }

    /// Drop the first `n` segments. Used by the dispatcher to strip the
    /// API-version prefix before handing the path to matchers.
    pub fn tail(self: Self, n: usize) Path {
        if (n >= self.segs.len) return .{ .segs = &.{} };
        return .{ .segs = self.segs[n..] };
    }
};

// All matchers below operate on version-stripped paths. The dispatcher in
// `router.zig::match()` peels off the API-version segment (`v1`, future `v2`)
// before calling these. No matcher checks the API version.

// ── /auth/sessions/{session_id}[/{action}] ─────────────────────────────────
// Bare 3-seg form serves GET poll + DELETE cancel. `{id} == "all"` belongs
// to the bulk-delete matcher (rejected here for deterministic dispatch).
// 4-seg forms carry `/approve` (dashboard PATCH) or `/verify` (CLI POST).

pub fn matchAuthSession(p: Path) ?[]const u8 {
    if (p.segs.len != 3) return null;
    if (!p.eq(0, S_AUTH) or !p.eq(1, S_SESSIONS) or p.eq(2, S_ALL)) return null;
    return p.param(2);
}

pub fn matchAuthSessionsAll(p: Path) bool {
    return p.segs.len == 3 and p.eq(0, S_AUTH) and p.eq(1, S_SESSIONS) and p.eq(2, S_ALL);
}

fn matchAuthSessionAction(p: Path, action: []const u8) ?[]const u8 {
    if (p.segs.len != 4) return null;
    if (!p.eq(0, S_AUTH) or !p.eq(1, S_SESSIONS) or !p.eq(3, action)) return null;
    return p.param(2);
}

pub fn matchAuthSessionApprove(p: Path) ?[]const u8 {
    return matchAuthSessionAction(p, S_APPROVE);
}

pub fn matchAuthSessionVerify(p: Path) ?[]const u8 {
    return matchAuthSessionAction(p, S_VERIFY);
}

// Tenant + admin billing matchers (/admin/platform-keys, /api-keys,
// /tenants/me/billing/charges/.../telemetry) live in
// route_matchers_billing.zig (RULE FLL). Re-exported so call sites stay unchanged.
pub const matchAdminPlatformKey = billing.matchAdminPlatformKey;
pub const matchAdminModel = billing.matchAdminModel;
pub const matchTenantApiKeyById = billing.matchTenantApiKeyById;
pub const matchTenantMeteringPeriods = billing.matchTenantMeteringPeriods;

// ── /workspaces/{workspace_id}/{suffix} ────────────────────────────────────
// suffix ∈ {"fleets", "credentials", "fleet-keys", "events", "approvals"}.

pub fn matchWorkspaceSuffix(p: Path, suffix: []const u8) ?[]const u8 {
    if (p.segs.len != 3) return null;
    if (!p.eq(0, S_WORKSPACES)) return null;
    if (!p.eq(2, suffix)) return null;
    return p.param(1);
}

fn isFleetRuntimeSegment(p: Path, idx: usize) bool {
    return p.eq(idx, S_FLEETS) and (idx + 1 >= p.segs.len or !p.eq(idx + 1, S_BUNDLES));
}

// ── /workspaces/{ws}/credentials/{name} ────────────────────────────────────

pub const WorkspaceCredentialRoute = struct {
    workspace_id: []const u8,
    credential_name: []const u8,
};

pub fn matchWorkspaceCredential(p: Path) ?WorkspaceCredentialRoute {
    if (p.segs.len != 4) return null;
    if (!p.eq(0, S_WORKSPACES) or !p.eq(2, "credentials")) return null;
    const ws = p.param(1) orelse return null;
    const name = p.param(3) orelse return null;
    return .{ .workspace_id = ws, .credential_name = name };
}

// Connector OAuth matchers (GitHub App-install + Slack OAuth connect/callback)
// live in route_matchers_connectors.zig (RULE FLL — keep this file under 350
// lines); re-exported so the router's `matchers.match*Connector*` /
// `match*ConnectCallback` call sites stay unchanged.
pub const matchWorkspaceConnectorGithub = connectors.matchWorkspaceConnectorGithub;
pub const matchWorkspaceConnectorGithubConnect = connectors.matchWorkspaceConnectorGithubConnect;
pub const matchGithubConnectCallback = connectors.matchGithubConnectCallback;
pub const matchWorkspaceConnectorSlackConnect = connectors.matchWorkspaceConnectorSlackConnect;
pub const matchWorkspaceConnectorSlack = connectors.matchWorkspaceConnectorSlack;
pub const matchSlackConnectCallback = connectors.matchSlackConnectCallback;
pub const matchSlackEvents = connectors.matchSlackEvents;

// ── /workspaces/{ws}/fleet-keys/{fleet_key_id} ─────────────────────────────────

pub const WorkspaceFleetKeyRoute = struct {
    workspace_id: []const u8,
    fleet_key_id: []const u8,
};

pub fn matchWorkspaceFleetKeyDelete(p: Path) ?WorkspaceFleetKeyRoute {
    if (p.segs.len != 4) return null;
    if (!p.eq(0, S_WORKSPACES) or !p.eq(2, "fleet-keys")) return null;
    const ws = p.param(1) orelse return null;
    const fleet_key_id = p.param(3) orelse return null;
    return .{ .workspace_id = ws, .fleet_key_id = fleet_key_id };
}

// ── /workspaces/{ws}/fleets/{fleet_id} ───────────────────────────────────

pub const WorkspaceFleetRoute = struct {
    workspace_id: []const u8,
    fleet_id: []const u8,
};

pub fn matchWorkspaceFleet(p: Path) ?WorkspaceFleetRoute {
    if (p.segs.len != 4) return null;
    if (!p.eq(0, S_WORKSPACES) or !isFleetRuntimeSegment(p, 2)) return null;
    const ws = p.param(1) orelse return null;
    const fleet_id = p.param(3) orelse return null;
    return .{ .workspace_id = ws, .fleet_id = fleet_id };
}

// ── /workspaces/{ws}/fleets/{fleet_id}/{action} ──────────────────────────
// action ∈ {"events", "messages", "memories",
// "integration-requests", "integration-grants"}.

pub fn matchWorkspaceFleetAction(p: Path, action: []const u8) ?WorkspaceFleetRoute {
    if (p.segs.len != 5) return null;
    if (!p.eq(0, S_WORKSPACES) or !isFleetRuntimeSegment(p, 2)) return null;
    if (!p.eq(4, action)) return null;
    const ws = p.param(1) orelse return null;
    const fleet_id = p.param(3) orelse return null;
    return .{ .workspace_id = ws, .fleet_id = fleet_id };
}

// ── /workspaces/{ws}/fleets/{fleet_id}/events/stream ─────────────────────
// Distinct shape (6 segments) from the bare /events action (5 segments).

pub fn matchWorkspaceFleetEventsStream(p: Path) ?WorkspaceFleetRoute {
    if (p.segs.len != 6) return null;
    if (!p.eq(0, S_WORKSPACES) or !isFleetRuntimeSegment(p, 2)) return null;
    if (!p.eq(4, "events") or !p.eq(5, "stream")) return null;
    const ws = p.param(1) orelse return null;
    const fleet_id = p.param(3) orelse return null;
    return .{ .workspace_id = ws, .fleet_id = fleet_id };
}

// ── /workspaces/{ws}/fleets/{fleet_id}/{leaf_segment}/{leaf_id} ──────────
// Per-Fleet sub-resource leaves. Each route gets its own typed struct with a
// semantically named leaf field; the parsing logic is shared via a private
// helper.

const FleetLeafView = struct {
    workspace_id: []const u8,
    fleet_id: []const u8,
    leaf: []const u8,
};

fn matchFleetLeaf(p: Path, leaf_segment: []const u8) ?FleetLeafView {
    if (p.segs.len != 6) return null;
    if (!p.eq(0, S_WORKSPACES) or !isFleetRuntimeSegment(p, 2)) return null;
    if (!p.eq(4, leaf_segment)) return null;
    const ws = p.param(1) orelse return null;
    const fleet_id = p.param(3) orelse return null;
    const leaf = p.param(5) orelse return null;
    return .{ .workspace_id = ws, .fleet_id = fleet_id, .leaf = leaf };
}

pub const WorkspaceFleetGrantRoute = struct {
    workspace_id: []const u8,
    fleet_id: []const u8,
    grant_id: []const u8,
};

pub fn matchWorkspaceFleetGrant(p: Path) ?WorkspaceFleetGrantRoute {
    const v = matchFleetLeaf(p, "integration-grants") orelse return null;
    return .{ .workspace_id = v.workspace_id, .fleet_id = v.fleet_id, .grant_id = v.leaf };
}

// ── /workspaces/{ws}/approvals/{gate_id}[:approve|:deny] ───────────────────
// Both matchers share segs.len == 4 + segs[2] == "approvals"; mutual
// exclusivity is decided by whether the leaf ends with one of the colon
// actions.

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

fn approvalDecisionFromLeaf(leaf: []const u8) ?ApprovalResolveDecision {
    if (std.mem.endsWith(u8, leaf, APPROVAL_ACTION_APPROVE)) return .approve;
    if (std.mem.endsWith(u8, leaf, APPROVAL_ACTION_DENY)) return .deny;
    return null;
}

pub fn matchWorkspaceApprovalResolve(p: Path) ?ApprovalResolveRoute {
    if (p.segs.len != 4) return null;
    if (!p.eq(0, S_WORKSPACES) or !p.eq(2, S_APPROVALS)) return null;
    const ws = p.param(1) orelse return null;
    const leaf = p.param(3) orelse return null;
    const decision = approvalDecisionFromLeaf(leaf) orelse return null;
    const action_len = if (decision == .approve) APPROVAL_ACTION_APPROVE.len else APPROVAL_ACTION_DENY.len;
    if (leaf.len <= action_len) return null;
    const gate_id = leaf[0 .. leaf.len - action_len];
    if (std.mem.indexOfScalar(u8, gate_id, ':') != null) return null;
    return .{ .workspace_id = ws, .gate_id = gate_id, .decision = decision };
}

pub fn matchWorkspaceApprovalGate(p: Path) ?ApprovalGateRoute {
    if (p.segs.len != 4) return null;
    if (!p.eq(0, S_WORKSPACES) or !p.eq(2, S_APPROVALS)) return null;
    const ws = p.param(1) orelse return null;
    const leaf = p.param(3) orelse return null;
    if (approvalDecisionFromLeaf(leaf) != null) return null;
    if (std.mem.indexOfScalar(u8, leaf, ':') != null) return null;
    return .{ .workspace_id = ws, .gate_id = leaf };
}

// ── /webhooks/* family ─────────────────────────────────────────────────────
//
// Five shapes share the prefix; reserved second segments (svix, clerk) and
// reserved trailing actions (approval, grant-approval) are excluded from the
// catch-all matchers so any two matchers are mutually exclusive at the
// segment level.

// Webhook matchers live in route_matchers_webhook.zig (RULE FLL — keep this
// file under 350 lines). Re-exported so call sites stay unchanged.
pub const matchWebhookAction = webhook.matchWebhookAction;
pub const matchSvixWebhook = webhook.matchSvixWebhook;
pub const matchWebhook = webhook.matchWebhook;

pub const matchFleetRunner = fleet.matchFleetRunner;
pub const matchFleetRunnerEvents = fleet.matchFleetRunnerEvents;

// Runner control-plane matchers live in `route_matchers_runner.zig` (RULE FLL);
// re-exported here so `matchers.matchRunner*` is unchanged for the router.
pub const matchRunnerLeaseActivity = runner_m.matchRunnerLeaseActivity;
pub const matchRunnerMemory = runner_m.matchRunnerMemory;
pub const matchRunnerBundles = runner_m.matchRunnerBundles;
pub const matchRunnerLeaseRenew = runner_m.matchRunnerLeaseRenew;

test {
    _ = @import("route_matchers_test.zig");
}
