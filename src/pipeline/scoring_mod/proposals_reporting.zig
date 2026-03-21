const std = @import("std");
const pg = @import("pg");
const shared = @import("proposals_shared.zig");

const log = std.log.scoped(.scoring);

pub const ImprovementReport = shared.ImprovementReport;
pub const ImprovementStalledAlert = shared.ImprovementStalledAlert;

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

    if (score_delta >= 0 or !try hasThreeConsecutiveNegativeDeltas(conn, agent_id)) return null;

    _ = try conn.exec(
        \\UPDATE agent_profiles
        \\SET trust_level = $2,
        \\    trust_streak_runs = 0,
        \\    updated_at = $3
        \\WHERE agent_id = $1
    , .{ agent_id, shared.TRUST_LEVEL_UNEARNED, scored_at });

    return .{ .proposal_id = try alloc.dupe(u8, window.proposal_id) };
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

    log.info("improvement report generated agent_id={s} applied={d}", .{ agent_id, counts.applied });

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
    q.drain() catch {};
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
    q.drain() catch {};
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
    q.drain() catch {};
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
        if ((try row.get(f64, 0)) >= 0) {
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

    const row = (try q.next()) orelse return .{ .generated = 0, .approved = 0, .vetoed = 0, .rejected = 0, .applied = 0 };
    const result = ProposalCounts{
        .generated = @intCast(@max(try row.get(i64, 0), 0)),
        .approved = @intCast(@max(try row.get(i64, 1), 0)),
        .vetoed = @intCast(@max(try row.get(i64, 2), 0)),
        .rejected = @intCast(@max(try row.get(i64, 3), 0)),
        .applied = @intCast(@max(try row.get(i64, 4), 0)),
    };
    q.drain() catch {};
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
    q.drain() catch {};
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
    q.drain() catch {};
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
    q.drain() catch {};
    return avg_score;
}

fn scoreTierLabelForAverage(avg_score: ?f64) ?[]const u8 {
    const value = avg_score orelse return null;
    if (value >= 90.0) return "Elite";
    if (value >= 70.0) return "Gold";
    if (value >= 40.0) return "Silver";
    return "Bronze";
}
