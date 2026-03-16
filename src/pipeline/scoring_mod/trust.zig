const std = @import("std");
const pg = @import("pg");
const shared_types = @import("../../types.zig");

const TRUST_LEVEL_UNEARNED = shared_types.TrustLevel.unearned.label();
const TRUST_LEVEL_TRUSTED = shared_types.TrustLevel.trusted.label();
const TRUST_STREAK_THRESHOLD: i32 = 10;
const TRUST_SCORE_THRESHOLD: i32 = 70;
const FAILURE_CLASS_TIMEOUT = "TIMEOUT";
const FAILURE_CLASS_OOM = "OOM";
const FAILURE_CLASS_CONTEXT_OVERFLOW = "CONTEXT_OVERFLOW";
const FAILURE_CLASS_AUTH_FAILURE = "AUTH_FAILURE";

pub const TrustUpdate = struct {
    previous_level: shared_types.TrustLevel,
    current_level: shared_types.TrustLevel,
    trust_streak_runs: i32,

    pub fn earned(self: TrustUpdate) bool {
        return self.previous_level != .trusted and self.current_level == .trusted;
    }

    pub fn lost(self: TrustUpdate) bool {
        return self.previous_level == .trusted and self.current_level != .trusted;
    }

    pub fn currentLevelLabel(self: TrustUpdate) []const u8 {
        return self.current_level.label();
    }
};

const TrustHistoryRow = struct {
    score: i32,
    failure_is_infra: bool,
    failure_class: ?[]const u8,
};

const TrustSnapshot = struct {
    trust_streak_runs: i32,
    trust_level: shared_types.TrustLevel,
};

pub fn refreshAgentTrustState(conn: *pg.Conn, agent_id: []const u8, scored_at: i64) !?TrustUpdate {
    const previous = (try loadCurrentTrustState(conn, agent_id)) orelse return null;
    const next = try computeTrustState(conn, agent_id);

    var q = try conn.query(
        \\UPDATE agent_profiles
        \\SET trust_streak_runs = $2,
        \\    trust_level = $3,
        \\    last_scored_at = $4,
        \\    updated_at = $5
        \\WHERE agent_id = $1
    , .{ agent_id, next.trust_streak_runs, next.trust_level.label(), scored_at, scored_at });
    q.deinit();

    return .{
        .previous_level = previous.trust_level,
        .current_level = next.trust_level,
        .trust_streak_runs = next.trust_streak_runs,
    };
}

fn loadCurrentTrustState(conn: *pg.Conn, agent_id: []const u8) !?TrustSnapshot {
    var q = try conn.query(
        \\SELECT trust_streak_runs, trust_level
        \\FROM agent_profiles
        \\WHERE agent_id = $1
        \\LIMIT 1
    , .{agent_id});
    defer q.deinit();

    const row = (try q.next()) orelse return null;
    return .{
        .trust_streak_runs = try row.get(i32, 0),
        .trust_level = parseTrustLevel(try row.get([]const u8, 1)),
    };
}

fn computeTrustState(conn: *pg.Conn, agent_id: []const u8) !TrustSnapshot {
    var q = try conn.query(
        \\SELECT s.score, COALESCE(a.failure_is_infra, FALSE), a.failure_class
        \\FROM agent_run_scores s
        \\LEFT JOIN agent_run_analysis a ON a.run_id = s.run_id
        \\WHERE s.agent_id = $1
        \\ORDER BY s.scored_at DESC, s.score_id DESC
    , .{agent_id});
    defer q.deinit();

    var trust_streak_runs: i32 = 0;
    while (try q.next()) |row| {
        const history_row = TrustHistoryRow{
            .score = try row.get(i32, 0),
            .failure_is_infra = try row.get(bool, 1),
            .failure_class = try row.get(?[]const u8, 2),
        };
        if (isExcludedInfraFailure(history_row)) continue;
        if (history_row.score >= TRUST_SCORE_THRESHOLD) {
            trust_streak_runs += 1;
            continue;
        }
        trust_streak_runs = 0;
        break;
    }

    return .{
        .trust_streak_runs = trust_streak_runs,
        .trust_level = if (trust_streak_runs >= TRUST_STREAK_THRESHOLD) .trusted else .unearned,
    };
}

fn parseTrustLevel(raw: []const u8) shared_types.TrustLevel {
    if (std.mem.eql(u8, raw, TRUST_LEVEL_TRUSTED)) return .trusted;
    return .unearned;
}

fn isExcludedInfraFailure(row: TrustHistoryRow) bool {
    if (!row.failure_is_infra) return false;
    const failure_class = row.failure_class orelse return false;
    return std.mem.eql(u8, failure_class, FAILURE_CLASS_TIMEOUT) or
        std.mem.eql(u8, failure_class, FAILURE_CLASS_OOM) or
        std.mem.eql(u8, failure_class, FAILURE_CLASS_CONTEXT_OVERFLOW) or
        std.mem.eql(u8, failure_class, FAILURE_CLASS_AUTH_FAILURE);
}

test "unit: only explicit infra failures are excluded from trust streaks" {
    try std.testing.expect(isExcludedInfraFailure(.{
        .score = 12,
        .failure_is_infra = true,
        .failure_class = FAILURE_CLASS_TIMEOUT,
    }));
    try std.testing.expect(!isExcludedInfraFailure(.{
        .score = 12,
        .failure_is_infra = true,
        .failure_class = "UNKNOWN",
    }));
    try std.testing.expect(!isExcludedInfraFailure(.{
        .score = 12,
        .failure_is_infra = false,
        .failure_class = FAILURE_CLASS_TIMEOUT,
    }));
}
