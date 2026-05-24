//! Frozen /v1/runners control contract — the request/response types and enums
//! the mothership and the host-resident runner exchange over HTTPS.
//!
//! These shapes are the interface the parallel runner workstreams build against;
//! do not change a field without amending the keystone spec. Two conventions
//! hold throughout:
//!   * Identity comes from the Bearer token, never the URL or body. register is
//!     authed by an existing operator/provisioner credential — a Clerk JWT or a
//!     `zmb_t_` api_key, via bearer_or_api_key — and mints the runner_token;
//!     every later call carries that minted runner_token (`/v1/runners/me/...`,
//!     where `me` resolves from the token). No request carries a runner_id —
//!     there is nothing to reconcile.
//!   * Wire enum values are the enum tag names verbatim (std.json renders enums
//!     via @tagName), so the enum is the single source for each value (RULE UFS).
//!
//! The lease payload reuses the canonical execution types so the wire and the
//! executor never drift: the event is the normalized `EventEnvelope`, and the
//! resolved config + inline secrets travel as the executor's own
//! `ExecutionPolicy`. Leases are fenced — see `LeasePayload.fencing_token`.

const EventEnvelope = @import("../zombie/event_envelope.zig");
const ExecutionPolicy = @import("../executor/context_budget.zig").ExecutionPolicy;

/// Isolation strength a runner *self-reports* at enrollment. Stored as telemetry
/// only — placement keys off operator-assigned trust, not this claim (a runner
/// can lie about its tier). The trust/attestation model lands in a later
/// identity + scheduler workstream.
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

/// POST /v1/runners — register. Auth: an existing credential —
/// `Bearer <Clerk JWT | zmb_t_ api_key>` (via bearer_or_api_key), not an
/// enrollment token. The response's runner_token identifies the runner on
/// every later call.
pub const RegisterRequest = struct {
    host_id: []const u8,
    sandbox_tier: SandboxTier,
    labels: []const []const u8,
};

/// register reply: the durable runner identity + its bearer token (returned once;
/// the mothership stores only the token hash).
pub const RegisterResponse = struct {
    runner_id: []const u8,
    runner_token: []const u8,
};

/// POST /v1/runners/me/heartbeats reply (Bearer runner_token; `me` resolves from
/// the token). The request body is empty in S0 — capacity/version land in a
/// later fleet/heartbeat workstream.
pub const HeartbeatResponse = struct {
    status: HeartbeatStatus,
};

/// The work half of a lease. `fencing_token` is a monotonic guard: report must
/// echo it, and a stale (reclaimed) lease holder carrying an older token is
/// rejected — this is what makes report safe under lease reclaim, beyond plain
/// idempotency by event_id.
pub const LeasePayload = struct {
    lease_id: []const u8,
    fencing_token: u64,
    /// Epoch ms after which the lease expires and the event becomes reclaimable.
    lease_expires_at: i64,
    secret_delivery: SecretDelivery,
    event: EventEnvelope,
    policy: ExecutionPolicy,
};

/// POST /v1/runners/me/leases (Bearer runner_token, long-poll). Always 200:
/// `lease` is the work payload, or null with `retry_after_ms` set when there is
/// no work (a backoff hint — no 204).
pub const LeaseResponse = struct {
    lease: ?LeasePayload = null,
    retry_after_ms: ?u32 = null,
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

/// POST /v1/runners/me/reports (Bearer runner_token) — one idempotent batched
/// write keyed by `event_id`, guarded by `fencing_token` against a reclaimed
/// lease. No runner_id: the token owns the identity.
pub const ReportRequest = struct {
    lease_id: []const u8,
    event_id: []const u8,
    fencing_token: u64,
    outcome: Outcome,
    response_text: []const u8,
    /// Billing token count → `zombie_execution_telemetry.token_count`.
    tokens: u64,
    telemetry: ReportTelemetry,
    checkpoint: ReportCheckpoint,
};

/// report reply — idempotent; a replay returns the recorded result, no double write.
pub const ReportResponse = struct {
    ok: bool,
};
