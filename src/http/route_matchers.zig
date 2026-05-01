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
const router = @import("router.zig");

pub const WebhookRoute = router.WebhookRoute;
pub const ZombieTelemetryRoute = router.ZombieTelemetryRoute;

pub const PATH_MAX_SEGMENTS: usize = 16;

const RESERVED_SVIX = "svix";
const RESERVED_CLERK = "clerk";
const RESERVED_APPROVAL = "approval";
const RESERVED_GRANT_APPROVAL = "grant-approval";
const RESERVED_LLM = "llm";

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

    pub fn eq(self: Path, idx: usize, literal: []const u8) bool {
        return idx < self.segs.len and std.mem.eql(u8, self.segs[idx], literal);
    }

    /// Return the segment at `idx` if present and non-empty. Use this for
    /// path-parameter slots (workspace_id, zombie_id, etc.) — empty segments
    /// from `//` or trailing slashes get rejected at the matcher.
    pub fn param(self: Path, idx: usize) ?[]const u8 {
        if (idx >= self.segs.len) return null;
        if (self.segs[idx].len == 0) return null;
        return self.segs[idx];
    }

    /// Drop the first `n` segments. Used by the dispatcher to strip the
    /// API-version prefix before handing the path to matchers.
    pub fn tail(self: Path, n: usize) Path {
        if (n >= self.segs.len) return .{ .segs = &.{} };
        return .{ .segs = self.segs[n..] };
    }
};

// All matchers below operate on version-stripped paths. The dispatcher in
// `router.zig::match()` peels off the API-version segment (`v1`, future `v2`)
// before calling these. No matcher checks the API version.

// ── /auth/sessions/{session_id} ─────────────────────────────────────────────

pub fn matchAuthSession(p: Path) ?[]const u8 {
    if (p.segs.len != 3) return null;
    if (!p.eq(0, "auth") or !p.eq(1, "sessions")) return null;
    return p.param(2);
}

// ── /admin/platform-keys/{provider} ────────────────────────────────────────

pub fn matchAdminPlatformKey(p: Path) ?[]const u8 {
    if (p.segs.len != 3) return null;
    if (!p.eq(0, "admin") or !p.eq(1, "platform-keys")) return null;
    return p.param(2);
}

// ── /api-keys/{id} ─────────────────────────────────────────────────────────

pub fn matchTenantApiKeyById(p: Path) ?[]const u8 {
    if (p.segs.len != 2) return null;
    if (!p.eq(0, "api-keys")) return null;
    return p.param(1);
}

// ── /workspaces/{workspace_id} ─────────────────────────────────────────────

pub fn matchWorkspace(p: Path) ?[]const u8 {
    if (p.segs.len != 2) return null;
    if (!p.eq(0, "workspaces")) return null;
    return p.param(1);
}

// ── /workspaces/{workspace_id}/{suffix} ────────────────────────────────────
// suffix ∈ {"zombies", "credentials", "agent-keys", "events", "approvals"}.

pub fn matchWorkspaceSuffix(p: Path, suffix: []const u8) ?[]const u8 {
    if (p.segs.len != 3) return null;
    if (!p.eq(0, "workspaces")) return null;
    if (!p.eq(2, suffix)) return null;
    return p.param(1);
}

// ── /workspaces/{ws}/credentials/llm  (BYOK reserved) ──────────────────────

pub fn matchWorkspaceLlmCredential(p: Path) ?[]const u8 {
    if (p.segs.len != 4) return null;
    if (!p.eq(0, "workspaces") or !p.eq(2, "credentials")) return null;
    if (!p.eq(3, RESERVED_LLM)) return null;
    return p.param(1);
}

// ── /workspaces/{ws}/credentials/{name}  (name != "llm") ───────────────────

pub const WorkspaceCredentialRoute = struct {
    workspace_id: []const u8,
    credential_name: []const u8,
};

pub fn matchWorkspaceCredential(p: Path) ?WorkspaceCredentialRoute {
    if (p.segs.len != 4) return null;
    if (!p.eq(0, "workspaces") or !p.eq(2, "credentials")) return null;
    if (p.eq(3, RESERVED_LLM)) return null;
    const ws = p.param(1) orelse return null;
    const name = p.param(3) orelse return null;
    return .{ .workspace_id = ws, .credential_name = name };
}

// ── /workspaces/{ws}/agent-keys/{agent_id} ─────────────────────────────────

pub const WorkspaceAgentRoute = struct {
    workspace_id: []const u8,
    agent_id: []const u8,
};

pub fn matchWorkspaceAgentDelete(p: Path) ?WorkspaceAgentRoute {
    if (p.segs.len != 4) return null;
    if (!p.eq(0, "workspaces") or !p.eq(2, "agent-keys")) return null;
    const ws = p.param(1) orelse return null;
    const agent_id = p.param(3) orelse return null;
    return .{ .workspace_id = ws, .agent_id = agent_id };
}

// ── /workspaces/{ws}/zombies/{zombie_id} ───────────────────────────────────

pub const WorkspaceZombieRoute = struct {
    workspace_id: []const u8,
    zombie_id: []const u8,
};

pub fn matchWorkspaceZombie(p: Path) ?WorkspaceZombieRoute {
    if (p.segs.len != 4) return null;
    if (!p.eq(0, "workspaces") or !p.eq(2, "zombies")) return null;
    const ws = p.param(1) orelse return null;
    const zid = p.param(3) orelse return null;
    return .{ .workspace_id = ws, .zombie_id = zid };
}

// ── /workspaces/{ws}/zombies/{zombie_id}/{action} ──────────────────────────
// action ∈ {"events", "messages", "current-run", "telemetry", "memories",
// "integration-requests", "integration-grants"}.

pub fn matchWorkspaceZombieAction(p: Path, action: []const u8) ?WorkspaceZombieRoute {
    if (p.segs.len != 5) return null;
    if (!p.eq(0, "workspaces") or !p.eq(2, "zombies")) return null;
    if (!p.eq(4, action)) return null;
    const ws = p.param(1) orelse return null;
    const zid = p.param(3) orelse return null;
    return .{ .workspace_id = ws, .zombie_id = zid };
}

// ── /workspaces/{ws}/zombies/{zombie_id}/events/stream ─────────────────────
// Distinct shape (6 segments) from the bare /events action (5 segments).

pub fn matchWorkspaceZombieEventsStream(p: Path) ?WorkspaceZombieRoute {
    if (p.segs.len != 6) return null;
    if (!p.eq(0, "workspaces") or !p.eq(2, "zombies")) return null;
    if (!p.eq(4, "events") or !p.eq(5, "stream")) return null;
    const ws = p.param(1) orelse return null;
    const zid = p.param(3) orelse return null;
    return .{ .workspace_id = ws, .zombie_id = zid };
}

// ── /workspaces/{ws}/zombies/{zombie_id}/{leaf_segment}/{leaf_id} ──────────
// Per-zombie sub-resource leaves. Each route gets its own typed struct with a
// semantically named leaf field; the parsing logic is shared via a private
// helper.

const ZombieLeafView = struct {
    workspace_id: []const u8,
    zombie_id: []const u8,
    leaf: []const u8,
};

fn matchZombieLeaf(p: Path, leaf_segment: []const u8) ?ZombieLeafView {
    if (p.segs.len != 6) return null;
    if (!p.eq(0, "workspaces") or !p.eq(2, "zombies")) return null;
    if (!p.eq(4, leaf_segment)) return null;
    const ws = p.param(1) orelse return null;
    const zid = p.param(3) orelse return null;
    const leaf = p.param(5) orelse return null;
    return .{ .workspace_id = ws, .zombie_id = zid, .leaf = leaf };
}

pub const WorkspaceZombieGrantRoute = struct {
    workspace_id: []const u8,
    zombie_id: []const u8,
    grant_id: []const u8,
};

pub fn matchWorkspaceZombieGrant(p: Path) ?WorkspaceZombieGrantRoute {
    const v = matchZombieLeaf(p, "integration-grants") orelse return null;
    return .{ .workspace_id = v.workspace_id, .zombie_id = v.zombie_id, .grant_id = v.leaf };
}

pub const WorkspaceZombieMemoryRoute = struct {
    workspace_id: []const u8,
    zombie_id: []const u8,
    memory_key: []const u8,
};

pub fn matchWorkspaceZombieMemoryByKey(p: Path) ?WorkspaceZombieMemoryRoute {
    const v = matchZombieLeaf(p, "memories") orelse return null;
    return .{ .workspace_id = v.workspace_id, .zombie_id = v.zombie_id, .memory_key = v.leaf };
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
    if (!p.eq(0, "workspaces") or !p.eq(2, "approvals")) return null;
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
    if (!p.eq(0, "workspaces") or !p.eq(2, "approvals")) return null;
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

pub fn matchWebhookAction(p: Path, action: []const u8) ?[]const u8 {
    if (p.segs.len != 3) return null;
    if (!p.eq(0, "webhooks")) return null;
    if (!p.eq(2, action)) return null;
    if (p.eq(1, RESERVED_SVIX) or p.eq(1, RESERVED_CLERK)) return null;
    if (p.eq(1, RESERVED_APPROVAL) or p.eq(1, RESERVED_GRANT_APPROVAL)) return null;
    return p.param(1);
}

pub fn matchSvixWebhook(p: Path) ?[]const u8 {
    if (p.segs.len != 3) return null;
    if (!p.eq(0, "webhooks") or !p.eq(1, RESERVED_SVIX)) return null;
    return p.param(2);
}

pub fn matchWebhook(p: Path) ?WebhookRoute {
    if (p.segs.len < 2 or p.segs.len > 3) return null;
    if (!p.eq(0, "webhooks")) return null;
    // Reserved literals — never accepted as a zombie_id at the slot-1 position
    // (svix is the receive_svix_webhook prefix; clerk is the signup webhook;
    // approval / grant-approval are dedicated action endpoints).
    if (p.eq(1, RESERVED_SVIX) or p.eq(1, RESERVED_CLERK)) return null;
    if (p.eq(1, RESERVED_APPROVAL) or p.eq(1, RESERVED_GRANT_APPROVAL)) return null;
    const zid = p.param(1) orelse return null;
    if (p.segs.len == 2) return .{ .zombie_id = zid, .secret = null };
    if (p.eq(2, RESERVED_APPROVAL) or p.eq(2, RESERVED_GRANT_APPROVAL)) return null;
    const secret = p.param(2) orelse return null;
    return .{ .zombie_id = zid, .secret = secret };
}

test {
    _ = @import("route_matchers_test.zig");
}
