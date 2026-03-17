const std = @import("std");
const pg = @import("pg");
const id_format = @import("../../types/id_format.zig");
const shared = @import("proposals_shared.zig");
const validation = @import("proposals_validation.zig");

pub const AutoApprovalReconcileResult = shared.AutoApprovalReconcileResult;

const ProposalAutoApprovalError = error{
    MissingConfigVersionContext,
};

pub const ApplyProposalResult = enum {
    applied,
    config_changed,
    rejected,
};

const DueProposal = struct {
    proposal_id: []u8,
    agent_id: []u8,
    workspace_id: []u8,
    config_version_id: []u8,
    proposed_changes: []u8,

    fn deinit(self: *DueProposal, alloc: std.mem.Allocator) void {
        alloc.free(self.proposal_id);
        alloc.free(self.agent_id);
        alloc.free(self.workspace_id);
        alloc.free(self.config_version_id);
        alloc.free(self.proposed_changes);
    }
};

fn beginTx(conn: *pg.Conn) !void {
    _ = try conn.exec("BEGIN", .{});
}

fn commitTx(conn: *pg.Conn) !void {
    _ = try conn.exec("COMMIT", .{});
}

fn rollbackTx(conn: *pg.Conn) void {
    _ = conn.exec("ROLLBACK", .{}) catch return;
}

pub fn reconcileDueAutoApprovalProposals(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    limit: u32,
    now_ms: i64,
) !AutoApprovalReconcileResult {
    var result: AutoApprovalReconcileResult = .{};
    const batch_limit: i32 = @intCast(if (limit == 0) shared.DEFAULT_RECONCILE_BATCH_LIMIT else limit);
    var due_list: std.ArrayList(DueProposal) = .{};
    defer {
        for (due_list.items) |*proposal| proposal.deinit(alloc);
        due_list.deinit(alloc);
    }

    var q = try conn.query(
        \\SELECT proposal_id, agent_id, workspace_id, config_version_id, proposed_changes
        \\FROM agent_improvement_proposals
        \\WHERE generation_status = $1
        \\  AND approval_mode = $2
        \\  AND status = $3
        \\  AND auto_apply_at IS NOT NULL
        \\  AND auto_apply_at <= $4
        \\ORDER BY auto_apply_at ASC, proposal_id ASC
        \\LIMIT $5
    , .{
        shared.GENERATION_STATUS_READY,
        shared.ApprovalMode.auto.label(),
        shared.STATUS_VETO_WINDOW,
        now_ms,
        batch_limit,
    });

    while (try q.next()) |row| {
        try due_list.append(alloc, .{
            .proposal_id = try alloc.dupe(u8, try row.get([]const u8, 0)),
            .agent_id = try alloc.dupe(u8, try row.get([]const u8, 1)),
            .workspace_id = try alloc.dupe(u8, try row.get([]const u8, 2)),
            .config_version_id = try alloc.dupe(u8, try row.get([]const u8, 3)),
            .proposed_changes = try alloc.dupe(u8, try row.get([]const u8, 4)),
        });
    }
    q.deinit();

    for (due_list.items) |*proposal| {
        switch (try applyProposal(conn, alloc, proposal.agent_id, proposal.workspace_id, proposal.proposal_id, proposal.config_version_id, proposal.proposed_changes, shared.STATUS_VETO_WINDOW, shared.APPLIED_BY_SYSTEM_AUTO, now_ms)) {
            .applied => result.applied += 1,
            .config_changed => result.config_changed += 1,
            .rejected => result.rejected += 1,
        }
    }

    const expired = try expireStaleManualProposals(conn, now_ms - shared.MANUAL_PROPOSAL_EXPIRY_MS);
    result.expired = expired;

    return result;
}

pub fn applyProposal(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    agent_id: []const u8,
    workspace_id: []const u8,
    proposal_id: []const u8,
    config_version_id: []const u8,
    proposed_changes: []const u8,
    required_status: []const u8,
    applied_by: []const u8,
    now_ms: i64,
) !ApplyProposalResult {
    try beginTx(conn);
    var tx_open = true;
    errdefer if (tx_open) rollbackTx(conn);

    if (!try markProposalApprovedIfExpected(conn, proposal_id, required_status, now_ms)) {
        rollbackTx(conn);
        tx_open = false;
        return .rejected;
    }

    const current_config_version_id = (try loadCurrentActiveConfigVersionId(conn, alloc, workspace_id)) orelse {
        try markProposalConfigChanged(conn, proposal_id, now_ms);
        try commitTx(conn);
        tx_open = false;
        return .config_changed;
    };
    defer alloc.free(current_config_version_id);

    if (!std.mem.eql(u8, current_config_version_id, config_version_id)) {
        try markProposalConfigChanged(conn, proposal_id, now_ms);
        try commitTx(conn);
        tx_open = false;
        return .config_changed;
    }

    const candidate_profile_json = validation.buildCandidateProfileJson(
        conn,
        alloc,
        config_version_id,
        proposed_changes,
    ) catch {
        try rejectProposal(conn, proposal_id, shared.rejectionCodeForError(shared.ProposalError.ProposalWouldNotCompile), now_ms);
        try commitTx(conn);
        tx_open = false;
        return .rejected;
    };
    defer alloc.free(candidate_profile_json);

    const activated_config_version_id = try persistCandidateConfigVersion(conn, alloc, agent_id, candidate_profile_json, now_ms);
    defer alloc.free(activated_config_version_id);
    try activateAppliedProposal(conn, agent_id, workspace_id, proposal_id, activated_config_version_id, applied_by, now_ms);

    try commitTx(conn);
    tx_open = false;
    return .applied;
}

fn loadCurrentActiveConfigVersionId(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
) !?[]u8 {
    var q = try conn.query(
        \\SELECT config_version_id
        \\FROM workspace_active_config
        \\WHERE workspace_id = $1
        \\LIMIT 1
    , .{workspace_id});

    const row = (try q.next()) orelse {
        q.deinit();
        return null;
    };
    const result = try alloc.dupe(u8, try row.get([]const u8, 0));
    q.drain() catch {};
    q.deinit();
    return result;
}

fn persistCandidateConfigVersion(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    agent_id: []const u8,
    candidate_profile_json: []const u8,
    now_ms: i64,
) ![]const u8 {
    const config_version_id = try id_format.generateTransitionId(alloc);
    errdefer alloc.free(config_version_id);

    var current_q = try conn.query(
        \\SELECT tenant_id, COALESCE(MAX(version), 0)::INTEGER
        \\FROM agent_config_versions
        \\WHERE agent_id = $1
        \\GROUP BY tenant_id
        \\ORDER BY MAX(version) DESC
        \\LIMIT 1
    , .{agent_id});

    const row = (try current_q.next()) orelse {
        current_q.deinit();
        return ProposalAutoApprovalError.MissingConfigVersionContext;
    };
    const tenant_id = try alloc.dupe(u8, try row.get([]const u8, 0));
    defer alloc.free(tenant_id);
    const next_version = (try row.get(i32, 1)) + 1;
    current_q.drain() catch {};
    current_q.deinit();

    _ = try conn.exec(
        \\INSERT INTO agent_config_versions
        \\  (config_version_id, tenant_id, agent_id, version, source_markdown, compiled_profile_json,
        \\   compile_engine, validation_report_json, is_valid, created_at, updated_at)
        \\VALUES ($1, $2, $3, $4, $5, $6, $7, $8, TRUE, $9, $9)
    , .{
        config_version_id,
        tenant_id,
        agent_id,
        next_version,
        candidate_profile_json,
        candidate_profile_json,
        shared.COMPILE_ENGINE_DETERMINISTIC_V1,
        shared.VALIDATION_STATUS_AUTO_APPLIED_JSON,
        now_ms,
    });

    return config_version_id;
}

fn activateAppliedProposal(
    conn: *pg.Conn,
    agent_id: []const u8,
    workspace_id: []const u8,
    proposal_id: []const u8,
    activated_config_version_id: []const u8,
    applied_by: []const u8,
    now_ms: i64,
) !void {
    _ = try conn.exec(
        \\UPDATE workspace_active_config
        \\SET config_version_id = $2,
        \\    activated_by = $3,
        \\    activated_at = $4
        \\WHERE workspace_id = $1
    , .{
        workspace_id,
        activated_config_version_id,
        applied_by,
        now_ms,
    });

    _ = try conn.exec(
        \\UPDATE agent_profiles
        \\SET status = CASE WHEN agent_id = $1 THEN 'ACTIVE' ELSE status END,
        \\    updated_at = $2
        \\WHERE workspace_id = $3
    , .{ agent_id, now_ms, workspace_id });

    _ = try conn.exec(
        \\UPDATE agent_improvement_proposals
        \\SET status = $2,
        \\    applied_by = $3,
        \\    updated_at = $4
        \\WHERE proposal_id = $1
    , .{
        proposal_id,
        shared.STATUS_APPLIED,
        applied_by,
        now_ms,
    });
}

fn markProposalApprovedIfExpected(
    conn: *pg.Conn,
    proposal_id: []const u8,
    expected_status: []const u8,
    now_ms: i64,
) !bool {
    var q = try conn.query(
        \\UPDATE agent_improvement_proposals
        \\SET status = $2,
        \\    updated_at = $3
        \\WHERE proposal_id = $1
        \\  AND status = $4
        \\RETURNING proposal_id
    , .{
        proposal_id,
        shared.STATUS_APPROVED,
        now_ms,
        expected_status,
    });

    const changed = (try q.next()) != null;
    if (changed) q.drain() catch {};
    q.deinit();
    return changed;
}

fn markProposalConfigChanged(conn: *pg.Conn, proposal_id: []const u8, now_ms: i64) !void {
    _ = try conn.exec(
        \\UPDATE agent_improvement_proposals
        \\SET status = $2,
        \\    rejection_reason = $3,
        \\    updated_at = $4
        \\WHERE proposal_id = $1
    , .{
        proposal_id,
        shared.STATUS_CONFIG_CHANGED,
        shared.REJECTION_REASON_CONFIG_CHANGED_SINCE_PROPOSAL,
        now_ms,
    });
}

fn rejectProposal(conn: *pg.Conn, proposal_id: []const u8, rejection_reason: []const u8, now_ms: i64) !void {
    _ = try conn.exec(
        \\UPDATE agent_improvement_proposals
        \\SET status = $2,
        \\    rejection_reason = $3,
        \\    updated_at = $4
        \\WHERE proposal_id = $1
    , .{
        proposal_id,
        shared.STATUS_REJECTED,
        rejection_reason,
        now_ms,
    });
}

fn expireStaleManualProposals(conn: *pg.Conn, cutoff_ms: i64) !u32 {
    var q = try conn.query(
        \\UPDATE agent_improvement_proposals
        \\SET status = $1,
        \\    rejection_reason = $2,
        \\    updated_at = $3
        \\WHERE approval_mode = $4
        \\  AND generation_status = $5
        \\  AND status = $6
        \\  AND created_at <= $7
        \\RETURNING proposal_id
    , .{
        shared.STATUS_REJECTED,
        shared.REJECTION_REASON_EXPIRED,
        std.time.milliTimestamp(),
        shared.ApprovalMode.manual.label(),
        shared.GENERATION_STATUS_READY,
        shared.STATUS_PENDING_REVIEW,
        cutoff_ms,
    });

    var expired: u32 = 0;
    while (try q.next()) |_| expired += 1;
    q.deinit();
    return expired;
}
