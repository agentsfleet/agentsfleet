pub const SCORE_FORMULA_VERSION = "1";

pub const Tier = enum {
    unranked,
    bronze,
    silver,
    gold,
    elite,

    pub fn label(self: Tier) []const u8 {
        return switch (self) {
            .unranked => "UNRANKED",
            .bronze => "BRONZE",
            .silver => "SILVER",
            .gold => "GOLD",
            .elite => "ELITE",
        };
    }
};

pub const TerminalOutcome = enum {
    pending,
    done,
    blocked_stage_graph,
    blocked_retries_exhausted,
    error_propagation,
};

pub const AxisScores = struct {
    completion: u8 = 0,
    error_rate: u8 = 0,
    latency: u8 = 0,
    resource: u8 = 0,
};

pub const Weights = struct {
    completion: f64 = 0.4,
    error_rate: f64 = 0.3,
    latency: f64 = 0.2,
    resource: f64 = 0.1,
};

pub const DEFAULT_WEIGHTS = Weights{};

pub const ScoringConfig = struct {
    enabled: bool = false,
    weights: Weights = DEFAULT_WEIGHTS,
};

pub const WeightsDoc = struct {
    completion: ?f64 = null,
    error_rate: ?f64 = null,
    latency: ?f64 = null,
    resource: ?f64 = null,
};

pub const LatencyBaseline = struct {
    p50_seconds: u64,
    p95_seconds: u64,
    sample_count: u32,
};

/// Mutable state accumulated during a run.
pub const ScoringState = struct {
    outcome: TerminalOutcome = .pending,
    stages_passed: u32 = 0,
    stages_total: u32 = 0,
};

pub fn hasPriorRuns(baseline: ?LatencyBaseline) bool {
    const bl = baseline orelse return false;
    return bl.sample_count > 0;
}
