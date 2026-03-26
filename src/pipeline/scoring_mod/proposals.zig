const std = @import("std");
const pg = @import("pg");
const id_format = @import("../../types/id_format.zig");
const auto_approval = @import("proposals_auto_approval.zig");
const generation = @import("proposals_generation.zig");
const listing = @import("proposals_listing.zig");
const reporting = @import("proposals_reporting.zig");
const revert = @import("proposals_revert.zig");
const shared = @import("proposals_shared.zig");
const trigger = @import("proposals_trigger.zig");
const validation = @import("proposals_validation.zig");

const log = std.log.scoped(.scoring);

pub const AppliedProposalTelemetry = listing.AppliedProposalTelemetry;
pub const ApplyProposalResult = auto_approval.ApplyProposalResult;
pub const AutoApprovalReconcileResult = shared.AutoApprovalReconcileResult;
pub const GenerationReconcileResult = shared.GenerationReconcileResult;
pub const ImprovementReport = reporting.ImprovementReport;
pub const ImprovementStalledAlert = reporting.ImprovementStalledAlert;
pub const ManualProposalSummary = shared.ManualProposalSummary;
pub const ProposalSummary = shared.ProposalSummary;
pub const ProposalValidationError = shared.ProposalError;
pub const RevertHarnessResult = revert.RevertHarnessResult;

pub fn maybePersistTriggerProposal(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    agent_id: []const u8,
    scored_at: i64,
) !void {
    const rolling_trigger = try trigger.detectRollingWindowTrigger(conn, agent_id) orelse return;
    var active_context = (try trigger.loadActiveConfigContext(conn, alloc, workspace_id, agent_id)) orelse return;
    defer active_context.deinit(alloc);
    if (try trigger.hasPendingOrReadyProposal(conn, agent_id, active_context.config_version_id)) return;

    try validation.validateProposedChanges(
        conn,
        alloc,
        workspace_id,
        active_context.config_version_id,
        shared.ACTIVE_PROPOSAL_PLACEHOLDER,
    );

    const proposal_id = try id_format.generateTransitionId(alloc);
    defer alloc.free(proposal_id);

    const approval_mode = if (std.mem.eql(u8, active_context.trust_level, shared.TRUST_LEVEL_TRUSTED))
        shared.ApprovalMode.auto
    else
        shared.ApprovalMode.manual;
    const proposal_status = if (approval_mode == .auto) shared.STATUS_VETO_WINDOW else shared.STATUS_PENDING_REVIEW;
    const auto_apply_at: ?i64 = if (approval_mode == .auto) scored_at + shared.AUTO_APPLY_WINDOW_MS else null;

    _ = try conn.exec(
        \\INSERT INTO agent_improvement_proposals
        \\  (proposal_id, agent_id, workspace_id, trigger_reason, proposed_changes, config_version_id,
        \\   approval_mode, generation_status, status, auto_apply_at, created_at, updated_at)
        \\VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
    , .{
        proposal_id,
        agent_id,
        workspace_id,
        rolling_trigger.reason.label(),
        shared.ACTIVE_PROPOSAL_PLACEHOLDER,
        active_context.config_version_id,
        approval_mode.label(),
        shared.GENERATION_STATUS_PENDING,
        proposal_status,
        auto_apply_at,
        scored_at,
        scored_at,
    });
    log.info("scoring.proposal_created proposal_id={s} agent_id={s} approval_mode={s}", .{ proposal_id, agent_id, approval_mode.label() });
}

pub fn reconcilePendingProposalGenerations(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    limit: u32,
) !GenerationReconcileResult {
    // check-pg-drain: ok — full while loop exhausts all rows, natural drain
    var result: GenerationReconcileResult = .{};
    const batch_limit: i32 = @intCast(if (limit == 0) shared.DEFAULT_RECONCILE_BATCH_LIMIT else limit);

    var pending_list: std.ArrayList(shared.PendingProposal) = .{};
    defer {
        for (pending_list.items) |*pending| pending.deinit(alloc);
        pending_list.deinit(alloc);
    }

    var q = try conn.query(
        \\SELECT proposal_id::text, agent_id::text, workspace_id::text, config_version_id::text, trigger_reason
        \\FROM agent_improvement_proposals
        \\WHERE generation_status = $1
        \\ORDER BY created_at ASC, proposal_id ASC
        \\LIMIT $2
    , .{ shared.GENERATION_STATUS_PENDING, batch_limit });

    while (try q.next()) |row| {
        try pending_list.append(alloc, .{
            .proposal_id = try alloc.dupe(u8, try row.get([]const u8, 0)),
            .agent_id = try alloc.dupe(u8, try row.get([]const u8, 1)),
            .workspace_id = try alloc.dupe(u8, try row.get([]const u8, 2)),
            .config_version_id = try alloc.dupe(u8, try row.get([]const u8, 3)),
            .trigger_reason = try alloc.dupe(u8, try row.get([]const u8, 4)),
        });
    }
    q.deinit();

    for (pending_list.items) |*pending| {
        const generated_changes = generation.generateProposalChanges(
            conn,
            alloc,
            pending.agent_id,
            pending.config_version_id,
            pending.trigger_reason,
        ) catch |err| {
            try trigger.rejectGenerationProposal(conn, pending.proposal_id, shared.rejectionCodeForError(err));
            result.rejected += 1;
            continue;
        };
        defer alloc.free(generated_changes);

        validation.validateProposedChanges(
            conn,
            alloc,
            pending.workspace_id,
            pending.config_version_id,
            generated_changes,
        ) catch |err| {
            try trigger.rejectGenerationProposal(conn, pending.proposal_id, shared.rejectionCodeForError(err));
            result.rejected += 1;
            continue;
        };

        _ = try conn.exec(
            \\UPDATE agent_improvement_proposals
            \\SET proposed_changes = $2,
            \\    generation_status = $3,
            \\    updated_at = $4
            \\WHERE proposal_id = $1
        , .{
            pending.proposal_id,
            generated_changes,
            shared.GENERATION_STATUS_READY,
            std.time.milliTimestamp(),
        });
        result.ready += 1;
        log.info("scoring.proposal_generation_ready proposal_id={s} agent_id={s}", .{ pending.proposal_id, pending.agent_id });
    }

    return result;
}

pub fn validateProposedChanges(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    raw_json: []const u8,
) (ProposalValidationError || anyerror)!void {
    var active_context = (try trigger.loadActiveConfigContext(conn, alloc, workspace_id, "")) orelse {
        return ProposalValidationError.ProposalWouldNotCompile;
    };
    defer active_context.deinit(alloc);
    try validation.validateProposedChanges(
        conn,
        alloc,
        workspace_id,
        active_context.config_version_id,
        raw_json,
    );
}

pub fn reconcileDueAutoApprovalProposals(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    limit: u32,
    now_ms: i64,
) !AutoApprovalReconcileResult {
    return auto_approval.reconcileDueAutoApprovalProposals(conn, alloc, limit, now_ms);
}

pub fn loadAppliedProposalTelemetry(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    proposal_id: []const u8,
) !?AppliedProposalTelemetry {
    return listing.loadAppliedProposalTelemetry(conn, alloc, proposal_id);
}

pub fn listAppliedAutoProposalTelemetryAt(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    applied_at_ms: i64,
) ![]AppliedProposalTelemetry {
    return listing.listAppliedAutoProposalTelemetryAt(conn, alloc, applied_at_ms);
}

pub fn listOpenProposals(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    agent_id: []const u8,
    limit: u32,
) ![]ProposalSummary {
    return listing.listOpenProposals(conn, alloc, agent_id, limit);
}

pub fn listManualProposals(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    agent_id: []const u8,
    limit: u32,
) ![]ManualProposalSummary {
    return listing.listManualProposals(conn, alloc, agent_id, limit);
}

pub fn approveManualProposal(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    agent_id: []const u8,
    proposal_id: []const u8,
    operator_identity: []const u8,
    now_ms: i64,
) !?ApplyProposalResult {
    return listing.approveManualProposal(conn, alloc, agent_id, proposal_id, operator_identity, now_ms);
}

pub fn rejectManualProposal(
    conn: *pg.Conn,
    agent_id: []const u8,
    proposal_id: []const u8,
    reason: []const u8,
    now_ms: i64,
) !bool {
    return listing.rejectManualProposal(conn, agent_id, proposal_id, reason, now_ms);
}

pub fn vetoAutoProposal(
    conn: *pg.Conn,
    agent_id: []const u8,
    proposal_id: []const u8,
    reason: []const u8,
    now_ms: i64,
) !bool {
    return listing.vetoAutoProposal(conn, agent_id, proposal_id, reason, now_ms);
}

pub fn recordScoreAgainstImprovementWindow(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    run_id: []const u8,
    agent_id: []const u8,
    scored_at: i64,
) !?ImprovementStalledAlert {
    return reporting.recordScoreAgainstImprovementWindow(conn, alloc, run_id, agent_id, scored_at);
}

pub fn hasImprovementStalledWarning(conn: *pg.Conn, agent_id: []const u8) !bool {
    return reporting.hasImprovementStalledWarning(conn, agent_id);
}

pub fn loadImprovementReport(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    agent_id: []const u8,
) !?ImprovementReport {
    return reporting.loadImprovementReport(conn, alloc, agent_id);
}

pub fn revertHarnessChange(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    agent_id: []const u8,
    change_id: []const u8,
    operator_identity: []const u8,
    now_ms: i64,
) !?RevertHarnessResult {
    return revert.revertHarnessChange(conn, alloc, agent_id, change_id, operator_identity, now_ms);
}
