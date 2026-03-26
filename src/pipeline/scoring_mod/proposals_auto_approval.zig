const std = @import("std");
const pg = @import("pg");
const shared = @import("proposals_shared.zig");
const support = @import("proposals_apply_support.zig");
const validation = @import("proposals_validation.zig");

const log = std.log.scoped(.scoring);

pub const ApplyProposalResult = enum {
    applied,
    config_changed,
    rejected,
};

pub const AutoApprovalReconcileResult = shared.AutoApprovalReconcileResult;
pub const activateConfigVersion = support.activateConfigVersion;
pub const persistCandidateConfigVersion = support.persistCandidateConfigVersion;

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
    // check-pg-drain: ok — full while loop exhausts all rows, natural drain
    var result: AutoApprovalReconcileResult = .{};
    const batch_limit: i32 = @intCast(if (limit == 0) shared.DEFAULT_RECONCILE_BATCH_LIMIT else limit);
    var due_list: std.ArrayList(DueProposal) = .{};
    defer {
        for (due_list.items) |*proposal| proposal.deinit(alloc);
        due_list.deinit(alloc);
    }

    var q = try conn.query(
        \\SELECT proposal_id::text, agent_id::text, workspace_id::text, config_version_id::text, proposed_changes
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
        switch (try applyProposal(
            conn,
            alloc,
            proposal.agent_id,
            proposal.workspace_id,
            proposal.proposal_id,
            proposal.config_version_id,
            proposal.proposed_changes,
            shared.STATUS_VETO_WINDOW,
            shared.APPLIED_BY_SYSTEM_AUTO,
            now_ms,
        )) {
            .applied => result.applied += 1,
            .config_changed => result.config_changed += 1,
            .rejected => result.rejected += 1,
        }
    }

    result.expired = try support.expireStaleManualProposals(conn, now_ms - shared.MANUAL_PROPOSAL_EXPIRY_MS);
    log.info("scoring.auto_approval_reconcile applied={d} rejected={d} config_changed={d} expired={d}", .{ result.applied, result.rejected, result.config_changed, result.expired });
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
    if (proposal_id.len == 0) return .rejected;

    try beginTx(conn);
    var tx_open = true;
    errdefer if (tx_open) rollbackTx(conn);

    if (!try support.markProposalApprovedIfExpected(
        conn,
        agent_id,
        workspace_id,
        proposal_id,
        config_version_id,
        proposed_changes,
        required_status,
        now_ms,
    )) {
        rollbackTx(conn);
        tx_open = false;
        return .rejected;
    }

    const current_config_version_id = (try support.loadCurrentActiveConfigVersionId(conn, alloc, workspace_id)) orelse {
        try support.markProposalConfigChanged(conn, proposal_id, now_ms);
        try commitTx(conn);
        tx_open = false;
        return .config_changed;
    };
    defer alloc.free(current_config_version_id);

    if (!std.mem.eql(u8, current_config_version_id, config_version_id)) {
        try support.markProposalConfigChanged(conn, proposal_id, now_ms);
        try commitTx(conn);
        tx_open = false;
        return .config_changed;
    }

    if (!try support.ensureProposalApproved(conn, proposal_id)) {
        rollbackTx(conn);
        tx_open = false;
        return .rejected;
    }

    const candidate_profile_json = validation.buildCandidateProfileJson(
        conn,
        alloc,
        config_version_id,
        proposed_changes,
    ) catch {
        try support.rejectProposal(conn, proposal_id, shared.REJECTION_REASON_COMPILE_FAILED, now_ms);
        try commitTx(conn);
        tx_open = false;
        return .rejected;
    };
    defer alloc.free(candidate_profile_json);

    const change_log_entries = support.collectChangeLogEntries(alloc, proposed_changes) catch {
        try support.rejectProposal(conn, proposal_id, shared.REJECTION_REASON_COMPILE_FAILED, now_ms);
        try commitTx(conn);
        tx_open = false;
        return .rejected;
    };
    defer {
        for (change_log_entries) |*entry| entry.deinit(alloc);
        alloc.free(change_log_entries);
    }

    const activated_config_version_id = support.persistCandidateConfigVersion(conn, alloc, agent_id, candidate_profile_json, now_ms) catch |err| switch (err) {
        support.ProposalAutoApprovalError.MissingConfigVersionContext => {
            rollbackTx(conn);
            tx_open = false;
            try support.rejectProposal(conn, proposal_id, shared.REJECTION_REASON_ACTIVATE_FAILED, now_ms);
            return .rejected;
        },
        else => return err,
    };
    defer alloc.free(activated_config_version_id);

    support.activateAppliedProposal(conn, agent_id, workspace_id, proposal_id, activated_config_version_id, applied_by, now_ms) catch |err| switch (err) {
        support.ProposalAutoApprovalError.ActivateTargetMissing => {
            rollbackTx(conn);
            tx_open = false;
            try support.rejectProposal(conn, proposal_id, shared.REJECTION_REASON_ACTIVATE_FAILED, now_ms);
            return .rejected;
        },
        support.ProposalAutoApprovalError.ProposalNotApproved => {
            rollbackTx(conn);
            tx_open = false;
            return .rejected;
        },
        else => return err,
    };
    try support.insertHarnessChangeLog(conn, alloc, agent_id, proposal_id, workspace_id, change_log_entries, applied_by, now_ms);

    try commitTx(conn);
    tx_open = false;
    log.info("scoring.proposal_applied proposal_id={s} agent_id={s}", .{ proposal_id, agent_id });
    return .applied;
}
