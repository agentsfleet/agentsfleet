//! Agent run quality scoring engine (M9_001).

const std = @import("std");
const pg = @import("pg");
const posthog = @import("posthog");
const posthog_events = @import("../observability/posthog_events.zig");
const metrics = @import("../observability/metrics.zig");

const log = std.log.scoped(.scoring);
const types = @import("scoring_mod/types.zig");
const math = @import("scoring_mod/math.zig");
const persistence = @import("scoring_mod/persistence.zig");

pub const SCORE_FORMULA_VERSION = types.SCORE_FORMULA_VERSION;
pub const Tier = types.Tier;
pub const TerminalOutcome = types.TerminalOutcome;
pub const AxisScores = types.AxisScores;
pub const Weights = types.Weights;
pub const DEFAULT_WEIGHTS = types.DEFAULT_WEIGHTS;
pub const LatencyBaseline = types.LatencyBaseline;
pub const ScoringState = types.ScoringState;
pub const ScoringConfig = types.ScoringConfig;

pub const computeCompletionScore = math.computeCompletionScore;
pub const computeErrorRateScore = math.computeErrorRateScore;
pub const computeLatencyScore = math.computeLatencyScore;
pub const computeResourceScore = math.computeResourceScore;
pub const computeScore = math.computeScore;
pub const tierFromScore = math.tierFromScore;
pub const tierFromRun = math.tierFromRun;
pub const parseWeightsJson = math.parseWeightsJson;
pub const axisScoresJson = math.axisScoresJson;
pub const weightsJson = math.weightsJson;

pub const queryLatencyBaseline = persistence.queryLatencyBaseline;
pub const updateLatencyBaseline = persistence.updateLatencyBaseline;
pub const queryScoringConfig = persistence.queryScoringConfig;
pub const buildScoringContextForEcho = persistence.buildScoringContextForEcho;
pub const estimateScoringContextTokens = persistence.estimateScoringContextTokens;
pub const persistRunAnalysis = persistence.persistRunAnalysis;
pub const orientationContext = persistence.orientationContext;

pub fn scoreRunIfTerminal(
    conn: *pg.Conn,
    posthog_client: ?*posthog.PostHogClient,
    run_id: []const u8,
    workspace_id: []const u8,
    agent_id: []const u8,
    requested_by: []const u8,
    state: *const ScoringState,
    total_wall_seconds: u64,
) void {
    if (state.outcome == .pending) return;

    const start_ns = std.time.nanoTimestamp();
    scoreRunInner(
        conn,
        posthog_client,
        run_id,
        workspace_id,
        agent_id,
        requested_by,
        state,
        total_wall_seconds,
    ) catch |err| {
        log.warn("scoring failed (non-fatal) run_id={s} workspace_id={s} agent_id={s} err={s}", .{ run_id, workspace_id, agent_id, @errorName(err) });
        metrics.incAgentScoringFailed();
        posthog_events.trackAgentScoringFailed(
            posthog_client,
            posthog_events.distinctIdOrSystem(requested_by),
            run_id,
            workspace_id,
            @errorName(err),
        );
    };

    const elapsed_ns = std.time.nanoTimestamp() - start_ns;
    const elapsed_ms: u64 = @intCast(@max(@divFloor(elapsed_ns, 1_000_000), 0));
    metrics.observeAgentScoringDurationMs(elapsed_ms);
}

fn scoreRunInner(
    conn: *pg.Conn,
    posthog_client: ?*posthog.PostHogClient,
    run_id: []const u8,
    workspace_id: []const u8,
    agent_id: []const u8,
    requested_by: []const u8,
    s: *const ScoringState,
    total_wall_seconds: u64,
) !void {
    const config = try queryScoringConfig(conn, std.heap.page_allocator, workspace_id);
    if (!config.enabled) return;

    const baseline = queryLatencyBaseline(conn, workspace_id);
    const axes = AxisScores{
        .completion = computeCompletionScore(s.outcome),
        .error_rate = computeErrorRateScore(s.stages_passed, s.stages_total),
        .latency = computeLatencyScore(total_wall_seconds, baseline),
        .resource = computeResourceScore(),
    };

    const score = computeScore(axes, config.weights);
    const tier = tierFromRun(score, baseline);
    const scored_at = std.time.milliTimestamp();

    const axis_scores_json = try axisScoresJson(std.heap.page_allocator, axes);
    defer std.heap.page_allocator.free(axis_scores_json);
    const weight_snapshot_json = try weightsJson(std.heap.page_allocator, config.weights);
    defer std.heap.page_allocator.free(weight_snapshot_json);

    var persist_outcome = try persistence.persistScore(
        conn,
        std.heap.page_allocator,
        run_id,
        workspace_id,
        agent_id,
        score,
        axis_scores_json,
        weight_snapshot_json,
        scored_at,
        s,
        s.stages_passed,
        s.stages_total,
        total_wall_seconds,
    );
    defer persist_outcome.deinit(std.heap.page_allocator);

    if (persist_outcome.trust_update) |trust_update| {
        if (trust_update.earned()) {
            posthog_events.trackAgentTrustEarned(
                posthog_client,
                posthog_events.distinctIdOrSystem(requested_by),
                run_id,
                workspace_id,
                agent_id,
                trust_update.trust_streak_runs,
            );
        } else if (trust_update.lost()) {
            posthog_events.trackAgentTrustLost(
                posthog_client,
                posthog_events.distinctIdOrSystem(requested_by),
                run_id,
                workspace_id,
                agent_id,
                trust_update.trust_streak_runs,
            );
        }
    }

    if (persist_outcome.stalled_alert) |alert| {
        posthog_events.trackAgentImprovementStalled(
            posthog_client,
            posthog_events.distinctIdOrSystem(requested_by),
            run_id,
            workspace_id,
            agent_id,
            alert.proposal_id,
            3,
        );
    }

    log.info("run scored run_id={s} score={d} tier={s}", .{ run_id, score, tier.label() });

    posthog_events.trackAgentRunScored(
        posthog_client,
        posthog_events.distinctIdOrSystem(requested_by),
        run_id,
        workspace_id,
        agent_id,
        score,
        tier.label(),
        SCORE_FORMULA_VERSION,
        axis_scores_json,
        weight_snapshot_json,
        scored_at,
        axes.completion,
        axes.error_rate,
        axes.latency,
        axes.resource,
    );

    metrics.incAgentScoreComputed(tier);
    metrics.setAgentScoreLatest(score);
    updateLatencyBaseline(conn, workspace_id);
}

test {
    _ = @import("proposal_agent_team_test.zig");
    _ = @import("proposals_test.zig");
    _ = @import("scoring_test.zig");
}
