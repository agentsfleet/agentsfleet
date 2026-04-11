//! In-process metrics registry exposed in Prometheus text format.

const std = @import("std");
const me = @import("metrics_external.zig");

pub const DurationBuckets = [_]u64{ 1, 3, 5, 10, 30, 60, 120, 300 };
pub const HistogramSnapshot = struct {
    buckets: [DurationBuckets.len]u64 = [_]u64{0} ** DurationBuckets.len,
    count: u64 = 0,
    sum: u64 = 0,
};

const gate_hist = @import("metrics_gate_histogram.zig");
pub const GateLoopBuckets = gate_hist.GateLoopBuckets;
pub const GateLoopHistogramSnapshot = gate_hist.GateLoopHistogramSnapshot;

const interrupt_hist = @import("metrics_interrupt_histogram.zig");
pub const InterruptLatencyBuckets = interrupt_hist.InterruptLatencyBuckets;
pub const InterruptLatencySnapshot = interrupt_hist.InterruptLatencySnapshot;

pub const Snapshot = struct {
    // External retry/failure fields — defaults to 0, filled by me.snapshotExternalFields().
    external_retries_total: u64 = 0,
    external_retries_rate_limited_total: u64 = 0,
    external_retries_timeout_total: u64 = 0,
    external_retries_context_exhausted_total: u64 = 0,
    external_retries_auth_total: u64 = 0,
    external_retries_invalid_request_total: u64 = 0,
    external_retries_server_error_total: u64 = 0,
    external_retries_unknown_total: u64 = 0,
    external_failures_total: u64 = 0,
    external_failures_rate_limited_total: u64 = 0,
    external_failures_timeout_total: u64 = 0,
    external_failures_context_exhausted_total: u64 = 0,
    external_failures_auth_total: u64 = 0,
    external_failures_invalid_request_total: u64 = 0,
    external_failures_server_error_total: u64 = 0,
    external_failures_unknown_total: u64 = 0,
    retry_after_hints_total: u64 = 0,
    worker_errors_total: u64,
    sandbox_shell_runs_total: u64,
    sandbox_host_runs_total: u64,
    sandbox_bubblewrap_runs_total: u64,
    sandbox_kill_switch_total: u64,
    sandbox_preflight_failures_total: u64,
    agent_echo_calls_total: u64,
    agent_scout_calls_total: u64,
    agent_warden_calls_total: u64,
    agent_tokens_total: u64,
    agent_echo_tokens_total: u64,
    agent_scout_tokens_total: u64,
    agent_warden_tokens_total: u64,
    agent_orchestrator_tokens_total: u64,
    backoff_wait_ms_total: u64,
    rate_limit_wait_ms_total: u64,
    side_effect_outbox_enqueued_total: u64,
    side_effect_outbox_delivered_total: u64,
    side_effect_outbox_dead_letter_total: u64,
    api_backpressure_rejections_total: u64,
    api_in_flight_requests: u64,
    worker_allocator_leaks_total: u64,
    gate_repair_loops_total: u64,
    gate_repair_exhausted_total: u64,
    // M17_001 §1.3: per-limit-type termination counters
    run_limit_token_budget_exceeded_total: u64,
    run_limit_wall_time_exceeded_total: u64,
    run_limit_repair_loops_exhausted_total: u64,
    interrupt_queued_total: u64,
    interrupt_instant_total: u64,
    interrupt_fallback_total: u64,
    run_aborted_total: u64,
    otel_export_total: u64,
    otel_export_failed_total: u64,
    otel_last_success_at_ms: i64,
    orphan_runs_recovered_total: u64,
    orphan_no_agent_profile_total: u64,
    reconcile_running: u64,
    agent_duration_seconds: HistogramSnapshot,
    // M28_001 §4.1
    gate_repair_loops_per_run: GateLoopHistogramSnapshot,
    // M21_002 §4.3
    interrupt_delivery_latency_ms: InterruptLatencySnapshot,
};

var g_worker_errors_total = std.atomic.Value(u64).init(0);
var g_sandbox_shell_runs_total = std.atomic.Value(u64).init(0);
var g_sandbox_host_runs_total = std.atomic.Value(u64).init(0);
var g_sandbox_bubblewrap_runs_total = std.atomic.Value(u64).init(0);
var g_sandbox_kill_switch_total = std.atomic.Value(u64).init(0);
var g_sandbox_preflight_failures_total = std.atomic.Value(u64).init(0);
var g_agent_echo_calls_total = std.atomic.Value(u64).init(0);
var g_agent_scout_calls_total = std.atomic.Value(u64).init(0);
var g_agent_warden_calls_total = std.atomic.Value(u64).init(0);
var g_agent_tokens_total = std.atomic.Value(u64).init(0);
var g_agent_echo_tokens_total = std.atomic.Value(u64).init(0);
var g_agent_scout_tokens_total = std.atomic.Value(u64).init(0);
var g_agent_warden_tokens_total = std.atomic.Value(u64).init(0);
var g_agent_orchestrator_tokens_total = std.atomic.Value(u64).init(0);
var g_backoff_wait_ms_total = std.atomic.Value(u64).init(0);
var g_rate_limit_wait_ms_total = std.atomic.Value(u64).init(0);
var g_side_effect_outbox_enqueued_total = std.atomic.Value(u64).init(0);
var g_side_effect_outbox_delivered_total = std.atomic.Value(u64).init(0);
var g_side_effect_outbox_dead_letter_total = std.atomic.Value(u64).init(0);
var g_api_backpressure_rejections_total = std.atomic.Value(u64).init(0);
var g_api_in_flight_requests = std.atomic.Value(u64).init(0);
var g_worker_allocator_leaks_total = std.atomic.Value(u64).init(0);
var g_gate_repair_loops_total = std.atomic.Value(u64).init(0);
var g_gate_repair_exhausted_total = std.atomic.Value(u64).init(0);
var g_run_limit_token_budget_exceeded_total = std.atomic.Value(u64).init(0);
var g_run_limit_wall_time_exceeded_total = std.atomic.Value(u64).init(0);
var g_run_limit_repair_loops_exhausted_total = std.atomic.Value(u64).init(0);
var g_interrupt_queued_total = std.atomic.Value(u64).init(0);
var g_interrupt_instant_total = std.atomic.Value(u64).init(0);
var g_interrupt_fallback_total = std.atomic.Value(u64).init(0);
var g_run_aborted_total = std.atomic.Value(u64).init(0);
var g_otel_export_total = std.atomic.Value(u64).init(0);
var g_otel_export_failed_total = std.atomic.Value(u64).init(0);
var g_otel_last_success_at_ms = std.atomic.Value(i64).init(0);
var g_orphan_runs_recovered_total = std.atomic.Value(u64).init(0);
var g_orphan_no_agent_profile_total = std.atomic.Value(u64).init(0);
var g_reconcile_running = std.atomic.Value(u64).init(0);
const mh = @import("metrics_histograms.zig");
pub const incExternalRetry = me.incExternalRetry;
pub const incExternalFailure = me.incExternalFailure;
pub const incRetryAfterHintsApplied = me.incRetryAfterHintsApplied;

pub fn incWorkerErrors() void {
    _ = g_worker_errors_total.fetchAdd(1, .monotonic);
}

pub fn incSandboxShellRuns() void {
    _ = g_sandbox_shell_runs_total.fetchAdd(1, .monotonic);
}

pub fn incSandboxHostRuns() void {
    _ = g_sandbox_host_runs_total.fetchAdd(1, .monotonic);
}

pub fn incSandboxBubblewrapRuns() void {
    _ = g_sandbox_bubblewrap_runs_total.fetchAdd(1, .monotonic);
}

pub fn incSandboxKillSwitches() void {
    _ = g_sandbox_kill_switch_total.fetchAdd(1, .monotonic);
}

pub fn incSandboxPreflightFailures() void {
    _ = g_sandbox_preflight_failures_total.fetchAdd(1, .monotonic);
}

pub fn incAgentEchoCalls() void {
    _ = g_agent_echo_calls_total.fetchAdd(1, .monotonic);
}

pub fn incAgentScoutCalls() void {
    _ = g_agent_scout_calls_total.fetchAdd(1, .monotonic);
}

pub fn incAgentWardenCalls() void {
    _ = g_agent_warden_calls_total.fetchAdd(1, .monotonic);
}

pub fn addAgentTokens(tokens: u64) void {
    _ = g_agent_tokens_total.fetchAdd(tokens, .monotonic);
}

/// M27_002 obs: increment both global total and per-actor counter.
/// Use this in the worker pipeline where the actor is known.
pub fn addAgentTokensByActor(actor: @import("../types.zig").Actor, tokens: u64) void {
    _ = g_agent_tokens_total.fetchAdd(tokens, .monotonic);
    switch (actor) {
        .echo => _ = g_agent_echo_tokens_total.fetchAdd(tokens, .monotonic),
        .scout => _ = g_agent_scout_tokens_total.fetchAdd(tokens, .monotonic),
        .warden => _ = g_agent_warden_tokens_total.fetchAdd(tokens, .monotonic),
        .orchestrator => _ = g_agent_orchestrator_tokens_total.fetchAdd(tokens, .monotonic),
    }
}

pub fn addBackoffWaitMs(ms: u64) void {
    _ = g_backoff_wait_ms_total.fetchAdd(ms, .monotonic);
}

pub fn addRateLimitWaitMs(ms: u64) void {
    _ = g_rate_limit_wait_ms_total.fetchAdd(ms, .monotonic);
}

pub fn incOutboxEnqueued() void {
    _ = g_side_effect_outbox_enqueued_total.fetchAdd(1, .monotonic);
}

pub fn incOutboxDelivered() void {
    _ = g_side_effect_outbox_delivered_total.fetchAdd(1, .monotonic);
}

pub fn incOutboxDeadLetter() void {
    _ = g_side_effect_outbox_dead_letter_total.fetchAdd(1, .monotonic);
}

pub fn incApiBackpressureRejections() void {
    _ = g_api_backpressure_rejections_total.fetchAdd(1, .monotonic);
}

pub fn setApiInFlightRequests(v: u32) void {
    g_api_in_flight_requests.store(@as(u64, @intCast(v)), .release);
}

pub fn incWorkerAllocatorLeaks() void {
    _ = g_worker_allocator_leaks_total.fetchAdd(1, .monotonic);
}

pub fn incGateRepairLoops() void {
    _ = g_gate_repair_loops_total.fetchAdd(1, .monotonic);
}
pub fn incGateRepairExhausted() void {
    _ = g_gate_repair_exhausted_total.fetchAdd(1, .monotonic);
}
pub fn incRunLimitTokenBudgetExceeded() void {
    _ = g_run_limit_token_budget_exceeded_total.fetchAdd(1, .monotonic);
}
pub fn incRunLimitWallTimeExceeded() void {
    _ = g_run_limit_wall_time_exceeded_total.fetchAdd(1, .monotonic);
}
pub fn incRunLimitRepairLoopsExhausted() void {
    _ = g_run_limit_repair_loops_exhausted_total.fetchAdd(1, .monotonic);
}
pub fn incInterruptQueued() void {
    _ = g_interrupt_queued_total.fetchAdd(1, .monotonic);
}
pub fn incInterruptInstant() void {
    _ = g_interrupt_instant_total.fetchAdd(1, .monotonic);
}
pub fn incInterruptFallback() void {
    _ = g_interrupt_fallback_total.fetchAdd(1, .monotonic);
}
pub fn incRunAborted() void {
    _ = g_run_aborted_total.fetchAdd(1, .monotonic);
}
pub fn incOtelExportTotal() void {
    _ = g_otel_export_total.fetchAdd(1, .monotonic);
}
pub fn incOtelExportFailed() void {
    _ = g_otel_export_failed_total.fetchAdd(1, .monotonic);
}
pub fn setOtelLastSuccessAtMs(ms: i64) void {
    g_otel_last_success_at_ms.store(ms, .release);
}

pub fn incOrphanRunsRecovered() void {
    _ = g_orphan_runs_recovered_total.fetchAdd(1, .monotonic);
}

pub fn incOrphanNoAgentProfile() void {
    _ = g_orphan_no_agent_profile_total.fetchAdd(1, .monotonic);
}

pub fn setReconcileRunning(v: bool) void {
    g_reconcile_running.store(if (v) 1 else 0, .release);
}

pub const observeAgentDurationSeconds = mh.observeAgentDurationSeconds;
pub const observeGateRepairLoopsPerRun = mh.observeGateRepairLoopsPerRun;
pub const observeInterruptDeliveryLatencyMs = mh.observeInterruptDeliveryLatencyMs;

pub fn snapshot() Snapshot {
    var s = Snapshot{
        .worker_errors_total = g_worker_errors_total.load(.acquire),
        .sandbox_shell_runs_total = g_sandbox_shell_runs_total.load(.acquire),
        .sandbox_host_runs_total = g_sandbox_host_runs_total.load(.acquire),
        .sandbox_bubblewrap_runs_total = g_sandbox_bubblewrap_runs_total.load(.acquire),
        .sandbox_kill_switch_total = g_sandbox_kill_switch_total.load(.acquire),
        .sandbox_preflight_failures_total = g_sandbox_preflight_failures_total.load(.acquire),
        .agent_echo_calls_total = g_agent_echo_calls_total.load(.acquire),
        .agent_scout_calls_total = g_agent_scout_calls_total.load(.acquire),
        .agent_warden_calls_total = g_agent_warden_calls_total.load(.acquire),
        .agent_tokens_total = g_agent_tokens_total.load(.acquire),
        .agent_echo_tokens_total = g_agent_echo_tokens_total.load(.acquire),
        .agent_scout_tokens_total = g_agent_scout_tokens_total.load(.acquire),
        .agent_warden_tokens_total = g_agent_warden_tokens_total.load(.acquire),
        .agent_orchestrator_tokens_total = g_agent_orchestrator_tokens_total.load(.acquire),
        .backoff_wait_ms_total = g_backoff_wait_ms_total.load(.acquire),
        .rate_limit_wait_ms_total = g_rate_limit_wait_ms_total.load(.acquire),
        .side_effect_outbox_enqueued_total = g_side_effect_outbox_enqueued_total.load(.acquire),
        .side_effect_outbox_delivered_total = g_side_effect_outbox_delivered_total.load(.acquire),
        .side_effect_outbox_dead_letter_total = g_side_effect_outbox_dead_letter_total.load(.acquire),
        .api_backpressure_rejections_total = g_api_backpressure_rejections_total.load(.acquire),
        .api_in_flight_requests = g_api_in_flight_requests.load(.acquire),
        .worker_allocator_leaks_total = g_worker_allocator_leaks_total.load(.acquire),
        .gate_repair_loops_total = g_gate_repair_loops_total.load(.acquire),
        .gate_repair_exhausted_total = g_gate_repair_exhausted_total.load(.acquire),
        .run_limit_token_budget_exceeded_total = g_run_limit_token_budget_exceeded_total.load(.acquire),
        .run_limit_wall_time_exceeded_total = g_run_limit_wall_time_exceeded_total.load(.acquire),
        .run_limit_repair_loops_exhausted_total = g_run_limit_repair_loops_exhausted_total.load(.acquire),
        .interrupt_queued_total = g_interrupt_queued_total.load(.acquire),
        .interrupt_instant_total = g_interrupt_instant_total.load(.acquire),
        .interrupt_fallback_total = g_interrupt_fallback_total.load(.acquire),
        .run_aborted_total = g_run_aborted_total.load(.acquire),
        .otel_export_total = g_otel_export_total.load(.acquire),
        .otel_export_failed_total = g_otel_export_failed_total.load(.acquire),
        .otel_last_success_at_ms = g_otel_last_success_at_ms.load(.acquire),
        .orphan_runs_recovered_total = g_orphan_runs_recovered_total.load(.acquire),
        .orphan_no_agent_profile_total = g_orphan_no_agent_profile_total.load(.acquire),
        .reconcile_running = g_reconcile_running.load(.acquire),
        .agent_duration_seconds = .{},
        .gate_repair_loops_per_run = .{},
        .interrupt_delivery_latency_ms = .{},
    };
    const ext = me.snapshotExternalFields();
    s.external_retries_total = ext.external_retries_total;
    s.external_retries_rate_limited_total = ext.external_retries_rate_limited_total;
    s.external_retries_timeout_total = ext.external_retries_timeout_total;
    s.external_retries_context_exhausted_total = ext.external_retries_context_exhausted_total;
    s.external_retries_auth_total = ext.external_retries_auth_total;
    s.external_retries_invalid_request_total = ext.external_retries_invalid_request_total;
    s.external_retries_server_error_total = ext.external_retries_server_error_total;
    s.external_retries_unknown_total = ext.external_retries_unknown_total;
    s.external_failures_total = ext.external_failures_total;
    s.external_failures_rate_limited_total = ext.external_failures_rate_limited_total;
    s.external_failures_timeout_total = ext.external_failures_timeout_total;
    s.external_failures_context_exhausted_total = ext.external_failures_context_exhausted_total;
    s.external_failures_auth_total = ext.external_failures_auth_total;
    s.external_failures_invalid_request_total = ext.external_failures_invalid_request_total;
    s.external_failures_server_error_total = ext.external_failures_server_error_total;
    s.external_failures_unknown_total = ext.external_failures_unknown_total;
    s.retry_after_hints_total = ext.retry_after_hints_total;
    mh.snapshotHistograms(&s);
    return s;
}
