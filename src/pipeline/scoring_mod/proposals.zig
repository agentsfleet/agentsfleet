const std = @import("std");
const pg = @import("pg");
const id_format = @import("../../types/id_format.zig");
const auto_approval = @import("proposals_auto_approval.zig");
const generation = @import("proposals_generation.zig");
const revert = @import("proposals_revert.zig");
const shared = @import("proposals_shared.zig");
const validation = @import("proposals_validation.zig");

pub const GenerationReconcileResult = shared.GenerationReconcileResult;
pub const AutoApprovalReconcileResult = shared.AutoApprovalReconcileResult;
pub const ProposalValidationError = shared.ProposalError;
pub const ManualProposalSummary = shared.ManualProposalSummary;
pub const ApplyProposalResult = auto_approval.ApplyProposalResult;
pub const AppliedProposalTelemetry = shared.AppliedProposalTelemetry;
pub const ImprovementReport = shared.ImprovementReport;
pub const ImprovementStalledAlert = shared.ImprovementStalledAlert;
pub const RevertHarnessResult = revert.RevertHarnessResult;

pub fn maybePersistTriggerProposal(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    agent_id: []const u8,
    scored_at: i64,
) !void {
    const trigger = try detectRollingWindowTrigger(conn, agent_id) orelse return;
    var active_context = (try loadActiveConfigContext(conn, alloc, workspace_id, agent_id)) orelse return;
    defer active_context.deinit(alloc);
    if (try hasPendingOrReadyProposal(conn, agent_id, active_context.config_version_id)) return;

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
    const proposal_status = if (approval_mode == .auto)
        shared.STATUS_VETO_WINDOW
    else
        shared.STATUS_PENDING_REVIEW;
    const auto_apply_at: ?i64 = if (approval_mode == .auto)
        scored_at + shared.AUTO_APPLY_WINDOW_MS
    else
        null;

    // Rule 1: exec() for INSERT — internal drain loop, always leaves _state=.idle
    _ = try conn.exec(
        \\INSERT INTO agent_improvement_proposals
        \\  (proposal_id, agent_id, workspace_id, trigger_reason, proposed_changes, config_version_id,
        \\   approval_mode, generation_status, status, auto_apply_at, created_at, updated_at)
        \\VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
    , .{
        proposal_id,
        agent_id,
        workspace_id,
        trigger.reason.label(),
        shared.ACTIVE_PROPOSAL_PLACEHOLDER,
        active_context.config_version_id,
        approval_mode.label(),
        shared.GENERATION_STATUS_PENDING,
        proposal_status,
        auto_apply_at,
        scored_at,
        scored_at,
    });
}

pub fn reconcilePendingProposalGenerations(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    limit: u32,
) !GenerationReconcileResult {
    var result: GenerationReconcileResult = .{};
    const batch_limit: i32 = @intCast(if (limit == 0) shared.DEFAULT_RECONCILE_BATCH_LIMIT else limit);

    // Rule: materialize all rows before closing the query, then process each
    // row with separate exec() calls. Running exec()/query() while an outer
    // SELECT is still open triggers ConnectionBusy on the same connection.
    var pending_list: std.ArrayList(shared.PendingProposal) = .{};
    defer {
        for (pending_list.items) |*p| p.deinit(alloc);
        pending_list.deinit(alloc);
    }

    var q = try conn.query(
        \\SELECT proposal_id, agent_id, workspace_id, config_version_id, trigger_reason
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
    // q.next() returning null drains 'C'+'Z' naturally; deinit is safe.
    q.deinit();

    for (pending_list.items) |*pending| {
        const generated_changes = generation.generateProposalChanges(
            conn,
            alloc,
            pending.agent_id,
            pending.config_version_id,
            pending.trigger_reason,
        ) catch |err| {
            try rejectProposal(conn, pending.proposal_id, shared.rejectionCodeForError(err));
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
            try rejectProposal(conn, pending.proposal_id, shared.rejectionCodeForError(err));
            result.rejected += 1;
            continue;
        };

        // Rule 1: exec() for UPDATE — internal drain loop, always leaves _state=.idle
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
    }

    return result;
}

pub fn validateProposedChanges(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    raw_json: []const u8,
) (ProposalValidationError || anyerror)!void {
    var active_context = (try loadActiveConfigContext(conn, alloc, workspace_id, "")) orelse {
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
    const copied_proposal_id = try alloc.dupe(u8, try row.get([]const u8, 0));
    const copied_agent_id = try alloc.dupe(u8, try row.get([]const u8, 1));
    const copied_workspace_id = try alloc.dupe(u8, try row.get([]const u8, 2));
    const copied_trigger_reason = try alloc.dupe(u8, try row.get([]const u8, 3));
    const copied_approval_mode = try alloc.dupe(u8, try row.get([]const u8, 4));
    q.drain() catch {};
    q.deinit();
    const telemetry = AppliedProposalTelemetry{
        .proposal_id = copied_proposal_id,
        .agent_id = copied_agent_id,
        .workspace_id = copied_workspace_id,
        .trigger_reason = copied_trigger_reason,
        .approval_mode = copied_approval_mode,
        .fields_changed = try loadAppliedProposalFieldsChanged(conn, alloc, proposal_id),
    };
    return telemetry;
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

pub fn recordScoreAgainstImprovementWindow(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    run_id: []const u8,
    agent_id: []const u8,
    scored_at: i64,
) !?ImprovementStalledAlert {
    var window = (try loadOpenImprovementWindow(conn, alloc, agent_id, scored_at)) orelse return null;
    defer window.deinit(alloc);

    _ = try conn.exec(
        \\UPDATE agent_run_scores
        \\SET proposal_id = $2
        \\WHERE run_id = $1
        \\  AND proposal_id IS NULL
    , .{ run_id, window.proposal_id });

    const tagged_count = try countTaggedWindowScores(conn, window.proposal_id);
    if (tagged_count < 5) return null;

    if (try hasProposalScoreDelta(conn, window.proposal_id)) return null;

    const baseline_avg = (try loadAverageScoreBeforeProposal(conn, agent_id, window.applied_at)) orelse return null;
    const post_change_avg = (try loadAverageScoreForProposal(conn, window.proposal_id)) orelse return null;
    const score_delta = post_change_avg - baseline_avg;

    _ = try conn.exec(
        \\UPDATE harness_change_log
        \\SET score_delta = $2
        \\WHERE proposal_id = $1
    , .{ window.proposal_id, score_delta });

    if (score_delta >= 0) return null;
    if (!try hasThreeConsecutiveNegativeDeltas(conn, agent_id)) return null;

    _ = try conn.exec(
        \\UPDATE agent_profiles
        \\SET trust_level = $2,
        \\    trust_streak_runs = 0,
        \\    updated_at = $3
        \\WHERE agent_id = $1
    , .{ agent_id, shared.TRUST_LEVEL_UNEARNED, scored_at });

    return .{
        .proposal_id = try alloc.dupe(u8, window.proposal_id),
    };
}

pub fn hasImprovementStalledWarning(conn: *pg.Conn, agent_id: []const u8) !bool {
    return hasThreeConsecutiveNegativeDeltas(conn, agent_id);
}

pub fn loadImprovementReport(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    agent_id: []const u8,
) !?ImprovementReport {
    var trust_row = (try loadAgentTrustLevel(conn, alloc, agent_id)) orelse return null;
    errdefer trust_row.deinit(alloc);

    const counts = try loadProposalCounts(conn, agent_id);
    const avg_score_delta = try loadAverageAppliedScoreDelta(conn, agent_id);
    const baseline_avg = try loadLatestCompletedBaselineAverage(conn, agent_id);
    const current_avg = try loadCurrentAverageScore(conn, agent_id);

    return .{
        .agent_id = trust_row.agent_id,
        .trust_level = trust_row.trust_level,
        .improvement_stalled_warning = try hasThreeConsecutiveNegativeDeltas(conn, agent_id),
        .proposals_generated = counts.generated,
        .proposals_approved = counts.approved,
        .proposals_vetoed = counts.vetoed,
        .proposals_rejected = counts.rejected,
        .proposals_applied = counts.applied,
        .avg_score_delta_per_applied_change = avg_score_delta,
        .current_tier = scoreTierLabelForAverage(current_avg),
        .baseline_tier = scoreTierLabelForAverage(baseline_avg),
    };
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
        \\SELECT proposal_id, trigger_reason, proposed_changes, config_version_id, created_at, updated_at
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
        try list.append(alloc, .{
            .proposal_id = try alloc.dupe(u8, try row.get([]const u8, 0)),
            .trigger_reason = try alloc.dupe(u8, try row.get([]const u8, 1)),
            .proposed_changes = try alloc.dupe(u8, try row.get([]const u8, 2)),
            .config_version_id = try alloc.dupe(u8, try row.get([]const u8, 3)),
            .created_at = try row.get(i64, 4),
            .updated_at = try row.get(i64, 5),
        });
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
        shared.STATUS_REJECTED,
        reason,
        now_ms,
        shared.ApprovalMode.manual.label(),
        shared.GENERATION_STATUS_READY,
        shared.STATUS_PENDING_REVIEW,
    });

    const changed = (try q.next()) != null;
    if (changed) q.drain() catch {};
    q.deinit();
    return changed;
}

fn detectRollingWindowTrigger(conn: *pg.Conn, agent_id: []const u8) !?shared.RollingTrigger {
    var q = try conn.query(
        \\SELECT score
        \\FROM agent_run_scores
        \\WHERE agent_id = $1
        \\ORDER BY scored_at DESC, score_id DESC
        \\LIMIT 10
    , .{agent_id});
    defer q.deinit();

    var scores: [10]i32 = undefined;
    var count: usize = 0;
    while (count < scores.len) {
        const row = (try q.next()) orelse break;
        scores[count] = try row.get(i32, 0);
        count += 1;
    }
    // If the loop exited because count == scores.len (not null from q.next()),
    // the server still has 'C'+'Z' pending; drain so _state=.idle.
    if (count == scores.len) q.drain() catch {};

    if (count < 5) return null;

    const current_sum = sumScores(scores[0..5]);
    if (current_sum < 300) {
        return .{ .reason = .sustained_low_score };
    }
    if (count < 10) return null;

    const previous_sum = sumScores(scores[5..10]);
    if (current_sum < previous_sum) {
        return .{ .reason = .declining_score };
    }
    return null;
}

fn sumScores(scores: []const i32) i32 {
    var total: i32 = 0;
    for (scores) |score| total += score;
    return total;
}

fn loadActiveConfigContext(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    agent_id: []const u8,
) !?shared.ActiveConfigContext {
    const sql = if (agent_id.len == 0)
        \\SELECT COALESCE(MAX(p.trust_level), 'UNEARNED'), active.config_version_id
        \\FROM workspace_active_config active
        \\LEFT JOIN agent_profiles p ON p.workspace_id = active.workspace_id
        \\WHERE active.workspace_id = $1
        \\GROUP BY active.config_version_id
        \\LIMIT 1
    else
        \\SELECT p.trust_level, active.config_version_id
        \\FROM agent_profiles p
        \\JOIN workspace_active_config active ON active.workspace_id = p.workspace_id
        \\WHERE p.workspace_id = $1 AND p.agent_id = $2
        \\LIMIT 1
    ;
    var q = if (agent_id.len == 0)
        try conn.query(sql, .{workspace_id})
    else
        try conn.query(sql, .{ workspace_id, agent_id });

    const row = (try q.next()) orelse {
        // q.next() returned null → it read 'C' then called readyForQuery() which
        // read 'Z' → _state=.idle. Safe to deinit directly.
        q.deinit();
        return null;
    };
    // Rule 4: copy values before draining (row buffer lives in the connection reader)
    const result = shared.ActiveConfigContext{
        .trust_level = try alloc.dupe(u8, try row.get([]const u8, 0)),
        .config_version_id = try alloc.dupe(u8, try row.get([]const u8, 1)),
    };
    // Rule 2: drain remaining 'C'+'Z' (LIMIT 1 but server may buffer differently)
    q.drain() catch {};
    q.deinit();
    return result;
}

fn hasPendingOrReadyProposal(conn: *pg.Conn, agent_id: []const u8, config_version_id: []const u8) !bool {
    var q = try conn.query(
        \\SELECT 1
        \\FROM agent_improvement_proposals
        \\WHERE agent_id = $1
        \\  AND config_version_id = $2
        \\  AND generation_status IN ($3, $4)
        \\LIMIT 1
    , .{ agent_id, config_version_id, shared.GENERATION_STATUS_PENDING, shared.GENERATION_STATUS_READY });

    const found = (try q.next()) != null;
    if (found) q.drain() catch {}; // Rule 3: drain remaining 'C'+'Z' when row found and breaking early
    q.deinit();
    return found;
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

const OpenImprovementWindow = struct {
    proposal_id: []u8,
    applied_at: i64,

    fn deinit(self: *OpenImprovementWindow, alloc: std.mem.Allocator) void {
        alloc.free(self.proposal_id);
    }
};

const AgentTrustRow = struct {
    agent_id: []u8,
    trust_level: []u8,

    fn deinit(self: *AgentTrustRow, alloc: std.mem.Allocator) void {
        alloc.free(self.agent_id);
        alloc.free(self.trust_level);
    }
};

const ProposalCounts = struct {
    generated: u32,
    approved: u32,
    vetoed: u32,
    rejected: u32,
    applied: u32,
};

fn loadOpenImprovementWindow(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    agent_id: []const u8,
    scored_at: i64,
) !?OpenImprovementWindow {
    var q = try conn.query(
        \\SELECT p.proposal_id, MAX(h.applied_at) AS applied_at
        \\FROM agent_improvement_proposals p
        \\JOIN harness_change_log h ON h.proposal_id = p.proposal_id
        \\LEFT JOIN agent_run_scores s ON s.proposal_id = p.proposal_id
        \\WHERE p.agent_id = $1
        \\  AND p.status = $2
        \\  AND p.updated_at <= $3
        \\GROUP BY p.proposal_id
        \\HAVING COUNT(s.score_id) < 5
        \\ORDER BY applied_at ASC, p.proposal_id ASC
        \\LIMIT 1
    , .{ agent_id, shared.STATUS_APPLIED, scored_at });

    const row = (try q.next()) orelse {
        q.deinit();
        return null;
    };
    const result = OpenImprovementWindow{
        .proposal_id = try alloc.dupe(u8, try row.get([]const u8, 0)),
        .applied_at = try row.get(i64, 1),
    };
    q.drain() catch {};
    q.deinit();
    return result;
}

fn countTaggedWindowScores(conn: *pg.Conn, proposal_id: []const u8) !u32 {
    var q = try conn.query(
        \\SELECT COUNT(*)
        \\FROM agent_run_scores
        \\WHERE proposal_id = $1
    , .{proposal_id});
    defer q.deinit();

    const row = (try q.next()) orelse return 0;
    const count: i64 = try row.get(i64, 0);
    _ = q.next() catch {};
    return @intCast(@max(count, 0));
}

fn hasProposalScoreDelta(conn: *pg.Conn, proposal_id: []const u8) !bool {
    var q = try conn.query(
        \\SELECT 1
        \\FROM harness_change_log
        \\WHERE proposal_id = $1
        \\  AND score_delta IS NOT NULL
        \\LIMIT 1
    , .{proposal_id});
    const found = (try q.next()) != null;
    if (found) q.drain() catch {};
    q.deinit();
    return found;
}

fn loadAverageScoreBeforeProposal(conn: *pg.Conn, agent_id: []const u8, applied_at: i64) !?f64 {
    var q = try conn.query(
        \\SELECT AVG(score)::DOUBLE PRECISION
        \\FROM (
        \\  SELECT score
        \\  FROM agent_run_scores
        \\  WHERE agent_id = $1
        \\    AND scored_at < $2
        \\  ORDER BY scored_at DESC, score_id DESC
        \\  LIMIT 5
        \\) prior_scores
    , .{ agent_id, applied_at });
    defer q.deinit();

    const row = (try q.next()) orelse return null;
    const avg_score = try row.get(?f64, 0);
    _ = q.next() catch {};
    return avg_score;
}

fn loadAverageScoreForProposal(conn: *pg.Conn, proposal_id: []const u8) !?f64 {
    var q = try conn.query(
        \\SELECT AVG(score)::DOUBLE PRECISION
        \\FROM agent_run_scores
        \\WHERE proposal_id = $1
    , .{proposal_id});
    defer q.deinit();

    const row = (try q.next()) orelse return null;
    const avg_score = try row.get(?f64, 0);
    _ = q.next() catch {};
    return avg_score;
}

fn hasThreeConsecutiveNegativeDeltas(conn: *pg.Conn, agent_id: []const u8) !bool {
    var q = try conn.query(
        \\SELECT proposal_delta.score_delta
        \\FROM (
        \\  SELECT proposal_id, MAX(applied_at) AS applied_at, MAX(score_delta) AS score_delta
        \\  FROM harness_change_log
        \\  WHERE agent_id = $1
        \\    AND score_delta IS NOT NULL
        \\  GROUP BY proposal_id
        \\) proposal_delta
        \\ORDER BY proposal_delta.applied_at DESC, proposal_delta.proposal_id DESC
        \\LIMIT 3
    , .{agent_id});
    defer q.deinit();

    var count: usize = 0;
    while (try q.next()) |row| {
        const delta = try row.get(f64, 0);
        if (delta >= 0) {
            q.drain() catch {};
            return false;
        }
        count += 1;
    }
    return count == 3;
}

fn loadAgentTrustLevel(conn: *pg.Conn, alloc: std.mem.Allocator, agent_id: []const u8) !?AgentTrustRow {
    var q = try conn.query(
        \\SELECT agent_id, trust_level
        \\FROM agent_profiles
        \\WHERE agent_id = $1
        \\LIMIT 1
    , .{agent_id});

    const row = (try q.next()) orelse {
        q.deinit();
        return null;
    };
    const result = AgentTrustRow{
        .agent_id = try alloc.dupe(u8, try row.get([]const u8, 0)),
        .trust_level = try alloc.dupe(u8, try row.get([]const u8, 1)),
    };
    q.drain() catch {};
    q.deinit();
    return result;
}

fn loadProposalCounts(conn: *pg.Conn, agent_id: []const u8) !ProposalCounts {
    var q = try conn.query(
        \\SELECT
        \\  COUNT(*)::BIGINT AS generated,
        \\  COUNT(*) FILTER (
        \\    WHERE approval_mode = $2
        \\      AND status = $3
        \\      AND applied_by LIKE 'operator:%'
        \\  )::BIGINT AS approved,
        \\  COUNT(*) FILTER (WHERE status = $4)::BIGINT AS vetoed,
        \\  COUNT(*) FILTER (WHERE status = $5)::BIGINT AS rejected,
        \\  COUNT(*) FILTER (WHERE status = $3)::BIGINT AS applied
        \\FROM agent_improvement_proposals
        \\WHERE agent_id = $1
    , .{
        agent_id,
        shared.ApprovalMode.manual.label(),
        shared.STATUS_APPLIED,
        shared.STATUS_VETOED,
        shared.STATUS_REJECTED,
    });
    defer q.deinit();

    const row = (try q.next()) orelse return .{
        .generated = 0,
        .approved = 0,
        .vetoed = 0,
        .rejected = 0,
        .applied = 0,
    };
    const result = ProposalCounts{
        .generated = @intCast(@max(try row.get(i64, 0), 0)),
        .approved = @intCast(@max(try row.get(i64, 1), 0)),
        .vetoed = @intCast(@max(try row.get(i64, 2), 0)),
        .rejected = @intCast(@max(try row.get(i64, 3), 0)),
        .applied = @intCast(@max(try row.get(i64, 4), 0)),
    };
    _ = q.next() catch {};
    return result;
}

fn loadAverageAppliedScoreDelta(conn: *pg.Conn, agent_id: []const u8) !?f64 {
    var q = try conn.query(
        \\SELECT AVG(score_delta)::DOUBLE PRECISION
        \\FROM (
        \\  SELECT MAX(score_delta) AS score_delta
        \\  FROM harness_change_log
        \\  WHERE agent_id = $1
        \\    AND score_delta IS NOT NULL
        \\  GROUP BY proposal_id
        \\) proposal_deltas
    , .{agent_id});
    defer q.deinit();

    const row = (try q.next()) orelse return null;
    const avg_delta = try row.get(?f64, 0);
    _ = q.next() catch {};
    return avg_delta;
}

fn loadLatestCompletedBaselineAverage(conn: *pg.Conn, agent_id: []const u8) !?f64 {
    var q = try conn.query(
        \\SELECT MAX(applied_at)
        \\FROM harness_change_log
        \\WHERE agent_id = $1
        \\  AND score_delta IS NOT NULL
    , .{agent_id});
    defer q.deinit();

    const row = (try q.next()) orelse return null;
    const applied_at = try row.get(?i64, 0);
    _ = q.next() catch {};
    if (applied_at == null) return null;
    return loadAverageScoreBeforeProposal(conn, agent_id, applied_at.?);
}

fn loadCurrentAverageScore(conn: *pg.Conn, agent_id: []const u8) !?f64 {
    var q = try conn.query(
        \\SELECT AVG(score)::DOUBLE PRECISION
        \\FROM (
        \\  SELECT score
        \\  FROM agent_run_scores
        \\  WHERE agent_id = $1
        \\  ORDER BY scored_at DESC, score_id DESC
        \\  LIMIT 5
        \\) recent_scores
    , .{agent_id});
    defer q.deinit();

    const row = (try q.next()) orelse return null;
    const avg_score = try row.get(?f64, 0);
    _ = q.next() catch {};
    return avg_score;
}

fn scoreTierLabelForAverage(avg_score: ?f64) ?[]const u8 {
    const value = avg_score orelse return null;
    if (value >= 90.0) return "Elite";
    if (value >= 70.0) return "Gold";
    if (value >= 40.0) return "Silver";
    return "Bronze";
}

fn rejectProposal(conn: *pg.Conn, proposal_id: []const u8, rejection_reason: []const u8) !void {
    // Rule 1: exec() for UPDATE — internal drain loop, always leaves _state=.idle
    _ = try conn.exec(
        \\UPDATE agent_improvement_proposals
        \\SET generation_status = $2,
        \\    status = $3,
        \\    rejection_reason = $4,
        \\    updated_at = $5
        \\WHERE proposal_id = $1
    , .{
        proposal_id,
        shared.GENERATION_STATUS_REJECTED,
        shared.STATUS_REJECTED,
        rejection_reason,
        std.time.milliTimestamp(),
    });
}
