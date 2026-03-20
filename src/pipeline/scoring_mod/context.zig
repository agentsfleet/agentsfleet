const std = @import("std");
const pg = @import("pg");
const obs_log = @import("../../observability/logging.zig");
const classify = @import("classify.zig");
const types = @import("types.zig");

const ORIENTATION_BLOCK =
    "## Agent Performance Context (v1)\n" ++
    "You have no prior score history. Aim for clean terminal states, minimal resource use, and valid output format.";

const ScoringRow = struct {
    score: i32,
    failure_class: []const u8,
    top_hint: []const u8,
};

pub fn orientationContext(alloc: std.mem.Allocator) ![]const u8 {
    return alloc.dupe(u8, ORIENTATION_BLOCK);
}

pub fn estimateScoringContextTokens(content: []const u8) u32 {
    // Match NullClaw's current runtime compaction heuristic: (total_chars + 3) / 4.
    return @intCast((content.len + 3) / 4);
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

        try buf.appendSlice(alloc, "## Agent Performance Context (v1)\n" ++
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
                .{ run_idx, if (i == 0) " (latest)" else "", row.score, classify.scoreTierLabel(row.score), issue },
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
        if (estimateScoringContextTokens(candidate) <= max_tokens) return candidate;
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
    // check-pg-drain: ok — full while loop exhausts all rows, natural drain
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

// T1 — orientationContext returns the expected constant block
test "orientationContext returns orientation block content" {
    const alloc = std.testing.allocator;
    const ctx = try orientationContext(alloc);
    defer alloc.free(ctx);
    try std.testing.expect(std.mem.indexOf(u8, ctx, "Agent Performance Context") != null);
    try std.testing.expect(std.mem.indexOf(u8, ctx, "no prior score history") != null);
}

// T1 + T2 — estimateScoringContextTokens: formula (len+3)/4
test "estimateScoringContextTokens applies (len+3)/4 heuristic" {
    try std.testing.expectEqual(@as(u32, 0), estimateScoringContextTokens(""));
    try std.testing.expectEqual(@as(u32, 1), estimateScoringContextTokens("ab"));
    try std.testing.expectEqual(@as(u32, 1), estimateScoringContextTokens("abc"));
    try std.testing.expectEqual(@as(u32, 1), estimateScoringContextTokens("abcd"));
    try std.testing.expectEqual(@as(u32, 2), estimateScoringContextTokens("abcde"));
}

// T2 — buildScoringContextForEcho early-returns for empty agent_id
test "buildScoringContextForEcho returns empty string for empty agent_id" {
    const alloc = std.testing.allocator;
    // conn is never dereferenced on this early-return path; aligned address satisfies type system
    const conn: *pg.Conn = @ptrFromInt(@alignOf(pg.Conn));
    const config = types.ScoringConfig{ .enabled = true, .enable_score_context_injection = true };
    const result = try buildScoringContextForEcho(conn, alloc, "ws-001", "", config);
    defer alloc.free(result);
    try std.testing.expectEqualStrings("", result);
}

// T2 — buildScoringContextForEcho early-returns when scoring disabled
test "buildScoringContextForEcho returns empty string when scoring disabled" {
    const alloc = std.testing.allocator;
    const conn: *pg.Conn = @ptrFromInt(@alignOf(pg.Conn));
    const config_disabled = types.ScoringConfig{ .enabled = false };
    const r1 = try buildScoringContextForEcho(conn, alloc, "ws-001", "agent-abc", config_disabled);
    defer alloc.free(r1);
    try std.testing.expectEqualStrings("", r1);

    const config_no_inject = types.ScoringConfig{ .enabled = true, .enable_score_context_injection = false };
    const r2 = try buildScoringContextForEcho(conn, alloc, "ws-001", "agent-abc", config_no_inject);
    defer alloc.free(r2);
    try std.testing.expectEqualStrings("", r2);
}
