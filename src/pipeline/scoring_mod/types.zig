pub const SCORE_FORMULA_VERSION = "2";

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
    blocked_gate_exhausted,
    error_propagation,
    /// M17_001 §1.2: run cancelled by operator signal or resource limit.
    cancelled,
};

pub const FailureClass = enum {
    timeout,
    oom,
    unhandled_exception,
    bad_output_format,
    tool_call_failure,
    context_overflow,
    auth_failure,
    unknown,

    pub fn label(self: FailureClass) []const u8 {
        return switch (self) {
            .timeout => "TIMEOUT",
            .oom => "OOM",
            .unhandled_exception => "UNHANDLED_EXCEPTION",
            .bad_output_format => "BAD_OUTPUT_FORMAT",
            .tool_call_failure => "TOOL_CALL_FAILURE",
            .context_overflow => "CONTEXT_OVERFLOW",
            .auth_failure => "AUTH_FAILURE",
            .unknown => "UNKNOWN",
        };
    }

    pub fn isInfra(self: FailureClass) bool {
        return switch (self) {
            .timeout, .oom, .context_overflow, .auth_failure => true,
            .unhandled_exception, .bad_output_format, .tool_call_failure, .unknown => false,
        };
    }
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
    enable_score_context_injection: bool = true,
    scoring_context_max_tokens: u32 = 2048,
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

/// M27_001: Resource metrics for scoring normalization.
pub const ResourceMetrics = struct {
    peak_memory_bytes: u64 = 0,
    memory_limit_bytes: u64 = 0,
    cpu_throttled_ms: u64 = 0,
    wall_ms: u64 = 0,

    /// Returns true if sufficient metrics exist for a real resource score.
    pub fn hasMetrics(self: ResourceMetrics) bool {
        return self.memory_limit_bytes > 0 and self.wall_ms > 0;
    }
};

/// Mutable state accumulated during a run.
pub const ScoringState = struct {
    outcome: TerminalOutcome = .pending,
    stages_passed: u32 = 0,
    stages_total: u32 = 0,
    failure_class_override: ?FailureClass = null,
    failure_error_name: ?[]const u8 = null,
    stderr_tail: ?[]const u8 = null,
    /// M27_001: resource metrics from executor for scoring.
    resource_metrics: ResourceMetrics = .{},
};

pub fn hasPriorRuns(baseline: ?LatencyBaseline) bool {
    const bl = baseline orelse return false;
    return bl.sample_count > 0;
}
