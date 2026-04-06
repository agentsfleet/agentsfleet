const std = @import("std");
const pg = @import("pg");
const state = @import("../state/machine.zig");
const billing = @import("../state/billing.zig");
const worker_pr_flow = @import("worker_pr_flow.zig");
const worker_rate_limiter = @import("worker_rate_limiter.zig");
const posthog_events = @import("../observability/posthog_events.zig");
const events = @import("../events/bus.zig");
const obs_log = @import("../observability/logging.zig");
const metrics = @import("../observability/metrics.zig");
const git = @import("../git/ops.zig");
const scoring = @import("scoring.zig");
const types = @import("../types.zig");
const github_auth = @import("../auth/github.zig");
const worker_stage_helpers = @import("worker_stage_helpers.zig");
const worker_gate_loop = @import("worker_gate_loop.zig");
const codes = @import("../errors/codes.zig");
const wst = @import("worker_stage_types.zig");
const log = std.log.scoped(.worker);

pub const DoneOutcomeCtx = struct {
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    ctx: wst.RunContext,
    cfg: wst.ExecuteConfig,
    wt: *git.WorktreeHandle,
    branch: []const u8,
    running: *const std.atomic.Value(bool),
    deadline_ms: i64,
    token_cache: *github_auth.TokenCache,
    tenant_limiter: *worker_rate_limiter.TenantRateLimiter,
    final_stage_output: []const u8,
    final_stage_actor: types.Actor,
    attempt: u32,
    total_tokens: u64,
    total_wall_seconds: u64,
    scoring_state: *scoring.ScoringState,
    gate_results: ?[]const worker_gate_loop.GateToolResult = null,
    gate_loop_count: u32 = 0,
};

pub const BlockedOutcomeCtx = struct {
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    ctx: wst.RunContext,
    cfg: wst.ExecuteConfig,
    final_stage_output: []const u8,
    attempt: u32,
    total_wall_seconds: u64,
    scoring_state: *scoring.ScoringState,
};

/// Handles the .done terminal outcome. Caller must `return` after this.
pub fn handleDoneOutcome(o: DoneOutcomeCtx) !void {
    o.scoring_state.outcome = .done;

    // M27_002 §1.1: score synchronously before billing so the gate can check quality.
    // scoreRunForBillingGate sets the score row; the defer scoreRunIfTerminal in the executor
    // becomes a no-op for this run (PersistScoreOutcome.inserted = false on second call).
    const maybe_score = scoring.scoreRunForBillingGate(
        o.conn,
        o.cfg.posthog,
        o.ctx.run_id,
        o.ctx.workspace_id,
        o.ctx.agent_id,
        o.ctx.requested_by,
        o.scoring_state,
        o.total_wall_seconds,
    );
    const billing_outcome: billing.FinalizeOutcome = blk: {
        if (maybe_score) |score| {
            if (score < scoring.BILLING_QUALITY_THRESHOLD) {
                log.info("billing.score_gate run_id={s} agent_id={s} trace_id={s} score={d} threshold={d} outcome=non_billable", .{
                    o.ctx.run_id, o.ctx.agent_id, o.ctx.trace_id, score, scoring.BILLING_QUALITY_THRESHOLD,
                });
                posthog_events.trackRunBillingGated(
                    o.cfg.posthog,
                    posthog_events.distinctIdOrSystem(o.ctx.requested_by),
                    o.ctx.run_id,
                    o.ctx.workspace_id,
                    o.ctx.agent_id,
                    score,
                    scoring.BILLING_QUALITY_THRESHOLD,
                );
                break :blk .score_gated;
            }
        }
        break :blk .completed;
    };

    try billing.finalizeRunForBilling(
        o.alloc,
        o.conn,
        o.ctx.workspace_id,
        o.ctx.run_id,
        o.attempt,
        billing_outcome,
    );
    _ = try state.transition(o.conn, o.ctx.run_id, .PR_PREPARED, o.final_stage_actor, .VALIDATION_PASSED, null);

    const pr_final = try worker_pr_flow.ensurePrForRun(
        o.alloc,
        o.conn,
        o.token_cache,
        o.tenant_limiter,
        o.running,
        o.deadline_ms,
        o.wt.path,
        .{
            .run_id = o.ctx.run_id,
            .workspace_id = o.ctx.workspace_id,
            .tenant_id = o.ctx.tenant_id,
            .spec_id = o.ctx.spec_id,
            .repo_url = o.ctx.repo_url,
            .default_branch = o.ctx.default_branch,
            .branch = o.branch,
            .final_stage_output = o.final_stage_output,
        },
    );

    {
        const now_ms = std.time.milliTimestamp();
        _ = try o.conn.exec("UPDATE runs SET pr_url = $1, updated_at = $2 WHERE run_id = $3", .{ pr_final, now_ms, o.ctx.run_id });
    }

    // M16_001 §3.4: Post gate scorecard comment on the PR.
    if (o.gate_results) |results| {
        const scorecard = worker_gate_loop.formatScorecard(o.alloc, results, o.gate_loop_count, o.ctx.run_id) catch null;
        if (scorecard) |card| {
            defer o.alloc.free(card);
            const token = o.token_cache.getInstallationToken(o.alloc, o.ctx.workspace_id) catch null;
            if (token) |t| {
                defer o.alloc.free(t);
                git.postPrComment(o.alloc, o.ctx.repo_url, pr_final, t, card);
            }
        }
    }

    _ = try state.transition(o.conn, o.ctx.run_id, .PR_OPENED, .orchestrator, .PR_CREATED, pr_final);
    _ = try state.transition(o.conn, o.ctx.run_id, .NOTIFIED, .orchestrator, .NOTIFICATION_SENT, null);
    _ = try state.transition(o.conn, o.ctx.run_id, .DONE, .orchestrator, .NOTIFICATION_SENT, null);

    const summary_content = std.fmt.allocPrint(
        o.alloc,
        "# Run Summary\n\n" ++
            "- **run_id**: {s}\n" ++
            "- **spec_id**: {s}\n" ++
            "- **final_state**: DONE\n" ++
            "- **attempt**: {d}\n" ++
            "- **pr_url**: {s}\n" ++
            "- **total_tokens**: {d}\n" ++
            "- **total_wall_seconds**: {d}\n" ++
            "\n## Artifacts\n\n" ++
            "- plan.json (echo)\n" ++
            "- implementation.md (scout)\n" ++
            "- validation.md (warden)\n" ++
            "- run_summary.md (orchestrator)\n",
        .{ o.ctx.run_id, o.ctx.spec_id, o.attempt, pr_final, o.total_tokens, o.total_wall_seconds },
    ) catch |err| {
        obs_log.logWarnErr(.worker, err, "pipeline.run_summary_alloc_fail run_id={s}", .{o.ctx.run_id});
        try billing.finalizeRunForBilling(o.alloc, o.conn, o.ctx.workspace_id, o.ctx.run_id, o.attempt, billing_outcome);
        log.info("pipeline.run_completed run_id={s} pr_url={s}", .{ o.ctx.run_id, pr_final });
        var done_detail: [160]u8 = undefined;
        const done_detail_slice = std.fmt.bufPrint(&done_detail, "request_id={s} trace_id={s} state=done total_wall_seconds={d}", .{ o.ctx.request_id, o.ctx.trace_id, o.total_wall_seconds }) catch "run_done";
        events.emit("run_done", o.ctx.run_id, done_detail_slice);
        posthog_events.trackRunCompleted(o.cfg.posthog, posthog_events.distinctIdOrSystem(o.ctx.requested_by), o.ctx.run_id, o.ctx.workspace_id, "passed", o.total_wall_seconds * 1000);
        metrics.observeRunTotalWallSeconds(o.total_wall_seconds);
        metrics.incRunsCompleted();
        metrics.wsIncRunsCompleted(o.ctx.workspace_id);
        return;
    };

    const summary_path = try std.fmt.allocPrint(o.alloc, "docs/runs/{s}/run_summary.md", .{o.ctx.run_id});
    worker_stage_helpers.commitArtifact(o.alloc, o.conn, o.ctx, o.wt, o.running, o.deadline_ms, summary_path, summary_content, "orchestrator: add run_summary.md", .orchestrator, o.attempt) catch |err| {
        obs_log.logWarnErr(.worker, err, "pipeline.run_summary_commit_fail run_id={s}", .{o.ctx.run_id});
    };

    try billing.finalizeRunForBilling(o.alloc, o.conn, o.ctx.workspace_id, o.ctx.run_id, o.attempt, .completed);
    log.info("pipeline.run_completed run_id={s} pr_url={s}", .{ o.ctx.run_id, pr_final });
    var done_detail: [160]u8 = undefined;
    const done_detail_slice = std.fmt.bufPrint(&done_detail, "request_id={s} trace_id={s} state=done total_wall_seconds={d}", .{ o.ctx.request_id, o.ctx.trace_id, o.total_wall_seconds }) catch "run_done";
    events.emit("run_done", o.ctx.run_id, done_detail_slice);
    posthog_events.trackRunCompleted(o.cfg.posthog, posthog_events.distinctIdOrSystem(o.ctx.requested_by), o.ctx.run_id, o.ctx.workspace_id, "passed", o.total_wall_seconds * 1000);
    metrics.observeRunTotalWallSeconds(o.total_wall_seconds);
    // M28_001 §4.2: record gate loop distribution for runs that entered gate repair.
    if (o.gate_loop_count > 0) metrics.observeGateRepairLoopsPerRun(o.gate_loop_count);
    metrics.incRunsCompleted();
    metrics.wsIncRunsCompleted(o.ctx.workspace_id);
}

pub const RetriesExhaustedCtx = struct {
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    ctx: wst.RunContext,
    cfg: wst.ExecuteConfig,
    defects: ?[]const u8,
    attempt: u32,
    total_wall_seconds: u64,
    scoring_state: *scoring.ScoringState,
};

/// Handles the retries-exhausted terminal path. Caller must `return` after this.
pub fn handleRetriesExhaustedOutcome(o: RetriesExhaustedCtx) !void {
    o.scoring_state.outcome = .blocked_retries_exhausted;
    o.scoring_state.stderr_tail = o.defects orelse "";
    try billing.finalizeRunForBilling(o.alloc, o.conn, o.ctx.workspace_id, o.ctx.run_id, o.attempt, .non_billable);
    _ = try state.transition(o.conn, o.ctx.run_id, .BLOCKED, .orchestrator, .RETRIES_EXHAUSTED, null);
    _ = try state.transition(o.conn, o.ctx.run_id, .NOTIFIED_BLOCKED, .orchestrator, .NOTIFICATION_SENT, null);
    log.warn("pipeline.run_blocked reason=retries_exhausted run_id={s}", .{o.ctx.run_id});
    var blocked_detail: [176]u8 = undefined;
    const blocked_detail_slice = std.fmt.bufPrint(&blocked_detail, "request_id={s} trace_id={s} state=blocked reason=retries_exhausted total_wall_seconds={d}", .{ o.ctx.request_id, o.ctx.trace_id, o.total_wall_seconds }) catch "run_blocked";
    events.emit("run_blocked", o.ctx.run_id, blocked_detail_slice);
    posthog_events.trackRunFailed(o.cfg.posthog, posthog_events.distinctIdOrSystem(o.ctx.requested_by), o.ctx.run_id, o.ctx.workspace_id, "retries_exhausted", o.total_wall_seconds * 1000);
    metrics.observeRunTotalWallSeconds(o.total_wall_seconds);
    metrics.incRunsBlocked();
    metrics.wsIncRunsBlocked(o.ctx.workspace_id);
}

pub const GateExhaustedCtx = struct {
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    ctx: wst.RunContext,
    cfg: wst.ExecuteConfig,
    gate_results: []const worker_gate_loop.GateToolResult,
    total_repair_loops: u32,
    attempt: u32,
    total_wall_seconds: u64,
    scoring_state: *scoring.ScoringState,
};

/// Handles gate repair exhaustion. Caller must `return` after this.
pub fn handleGateExhaustedOutcome(o: GateExhaustedCtx) !void {
    o.scoring_state.outcome = .blocked_gate_exhausted;
    try billing.finalizeRunForBilling(o.alloc, o.conn, o.ctx.workspace_id, o.ctx.run_id, o.attempt, .non_billable);
    _ = try state.transition(o.conn, o.ctx.run_id, .BLOCKED, .orchestrator, .RETRIES_EXHAUSTED, "gate repair loops exhausted");
    _ = try state.transition(o.conn, o.ctx.run_id, .NOTIFIED_BLOCKED, .orchestrator, .NOTIFICATION_SENT, null);
    log.warn("pipeline.run_blocked error_code={s} reason=gate_exhausted run_id={s} loops={d}", .{
        codes.ERR_GATE_REPAIR_EXHAUSTED, o.ctx.run_id, o.total_repair_loops,
    });
    events.emit("run_blocked", o.ctx.run_id, "gate_repair_exhausted");
    posthog_events.trackRunFailed(
        o.cfg.posthog,
        posthog_events.distinctIdOrSystem(o.ctx.requested_by),
        o.ctx.run_id,
        o.ctx.workspace_id,
        "gate_exhausted",
        o.total_wall_seconds * 1000,
    );
    metrics.observeRunTotalWallSeconds(o.total_wall_seconds);
    // M28_001 §4.2: record gate loop distribution for exhausted runs.
    if (o.total_repair_loops > 0) metrics.observeGateRepairLoopsPerRun(o.total_repair_loops);
    metrics.incRunsBlocked();
    metrics.wsIncRunsBlocked(o.ctx.workspace_id);
}

/// Handles the .blocked terminal outcome. Caller must `return` after this.
pub fn handleBlockedOutcome(o: BlockedOutcomeCtx) !void {
    o.scoring_state.outcome = .blocked_stage_graph;
    o.scoring_state.stderr_tail = o.final_stage_output;
    try billing.finalizeRunForBilling(
        o.alloc,
        o.conn,
        o.ctx.workspace_id,
        o.ctx.run_id,
        o.attempt,
        .non_billable,
    );
    _ = try state.transition(o.conn, o.ctx.run_id, .BLOCKED, .orchestrator, .VALIDATION_FAILED, "blocked by stage transition graph");
    _ = try state.transition(o.conn, o.ctx.run_id, .NOTIFIED_BLOCKED, .orchestrator, .NOTIFICATION_SENT, null);
    posthog_events.trackRunFailed(
        o.cfg.posthog,
        posthog_events.distinctIdOrSystem(o.ctx.requested_by),
        o.ctx.run_id,
        o.ctx.workspace_id,
        "blocked",
        o.total_wall_seconds * 1000,
    );
    metrics.observeRunTotalWallSeconds(o.total_wall_seconds);
    metrics.incRunsBlocked();
    metrics.wsIncRunsBlocked(o.ctx.workspace_id);
}
