//! Frozen /v1/runner control contract — the request/response types and enums
//! the mothership and the host-resident runner exchange over HTTPS.
//!
//! These shapes are the interface the parallel runner workstreams build
//! against; do not change a field without amending the keystone spec. Wire
//! enum values are the enum tag names verbatim (std.json renders enums via
//! @tagName), which makes the enum the single source for each value per
//! RULE UFS.
//!
//! The lease payload reuses the canonical execution types so the wire and the
//! executor never drift: the event is the normalized `EventEnvelope` every
//! producer/consumer already shares, and the resolved config + secrets travel
//! as the executor's own `ExecutionPolicy` (network policy, tool allowlist,
//! secrets_map, context budget).

const EventEnvelope = @import("../zombie/event_envelope.zig");
const ExecutionPolicy = @import("../executor/context_budget.zig").ExecutionPolicy;

/// Isolation strength a runner reports at enrollment. Assignment refuses to
/// place other-tenant or production work on a weak tier (that logic is later).
pub const SandboxTier = enum { landlock_full, container_nested, macos_seatbelt, dev_none };

/// How tenant secrets reach the runner. S0 ships `inline` only (secrets travel
/// in the lease over TLS, trusted fleet); `scoped`/`proxy` are the reserved
/// per-tenant / zero-trust modes a later workstream implements.
pub const SecretDelivery = enum { @"inline", scoped, proxy };

/// Terminal execution result the runner reports. Mirrors the
/// `core.zombie_events.status` values a runner can produce —
/// `gate_blocked`/`dead_lettered` are mothership-side and never runner-reported.
pub const Outcome = enum { processed, agent_error };

/// Heartbeat reply status. `ok` is the only S0 value; `drain`/`stop` are
/// reserved for fleet failover so that workstream needn't recut the type.
pub const HeartbeatStatus = enum { ok, drain, stop };

/// POST /v1/runner/register — exchange an enrollment token for a runner_token.
pub const RegisterRequest = struct {
    enrollment_token: []const u8,
    host_id: []const u8,
    sandbox_tier: SandboxTier,
    labels: []const []const u8,
};

/// register reply: the durable runner identity + its bearer token (returned
/// once; the mothership stores only the token hash).
pub const RegisterResponse = struct {
    runner_id: []const u8,
    runner_token: []const u8,
};

/// POST /v1/runner/heartbeat (Bearer runner_token).
pub const HeartbeatRequest = struct {
    runner_id: []const u8,
};

/// heartbeat reply — see `HeartbeatStatus`.
pub const HeartbeatResponse = struct {
    status: HeartbeatStatus,
};

/// POST /v1/runner/lease (Bearer runner_token, long-poll) success body. A 204
/// (no body) means no work — represented at the HTTP layer, not as a variant.
pub const LeaseResponse = struct {
    event: EventEnvelope,
    /// Resolved per-execution config + (mode inline) the tenant secrets_map.
    policy: ExecutionPolicy,
};

/// Latency telemetry the runner observed for one execution.
pub const ReportTelemetry = struct {
    time_to_first_token_ms: u32,
    wall_ms: u64,
};

/// Session resume cursor written to `core.zombie_sessions.context_json`.
pub const ReportCheckpoint = struct {
    last_event_id: []const u8,
    last_response: []const u8,
};

/// POST /v1/runner/report (Bearer runner_token) — one idempotent batched write
/// keyed by `event_id`.
pub const ReportRequest = struct {
    runner_id: []const u8,
    event_id: []const u8,
    outcome: Outcome,
    response_text: []const u8,
    /// Billing token count → `zombie_execution_telemetry.token_count`.
    tokens: u64,
    telemetry: ReportTelemetry,
    checkpoint: ReportCheckpoint,
};

/// report reply — idempotent; a replay returns the recorded result, no
/// double write.
pub const ReportResponse = struct {
    ok: bool,
};
