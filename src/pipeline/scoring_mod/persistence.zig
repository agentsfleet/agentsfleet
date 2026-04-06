const std = @import("std");
const pg = @import("pg");
const obs_log = @import("../../observability/logging.zig");
const defaults = @import("../../types/defaults.zig");
const id_format = @import("../../types/id_format.zig");
const types = @import("types.zig");
const math = @import("math.zig");
const proposals = @import("proposals.zig");
const trust = @import("trust.zig");
const classify = @import("classify.zig");
const context = @import("context.zig");

const log = std.log.scoped(.scoring);

const MIN_SCORING_CONTEXT_MAX_TOKENS: u32 = 512;
const MAX_SCORING_CONTEXT_MAX_TOKENS: u32 = 8192;

pub const orientationContext = context.orientationContext;
pub const estimateScoringContextTokens = context.estimateScoringContextTokens;
pub const buildScoringContextForEcho = context.buildScoringContextForEcho;

pub fn queryLatencyBaseline(conn: *pg.Conn, workspace_id: []const u8) ?types.LatencyBaseline {
    var q = conn.query(
        \\SELECT p50_seconds, p95_seconds, sample_count
        \\FROM workspace_latency_baseline
        \\WHERE workspace_id = $1
    , .{workspace_id}) catch return null;
    defer q.deinit();

    const row = (q.next() catch return null) orelse return null;
    const result = types.LatencyBaseline{
        .p50_seconds = @intCast(@max(row.get(i64, 0) catch return null, 0)),
        .p95_seconds = @intCast(@max(row.get(i64, 1) catch return null, 0)),
        .sample_count = @intCast(@max(row.get(i32, 2) catch return null, 0)),
    };
    q.drain() catch {}; // drain CommandComplete + ReadyForQuery → state = .idle
    return result;
}

pub fn updateLatencyBaseline(conn: *pg.Conn, workspace_id: []const u8) void {
    const now_ms = std.time.milliTimestamp();
    _ = conn.exec(
        \\WITH completed_runs AS (
        \\    SELECT COALESCE(SUM(ul.agent_seconds), 0) AS total_seconds
        \\    FROM runs r
        \\    JOIN usage_ledger ul ON ul.run_id = r.run_id AND ul.source = 'runtime_stage'
        \\    WHERE r.workspace_id = $1
        \\      AND r.state IN ('DONE', 'NOTIFIED_BLOCKED')
        \\    GROUP BY r.run_id
        \\    ORDER BY r.updated_at DESC
        \\    LIMIT 50
        \\)
        \\INSERT INTO workspace_latency_baseline
        \\    (workspace_id, p50_seconds, p95_seconds, sample_count, computed_at)
        \\SELECT $1,
        \\       COALESCE(percentile_cont(0.50) WITHIN GROUP (ORDER BY total_seconds), 0)::BIGINT,
        \\       COALESCE(percentile_cont(0.95) WITHIN GROUP (ORDER BY total_seconds), 0)::BIGINT,
        \\       COUNT(*)::INTEGER,
        \\       $2
        \\FROM completed_runs
        \\ON CONFLICT (workspace_id) DO UPDATE
        \\SET p50_seconds = EXCLUDED.p50_seconds,
        \\    p95_seconds = EXCLUDED.p95_seconds,
        \\    sample_count = EXCLUDED.sample_count,
        \\    computed_at = EXCLUDED.computed_at
        \\WHERE EXCLUDED.sample_count > 0
    , .{ workspace_id, now_ms }) catch |err| {
        obs_log.logWarnErr(.scoring, err, "scoring.latency_baseline_fail workspace_id={s}", .{workspace_id});
    };
}

pub fn queryScoringConfig(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
) !types.ScoringConfig {
    var q = conn.query(
        "SELECT enable_agent_scoring, agent_scoring_weights_json, enable_score_context_injection, scoring_context_max_tokens FROM workspace_entitlements WHERE workspace_id = $1",
        .{workspace_id},
    ) catch |err| return err;
    defer q.deinit();

    const row = (try q.next()) orelse return .{};
    const enabled = try row.get(bool, 0);
    const raw_weights = try row.get([]const u8, 1);
    const enable_context = try row.get(bool, 2);
    const raw_context_max_tokens = try row.get(i32, 3);

    const result = types.ScoringConfig{
        .enabled = enabled,
        .weights = if (enabled) try math.parseWeightsJson(alloc, raw_weights) else types.DEFAULT_WEIGHTS,
        .enable_score_context_injection = enable_context,
        .scoring_context_max_tokens = clampScoringContextMaxTokens(raw_context_max_tokens),
    };
    q.drain() catch {}; // drain CommandComplete + ReadyForQuery → state = .idle
    return result;
}

fn clampScoringContextMaxTokens(raw_value: i32) u32 {
    if (raw_value <= 0) return defaults.DEFAULT_SCORING_CONTEXT_MAX_TOKENS;
    const normalized: u32 = @intCast(raw_value);
    return @min(MAX_SCORING_CONTEXT_MAX_TOKENS, @max(MIN_SCORING_CONTEXT_MAX_TOKENS, normalized));
}

fn persistScoreRecord(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    run_id: []const u8,
    workspace_id: []const u8,
    agent_id: []const u8,
    score: u8,
    axis_scores_json: []const u8,
    weight_snapshot_json: []const u8,
    scored_at: i64,
) !bool {
    var existing = try conn.query(
        "SELECT 1 FROM agent_run_scores WHERE run_id = $1",
        .{run_id},
    );
    defer existing.deinit();
    if (try existing.next() != null) {
        existing.drain() catch {}; // drain CommandComplete + ReadyForQuery → state = .idle
        return false;
    }

    const score_id = try id_format.generateTransitionId(alloc);
    defer alloc.free(score_id);

    _ = try conn.exec(
        \\INSERT INTO agent_run_scores
        \\  (score_id, run_id, agent_id, workspace_id, score, axis_scores, weight_snapshot, scored_at)
        \\VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
    , .{ score_id, run_id, agent_id, workspace_id, score, axis_scores_json, weight_snapshot_json, scored_at });
    log.info("scoring.score_persisted run_id={s} agent_id={s} score={d}", .{ run_id, agent_id, score });
    return true;
}

pub fn persistRunAnalysis(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    run_id: []const u8,
    workspace_id: []const u8,
    agent_id: []const u8,
    scoring_state: *const types.ScoringState,
    stages_passed: u32,
    stages_total: u32,
    total_wall_seconds: u64,
) !void {
    if (agent_id.len == 0) return;

    var existing = try conn.query("SELECT 1 FROM agent_run_analysis WHERE run_id = $1", .{run_id});
    defer existing.deinit();
    if (try existing.next() != null) {
        existing.drain() catch {}; // drain CommandComplete + ReadyForQuery → state = .idle
        return;
    }

    const maybe_class = classify.classifyFailure(scoring_state);

    var failure_signals: std.ArrayList([]const u8) = .{};
    defer failure_signals.deinit(alloc);

    var improvement_hints: std.ArrayList([]const u8) = .{};
    defer improvement_hints.deinit(alloc);

    if (maybe_class) |failure_class| {
        switch (failure_class) {
            .timeout => {
                try failure_signals.append(alloc, "RETRIES_EXHAUSTED");
                try improvement_hints.append(alloc, "Reduce stage scope or split large tasks to avoid retries/timeouts.");
            },
            .oom => {
                try failure_signals.append(alloc, "RESOURCE_LIMIT_EXCEEDED");
                try improvement_hints.append(alloc, "Reduce stage memory pressure or split the task into smaller steps.");
            },
            .bad_output_format => {
                try failure_signals.append(alloc, "VALIDATION_FAILED");
                try improvement_hints.append(alloc, "Enforce output schema and explicit verdict formatting before completing stage.");
            },
            .tool_call_failure => {
                try failure_signals.append(alloc, "TOOL_CALL_FAILED");
                try improvement_hints.append(alloc, "Validate tool inputs and handle file or shell failures before completing the stage.");
            },
            .context_overflow => {
                try failure_signals.append(alloc, "TOKEN_CONTEXT_EXCEEDED");
                try improvement_hints.append(alloc, "Compress prompts, trim run context, or reduce artifact payload size before retrying.");
            },
            .auth_failure => {
                try failure_signals.append(alloc, "AUTHENTICATION_FAILED");
                try improvement_hints.append(alloc, "Repair missing or expired credentials before re-running the harness.");
            },
            .unhandled_exception => {
                try failure_signals.append(alloc, "ERROR_PROPAGATION");
                try improvement_hints.append(alloc, "Wrap risky operations with deterministic error handling and retry-safe fallbacks.");
            },
            .unknown => {
                try failure_signals.append(alloc, "UNKNOWN_FAILURE_SIGNAL");
                try improvement_hints.append(alloc, "Capture richer stage diagnostics to classify unknown failures deterministically.");
            },
        }
    } else {
        try failure_signals.append(alloc, "RUN_COMPLETED");
        if (stages_total > 0 and stages_passed < stages_total) {
            try improvement_hints.append(alloc, "Increase stage pass consistency across retries to improve reliability.");
        } else if (total_wall_seconds > 300) {
            try improvement_hints.append(alloc, "Execution latency is high; reduce context size and simplify stage objectives.");
        } else {
            try improvement_hints.append(alloc, "Run completed cleanly; preserve current behavior and monitor for drift.");
        }
    }

    const failure_signals_json = try std.json.Stringify.valueAlloc(alloc, failure_signals.items, .{});
    defer alloc.free(failure_signals_json);
    const improvement_hints_json = try std.json.Stringify.valueAlloc(alloc, improvement_hints.items, .{});
    defer alloc.free(improvement_hints_json);
    const scrubbed_stderr_tail = try classify.scrubStderrTail(alloc, scoring_state.stderr_tail);
    defer if (scrubbed_stderr_tail) |value| alloc.free(value);

    const analysis_id = try id_format.generateTransitionId(alloc);
    defer alloc.free(analysis_id);

    _ = try conn.exec(
        \\INSERT INTO agent_run_analysis
        \\  (analysis_id, run_id, agent_id, workspace_id, failure_class, failure_is_infra,
        \\   failure_signals, improvement_hints, stderr_tail, analyzed_at)
        \\VALUES ($1, $2, $3, $4, $5, $6, $7::jsonb, $8::jsonb, $9, $10)
    , .{
        analysis_id,
        run_id,
        agent_id,
        workspace_id,
        if (maybe_class) |fc| fc.label() else null,
        if (maybe_class) |fc| fc.isInfra() else false,
        failure_signals_json,
        improvement_hints_json,
        scrubbed_stderr_tail,
        std.time.milliTimestamp(),
    });
}

pub fn persistScore(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    run_id: []const u8,
    workspace_id: []const u8,
    agent_id: []const u8,
    score: u8,
    axis_scores_json: []const u8,
    weight_snapshot_json: []const u8,
    scored_at: i64,
    scoring_state: *const types.ScoringState,
    stages_passed: u32,
    stages_total: u32,
    total_wall_seconds: u64,
) !PersistScoreOutcome {
    if (agent_id.len == 0) return .{};
    const inserted = try persistScoreRecord(conn, alloc, run_id, workspace_id, agent_id, score, axis_scores_json, weight_snapshot_json, scored_at);
    if (!inserted) return .{ .inserted = false };
    try persistRunAnalysis(conn, alloc, run_id, workspace_id, agent_id, scoring_state, stages_passed, stages_total, total_wall_seconds);
    const trust_update = try trust.refreshAgentTrustState(conn, agent_id, scored_at);
    try proposals.maybePersistTriggerProposal(conn, alloc, workspace_id, agent_id, scored_at);
    const stalled_alert = try proposals.recordScoreAgainstImprovementWindow(conn, alloc, run_id, agent_id, scored_at);
    return .{
        .trust_update = trust_update,
        .stalled_alert = stalled_alert,
        .inserted = true,
    };
}

pub const PersistScoreOutcome = struct {
    trust_update: ?trust.TrustUpdate = null,
    stalled_alert: ?proposals.ImprovementStalledAlert = null,
    /// True when the score row was freshly inserted; false when the run was already scored.
    inserted: bool = false,

    pub fn deinit(self: *PersistScoreOutcome, alloc: std.mem.Allocator) void {
        if (self.stalled_alert) |*alert| alert.deinit(alloc);
    }
};
