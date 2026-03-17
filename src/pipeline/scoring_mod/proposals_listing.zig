const std = @import("std");
const pg = @import("pg");
const auto_approval = @import("proposals_auto_approval.zig");
const shared = @import("proposals_shared.zig");

pub const ApplyProposalResult = auto_approval.ApplyProposalResult;
pub const AppliedProposalTelemetry = shared.AppliedProposalTelemetry;
pub const ManualProposalSummary = shared.ManualProposalSummary;
pub const ProposalSummary = shared.ProposalSummary;

pub fn loadAppliedProposalTelemetry(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    proposal_id: []const u8,
) !?AppliedProposalTelemetry {
    var q = try conn.query(
        \\SELECT proposal_id, agent_id, workspace_id, trigger_reason, approval_mode
        \\FROM agent_improvement_proposals
        \\WHERE proposal_id = $1
        \\  AND status = $2
        \\LIMIT 1
    , .{ proposal_id, shared.STATUS_APPLIED });

    const row = (try q.next()) orelse {
        q.deinit();
        return null;
    };
    const telemetry = AppliedProposalTelemetry{
        .proposal_id = try alloc.dupe(u8, try row.get([]const u8, 0)),
        .agent_id = try alloc.dupe(u8, try row.get([]const u8, 1)),
        .workspace_id = try alloc.dupe(u8, try row.get([]const u8, 2)),
        .trigger_reason = try alloc.dupe(u8, try row.get([]const u8, 3)),
        .approval_mode = try alloc.dupe(u8, try row.get([]const u8, 4)),
        .fields_changed = undefined,
    };
    q.drain() catch {};
    q.deinit();
    var result = telemetry;
    errdefer {
        alloc.free(result.proposal_id);
        alloc.free(result.agent_id);
        alloc.free(result.workspace_id);
        alloc.free(result.trigger_reason);
        alloc.free(result.approval_mode);
    }
    result.fields_changed = try loadAppliedProposalFieldsChanged(conn, alloc, proposal_id);
    return result;
}

pub fn listAppliedAutoProposalTelemetryAt(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    applied_at_ms: i64,
) ![]AppliedProposalTelemetry {
    var q = try conn.query(
        \\SELECT proposal_id
        \\FROM agent_improvement_proposals
        \\WHERE status = $1
        \\  AND applied_by = $2
        \\  AND updated_at = $3
        \\ORDER BY proposal_id ASC
    , .{ shared.STATUS_APPLIED, shared.APPLIED_BY_SYSTEM_AUTO, applied_at_ms });

    var ids: std.ArrayList([]u8) = .{};
    defer {
        for (ids.items) |id| alloc.free(id);
        ids.deinit(alloc);
    }

    while (try q.next()) |row| {
        try ids.append(alloc, try alloc.dupe(u8, try row.get([]const u8, 0)));
    }
    q.deinit();

    var items: std.ArrayList(AppliedProposalTelemetry) = .{};
    errdefer {
        for (items.items) |*item| item.deinit(alloc);
        items.deinit(alloc);
    }

    for (ids.items) |id| {
        const telemetry = (try loadAppliedProposalTelemetry(conn, alloc, id)) orelse continue;
        try items.append(alloc, telemetry);
    }
    return try items.toOwnedSlice(alloc);
}

pub fn listOpenProposals(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    agent_id: []const u8,
    limit: u32,
) ![]ProposalSummary {
    const batch_limit: i32 = @intCast(if (limit == 0) shared.DEFAULT_RECONCILE_BATCH_LIMIT else limit);
    var list: std.ArrayList(ProposalSummary) = .{};
    errdefer {
        for (list.items) |*item| item.deinit(alloc);
        list.deinit(alloc);
    }

    var q = try conn.query(
        \\SELECT proposal_id, trigger_reason, proposed_changes, config_version_id, approval_mode, status, auto_apply_at, created_at, updated_at
        \\FROM agent_improvement_proposals
        \\WHERE agent_id = $1
        \\  AND generation_status = $2
        \\  AND (
        \\    (approval_mode = $3 AND status = $4)
        \\    OR
        \\    (approval_mode = $5 AND status = $6)
        \\  )
        \\ORDER BY
        \\  CASE WHEN status = $4 THEN 0 ELSE 1 END ASC,
        \\  COALESCE(auto_apply_at, created_at) ASC,
        \\  proposal_id ASC
        \\LIMIT $7
    , .{
        agent_id,
        shared.GENERATION_STATUS_READY,
        shared.ApprovalMode.auto.label(),
        shared.STATUS_VETO_WINDOW,
        shared.ApprovalMode.manual.label(),
        shared.STATUS_PENDING_REVIEW,
        batch_limit,
    });

    while (try q.next()) |row| {
        try list.append(alloc, try proposalSummaryFromRow(alloc, row));
    }
    q.deinit();
    return try list.toOwnedSlice(alloc);
}

pub fn listManualProposals(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    agent_id: []const u8,
    limit: u32,
) ![]ManualProposalSummary {
    const batch_limit: i32 = @intCast(if (limit == 0) shared.DEFAULT_RECONCILE_BATCH_LIMIT else limit);
    var list: std.ArrayList(ManualProposalSummary) = .{};
    errdefer {
        for (list.items) |*item| item.deinit(alloc);
        list.deinit(alloc);
    }

    var q = try conn.query(
        \\SELECT proposal_id, trigger_reason, proposed_changes, config_version_id, approval_mode, status, auto_apply_at, created_at, updated_at
        \\FROM agent_improvement_proposals
        \\WHERE agent_id = $1
        \\  AND approval_mode = $2
        \\  AND generation_status = $3
        \\  AND status = $4
        \\ORDER BY created_at DESC, proposal_id DESC
        \\LIMIT $5
    , .{
        agent_id,
        shared.ApprovalMode.manual.label(),
        shared.GENERATION_STATUS_READY,
        shared.STATUS_PENDING_REVIEW,
        batch_limit,
    });

    while (try q.next()) |row| {
        try list.append(alloc, try proposalSummaryFromRow(alloc, row));
    }
    q.deinit();
    return try list.toOwnedSlice(alloc);
}

pub fn approveManualProposal(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    agent_id: []const u8,
    proposal_id: []const u8,
    operator_identity: []const u8,
    now_ms: i64,
) !?ApplyProposalResult {
    var proposal = (try loadManualProposalForDecision(conn, alloc, agent_id, proposal_id)) orelse return null;
    defer proposal.deinit(alloc);

    const applied_by = try std.fmt.allocPrint(alloc, "{s}{s}", .{ shared.APPLIED_BY_OPERATOR_PREFIX, operator_identity });
    defer alloc.free(applied_by);

    return try auto_approval.applyProposal(
        conn,
        alloc,
        proposal.agent_id,
        proposal.workspace_id,
        proposal.proposal_id,
        proposal.config_version_id,
        proposal.proposed_changes,
        shared.STATUS_PENDING_REVIEW,
        applied_by,
        now_ms,
    );
}

pub fn rejectManualProposal(
    conn: *pg.Conn,
    agent_id: []const u8,
    proposal_id: []const u8,
    reason: []const u8,
    now_ms: i64,
) !bool {
    return updateProposalDecision(
        conn,
        proposal_id,
        agent_id,
        shared.ApprovalMode.manual.label(),
        shared.STATUS_PENDING_REVIEW,
        shared.STATUS_REJECTED,
        reason,
        now_ms,
    );
}

pub fn vetoAutoProposal(
    conn: *pg.Conn,
    agent_id: []const u8,
    proposal_id: []const u8,
    reason: []const u8,
    now_ms: i64,
) !bool {
    return updateProposalDecision(
        conn,
        proposal_id,
        agent_id,
        shared.ApprovalMode.auto.label(),
        shared.STATUS_VETO_WINDOW,
        shared.STATUS_VETOED,
        reason,
        now_ms,
    );
}

fn proposalSummaryFromRow(alloc: std.mem.Allocator, row: anytype) !ProposalSummary {
    return .{
        .proposal_id = try alloc.dupe(u8, try row.get([]const u8, 0)),
        .trigger_reason = try alloc.dupe(u8, try row.get([]const u8, 1)),
        .proposed_changes = try alloc.dupe(u8, try row.get([]const u8, 2)),
        .config_version_id = try alloc.dupe(u8, try row.get([]const u8, 3)),
        .approval_mode = try alloc.dupe(u8, try row.get([]const u8, 4)),
        .status = try alloc.dupe(u8, try row.get([]const u8, 5)),
        .auto_apply_at = try row.get(?i64, 6),
        .created_at = try row.get(i64, 7),
        .updated_at = try row.get(i64, 8),
    };
}

fn updateProposalDecision(
    conn: *pg.Conn,
    proposal_id: []const u8,
    agent_id: []const u8,
    approval_mode: []const u8,
    expected_status: []const u8,
    next_status: []const u8,
    reason: []const u8,
    now_ms: i64,
) !bool {
    var q = try conn.query(
        \\UPDATE agent_improvement_proposals
        \\SET status = $3,
        \\    rejection_reason = $4,
        \\    updated_at = $5
        \\WHERE proposal_id = $1
        \\  AND agent_id = $2
        \\  AND approval_mode = $6
        \\  AND generation_status = $7
        \\  AND status = $8
        \\RETURNING proposal_id
    , .{
        proposal_id,
        agent_id,
        next_status,
        reason,
        now_ms,
        approval_mode,
        shared.GENERATION_STATUS_READY,
        expected_status,
    });

    const changed = (try q.next()) != null;
    if (changed) q.drain() catch {};
    q.deinit();
    return changed;
}

fn loadManualProposalForDecision(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    agent_id: []const u8,
    proposal_id: []const u8,
) !?shared.ProposalLookup {
    var q = try conn.query(
        \\SELECT proposal_id, agent_id, workspace_id, config_version_id, proposed_changes
        \\FROM agent_improvement_proposals
        \\WHERE proposal_id = $1
        \\  AND agent_id = $2
        \\  AND approval_mode = $3
        \\  AND generation_status = $4
        \\  AND status = $5
        \\LIMIT 1
    , .{
        proposal_id,
        agent_id,
        shared.ApprovalMode.manual.label(),
        shared.GENERATION_STATUS_READY,
        shared.STATUS_PENDING_REVIEW,
    });

    const row = (try q.next()) orelse {
        q.deinit();
        return null;
    };
    const result = shared.ProposalLookup{
        .proposal_id = try alloc.dupe(u8, try row.get([]const u8, 0)),
        .agent_id = try alloc.dupe(u8, try row.get([]const u8, 1)),
        .workspace_id = try alloc.dupe(u8, try row.get([]const u8, 2)),
        .config_version_id = try alloc.dupe(u8, try row.get([]const u8, 3)),
        .proposed_changes = try alloc.dupe(u8, try row.get([]const u8, 4)),
    };
    q.drain() catch {};
    q.deinit();
    return result;
}

fn loadAppliedProposalFieldsChanged(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    proposal_id: []const u8,
) ![][]u8 {
    var q = try conn.query(
        \\SELECT field_name
        \\FROM harness_change_log
        \\WHERE proposal_id = $1
        \\ORDER BY applied_at ASC, change_id ASC
    , .{proposal_id});

    var fields: std.ArrayList([]u8) = .{};
    errdefer {
        for (fields.items) |field| alloc.free(field);
        fields.deinit(alloc);
    }

    while (try q.next()) |row| {
        try fields.append(alloc, try alloc.dupe(u8, try row.get([]const u8, 0)));
    }
    q.deinit();
    return try fields.toOwnedSlice(alloc);
}
