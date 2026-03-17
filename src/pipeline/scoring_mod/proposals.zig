const std = @import("std");
const pg = @import("pg");
const id_format = @import("../../types/id_format.zig");
const generation = @import("proposals_generation.zig");
const shared = @import("proposals_shared.zig");
const validation = @import("proposals_validation.zig");

pub const GenerationReconcileResult = shared.GenerationReconcileResult;
pub const ProposalValidationError = shared.ProposalError;

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

    // Rule 1: exec() for INSERT — internal drain loop, always leaves _state=.idle
    _ = try conn.exec(
        \\INSERT INTO agent_improvement_proposals
        \\  (proposal_id, agent_id, workspace_id, trigger_reason, proposed_changes, config_version_id,
        \\   approval_mode, generation_status, status, auto_apply_at, created_at, updated_at)
        \\VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, NULL, $10, $11)
    , .{
        proposal_id,
        agent_id,
        workspace_id,
        trigger.reason.label(),
        shared.ACTIVE_PROPOSAL_PLACEHOLDER,
        active_context.config_version_id,
        approval_mode.label(),
        shared.GENERATION_STATUS_PENDING,
        shared.STATUS_PENDING_REVIEW,
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
