//! Renewal + report wire sub-protocol — the metering-bearing half of the
//! runner plane: lease renewal (cumulative token counts → fenced charge) and
//! the terminal execution report. Split from `protocol.zig` (RULE FLL) and
//! re-exported there, so consumers keep the `protocol.X` names.

const FailureClass = @import("execution_result.zig").FailureClass;
const Outcome = @import("protocol.zig").Outcome;

/// renew reply (200): the authoritative new kill deadline (epoch ms). The runner
/// retargets its child wall-clock deadline to this. A non-200 (`UZ-RUN-010`
/// max-runtime, `011` lease_lost, `012` no-credits) means stop renewing and kill
/// the child — the run is over.
pub const RenewResponse = struct {
    lease_expires_at: i64,
};

/// renew request body — the runner's **cumulative** token counts for the run so
/// far (NOT deltas). The control plane charges the diff since the lease's
/// last-metered cursor inside the fenced renewal CTE, then advances the cursor;
/// so a fail-safe retry that re-sends the same cumulatives a few ms later
/// charges ≈0 (cumulative-diff idempotency). Additive + defaulted to 0: an empty
/// body or an older-runner body parses to all-zero → run-fee-only metering,
/// never a negative charge. Counts are audit data, not secrets — safe to log.
pub const RenewRequest = struct {
    input_tokens: u32 = 0,
    cached_input_tokens: u32 = 0,
    output_tokens: u32 = 0,
};

/// Latency telemetry the runner observed for one execution.
pub const ReportTelemetry = struct {
    time_to_first_token_ms: u32,
    wall_ms: u64,
};

/// Session resume cursor written to `core.fleet_sessions.context_json`.
pub const ReportCheckpoint = struct {
    last_event_id: []const u8,
    last_response: []const u8,
};

/// POST /v1/runners/me/reports (Bearer runner_token) — one batched write keyed
/// by `event_id`. `fencing_token` is echoed and recorded, and the control plane
/// verifies it at report: a reclaimed holder (token below the fleet's live
/// fencing sequence) is fenced UZ-RUN-005. No runner_id: the token owns the identity.
pub const ReportRequest = struct {
    lease_id: []const u8,
    event_id: []const u8,
    fencing_token: u64,
    outcome: Outcome,
    /// Granular failure cause when the execution failed, the runner's own
    /// `FailureClass` carried verbatim (std.json renders it via @tagName).
    /// Optional + defaulted so a mixed-version fleet is safe: an older runner
    /// omits it and the control plane treats absent as "reason unknown". The
    /// coarse `outcome` above stays the binary processed/fleet_error verdict.
    failure_reason: ?FailureClass = null,
    /// Human-readable cause line from the classification site (which check
    /// failed, and why). Defaulted empty so an older runner omits it safely;
    /// persisted only when the outcome is a failure (same trust boundary as
    /// `failure_reason`).
    failure_detail: []const u8 = "",
    response_text: []const u8,
    /// Billing token count → `fleet_execution_telemetry.token_count`.
    tokens: u64,
    /// The runner's **cumulative** token counts for the whole run (NOT deltas) —
    /// the same three fields `RenewRequest` carries, so the report-settle can
    /// charge the final slice (the diff since the lease's last-metered cursor)
    /// and the per-renewal debits + settle sum to the real total. Additive +
    /// defaulted to 0: an older runner that omits them settles run-fee-only.
    input_tokens: u32 = 0,
    cached_input_tokens: u32 = 0,
    output_tokens: u32 = 0,
    telemetry: ReportTelemetry,
    checkpoint: ReportCheckpoint,
};

/// report reply. S0 reproduces the direct worker's finalize() writes (terminal
/// status + telemetry actuals + session checkpoint) then XACKs; true
/// idempotency (`INSERT … ON CONFLICT`) + fencing verification are the later
/// `agentsfleetd` lease/report logic.
pub const ReportResponse = struct {
    ok: bool,
};
