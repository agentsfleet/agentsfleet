const std = @import("std");
const pg = @import("pg");
const obs_log = @import("../../observability/logging.zig");
const id_format = @import("../../types/id_format.zig");
const types = @import("types.zig");
const math = @import("math.zig");

const DEFAULT_SCORING_CONTEXT_MAX_TOKENS: u32 = 2048;
const MIN_SCORING_CONTEXT_MAX_TOKENS: u32 = 512;
const MAX_SCORING_CONTEXT_MAX_TOKENS: u32 = 8192;
const TOKEN_ESTIMATE_DIVISOR: u32 = 4;
const STDERR_TAIL_MAX_LINES: usize = 200;
const ORIENTATION_BLOCK =
    "## Agent Performance Context (v1)\n" ++
    "You have no prior score history. Aim for clean terminal states, minimal resource use, and valid output format.";

pub fn orientationContext(alloc: std.mem.Allocator) ![]const u8 {
    return alloc.dupe(u8, ORIENTATION_BLOCK);
}

const FailureClass = enum {
    timeout,
    bad_output_format,
    unhandled_exception,
    unknown,

    fn label(self: FailureClass) []const u8 {
        return switch (self) {
            .timeout => "TIMEOUT",
            .bad_output_format => "BAD_OUTPUT_FORMAT",
            .unhandled_exception => "UNHANDLED_EXCEPTION",
            .unknown => "UNKNOWN",
        };
    }

    fn isInfra(self: FailureClass) bool {
        return switch (self) {
            .timeout => true,
            .bad_output_format, .unhandled_exception, .unknown => false,
        };
    }
};

const ScoringRow = struct {
    score: i32,
    failure_class: []const u8,
    top_hint: []const u8,
};

pub fn queryLatencyBaseline(conn: *pg.Conn, workspace_id: []const u8) ?types.LatencyBaseline {
    var q = conn.query(
        \\SELECT p50_seconds, p95_seconds, sample_count
        \\FROM workspace_latency_baseline
        \\WHERE workspace_id = $1
    , .{workspace_id}) catch return null;
    defer q.deinit();

    const row = (q.next() catch return null) orelse return null;
    return types.LatencyBaseline{
        .p50_seconds = @intCast(@max(row.get(i64, 0) catch return null, 0)),
        .p95_seconds = @intCast(@max(row.get(i64, 1) catch return null, 0)),
        .sample_count = @intCast(@max(row.get(i32, 2) catch return null, 0)),
    };
}

pub fn updateLatencyBaseline(conn: *pg.Conn, workspace_id: []const u8) void {
    const now_ms = std.time.milliTimestamp();
    var q = conn.query(
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
    , .{ workspace_id, now_ms }) catch |err| {
        obs_log.logWarnErr(.scoring, err, "latency baseline update failed workspace_id={s}", .{workspace_id});
        return;
    };
    q.deinit();
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

    return .{
        .enabled = enabled,
        .weights = if (enabled) try math.parseWeightsJson(alloc, raw_weights) else types.DEFAULT_WEIGHTS,
        .enable_score_context_injection = enable_context,
        .scoring_context_max_tokens = clampScoringContextMaxTokens(raw_context_max_tokens),
    };
}

fn clampScoringContextMaxTokens(raw_value: i32) u32 {
    if (raw_value <= 0) return DEFAULT_SCORING_CONTEXT_MAX_TOKENS;
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
    if (try existing.next() != null) return false;

    const score_id = try id_format.generateTransitionId(alloc);
    defer alloc.free(score_id);

    var q = try conn.query(
        \\INSERT INTO agent_run_scores
        \\  (score_id, run_id, agent_id, workspace_id, score, axis_scores, weight_snapshot, scored_at)
        \\VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
    , .{ score_id, run_id, agent_id, workspace_id, score, axis_scores_json, weight_snapshot_json, scored_at });
    q.deinit();
    return true;
}

fn classifyFailure(outcome: types.TerminalOutcome) ?FailureClass {
    return switch (outcome) {
        .done => null,
        .blocked_retries_exhausted => .timeout,
        .blocked_stage_graph => .bad_output_format,
        .error_propagation => .unhandled_exception,
        .pending => .unknown,
    };
}

fn scrubStderrTail(alloc: std.mem.Allocator, stderr_tail: []const u8) ![]const u8 {
    var out = try alloc.dupe(u8, stderr_tail);

    const redact_markers = [_][]const u8{
        "API_KEY=",
        "Bearer ",
        "DATABASE_URL=",
        "ENCRYPTION_MASTER_KEY=",
        "-----BEGIN",
    };

    for (redact_markers) |marker| {
        var idx: usize = 0;
        while (idx < out.len) {
            const found = std.mem.indexOfPos(u8, out, idx, marker) orelse break;
            var end = found + marker.len;
            while (end < out.len and out[end] != '\n' and out[end] != '\r' and out[end] != ' ') : (end += 1) {}
            @memset(out[found + marker.len .. end], '*');
            idx = end;
        }
    }

    return out;
}

fn scoreTierLabel(score: i32) []const u8 {
    if (score >= 90) return "Elite";
    if (score >= 70) return "Gold";
    if (score >= 40) return "Silver";
    return "Bronze";
}

fn approximateTokenCount(content: []const u8) u32 {
    // Align with NullClaw runtime compaction heuristic tokenEstimate:
    // estimated_tokens = (chars + 3) / 4
    return @intCast((content.len + (TOKEN_ESTIMATE_DIVISOR - 1)) / TOKEN_ESTIMATE_DIVISOR);
}

fn buildRunDiagnosticsTail(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    run_id: []const u8,
) ![]const u8 {
    var q = conn.query(
        \\SELECT reason_code, state_to, COALESCE(notes, '')
        \\FROM run_transitions
        \\WHERE run_id = $1
        \\ORDER BY ts DESC
        \\LIMIT $2
    , .{ run_id, @as(i32, @intCast(STDERR_TAIL_MAX_LINES)) }) catch |err| {
        obs_log.logWarnErr(.scoring, err, "run transition diagnostics query failed run_id={s}", .{run_id});
        return alloc.dupe(u8, "");
    };
    defer q.deinit();

    var buf: std.ArrayList(u8) = .{};
    defer buf.deinit(alloc);

    while (try q.next()) |row| {
        const reason_code = row.get([]const u8, 0) catch continue;
        const state_to = row.get([]const u8, 1) catch continue;
        const notes = row.get([]const u8, 2) catch "";
        if (notes.len > 0) {
            try buf.writer(alloc).print("[{s}] {s}: {s}\n", .{ reason_code, state_to, notes });
        } else {
            try buf.writer(alloc).print("[{s}] {s}\n", .{ reason_code, state_to });
        }
    }

    return buf.toOwnedSlice(alloc);
}

fn buildScoringContextBlock(
    alloc: std.mem.Allocator,
    rows: []const ScoringRow,
    max_tokens: u32,
) ![]const u8 {
    if (rows.len == 0) return alloc.dupe(u8, ORIENTATION_BLOCK);

    var keep_count = rows.len;
    while (keep_count > 0) {
        var buf: std.ArrayList(u8) = .{};
        defer buf.deinit(alloc);

        try buf.appendSlice(alloc,
            "## Agent Performance Context (v1)\n" ++
                "Your recent run history:\n" ++
                "| Run | Score | Tier | Issue |\n" ++
                "|-----|-------|------|-------|\n");

        var i: usize = 0;
        while (i < keep_count) : (i += 1) {
            const row = rows[i];
            const issue = if (row.failure_class.len == 0) "-" else row.failure_class;
            const run_idx = keep_count - i;
            try buf.writer(alloc).print(
                "| {d}{s} | {d} | {s} | {s} |\n",
                .{ run_idx, if (i == 0) " (latest)" else "", row.score, scoreTierLabel(row.score), issue },
            );
        }

        if (rows[0].top_hint.len > 0) {
            try buf.writer(alloc).print("Focus: {s}\n", .{rows[0].top_hint});
        } else if (rows[0].failure_class.len > 0) {
            try buf.writer(alloc).print("Focus: resolve recurring {s} failures.\n", .{rows[0].failure_class});
        } else {
            try buf.appendSlice(alloc, "Focus: maintain clean terminal states and stable runtime usage.\n");
        }

        const candidate = try buf.toOwnedSlice(alloc);
        if (approximateTokenCount(candidate) <= max_tokens) return candidate;
        alloc.free(candidate);

        keep_count -= 1;
    }

    return alloc.dupe(u8, ORIENTATION_BLOCK);
}

pub fn buildScoringContextForEcho(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    agent_id: []const u8,
    config: types.ScoringConfig,
) ![]const u8 {
    if (agent_id.len == 0) return alloc.dupe(u8, "");
    if (!config.enabled or !config.enable_score_context_injection) return alloc.dupe(u8, "");

    var q = conn.query(
        \\SELECT s.score,
        \\       COALESCE(a.failure_class, ''),
        \\       COALESCE(a.improvement_hints->>0, '')
        \\FROM agent_run_scores s
        \\LEFT JOIN agent_run_analysis a ON a.run_id = s.run_id
        \\WHERE s.workspace_id = $1 AND s.agent_id = $2
        \\ORDER BY s.score_id DESC
        \\LIMIT 5
    , .{ workspace_id, agent_id }) catch |err| {
        obs_log.logWarnErr(.scoring, err, "scoring context query failed workspace_id={s} agent_id={s}", .{ workspace_id, agent_id });
        return alloc.dupe(u8, ORIENTATION_BLOCK);
    };
    defer q.deinit();

    var rows: std.ArrayList(ScoringRow) = .{};
    defer rows.deinit(alloc);

    while (try q.next()) |row| {
        const score = try row.get(i32, 0);
        const failure_class = try alloc.dupe(u8, try row.get([]const u8, 1));
        const top_hint = try alloc.dupe(u8, try row.get([]const u8, 2));
        try rows.append(alloc, .{
            .score = score,
            .failure_class = failure_class,
            .top_hint = top_hint,
        });
    }
    defer {
        for (rows.items) |item| {
            alloc.free(item.failure_class);
            alloc.free(item.top_hint);
        }
    }

    return buildScoringContextBlock(alloc, rows.items, config.scoring_context_max_tokens);
}

pub fn persistRunAnalysis(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    run_id: []const u8,
    workspace_id: []const u8,
    agent_id: []const u8,
    outcome: types.TerminalOutcome,
    stages_passed: u32,
    stages_total: u32,
    total_wall_seconds: u64,
) !void {
    if (agent_id.len == 0) return;

    var existing = try conn.query("SELECT 1 FROM agent_run_analysis WHERE run_id = $1", .{run_id});
    defer existing.deinit();
    if (try existing.next() != null) return;

    const maybe_class = classifyFailure(outcome);

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
            .bad_output_format => {
                try failure_signals.append(alloc, "VALIDATION_FAILED");
                try improvement_hints.append(alloc, "Enforce output schema and explicit verdict formatting before completing stage.");
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

    const raw_stderr_tail = try buildRunDiagnosticsTail(conn, alloc, run_id);
    defer alloc.free(raw_stderr_tail);
    const stderr_tail = try scrubStderrTail(alloc, raw_stderr_tail);
    defer alloc.free(stderr_tail);

    const analysis_id = try id_format.generateTransitionId(alloc);
    defer alloc.free(analysis_id);

    var q = try conn.query(
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
        stderr_tail,
        std.time.milliTimestamp(),
    });
    q.deinit();
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
    outcome: types.TerminalOutcome,
    stages_passed: u32,
    stages_total: u32,
    total_wall_seconds: u64,
) !void {
    if (agent_id.len == 0) return;
    _ = try persistScoreRecord(conn, alloc, run_id, workspace_id, agent_id, score, axis_scores_json, weight_snapshot_json, scored_at);
    try persistRunAnalysis(conn, alloc, run_id, workspace_id, agent_id, outcome, stages_passed, stages_total, total_wall_seconds);
}
