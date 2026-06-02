//! Frozen /v1/runners control protocol — the request/response types and enums
//! `zombied` (the control plane) and the host-resident runner exchange over HTTPS.
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

const EventEnvelope = @import("event_envelope.zig");
const ExecutionPolicy = @import("execution_policy.zig").ExecutionPolicy;
const FailureClass = @import("execution_result.zig").FailureClass;

// ── Wire paths ──────────────────────────────────────────────────────────────
// Single-sourced (RULE UFS) so the router and the future TS client share them
// verbatim. Identity is the Bearer token, so the self-plane is `me` — no
// runner_id ever appears in a path (mirrors `/v1/tenants/me/...`).
pub const PATH_RUNNERS = "/v1/runners";

/// Runner-token prefix — the wire contract for the machine principal. Single-
/// sourced here (RULE UFS) because BOTH build graphs reference it: zombied mints
/// + validates it (`runner_bearer.zig`, `register.zig`) and the host daemon
/// validates the env-supplied token's prefix before the lease loop. The literal
/// must stay `zrn_` verbatim — runner_bearer carries the pin test.
pub const RUNNER_TOKEN_PREFIX = "zrn_";

pub const PATH_RUNNER_HEARTBEATS = PATH_RUNNERS ++ "/me/heartbeats";
pub const PATH_RUNNER_LEASES = PATH_RUNNERS ++ "/me/leases";
pub const PATH_RUNNER_REPORTS = PATH_RUNNERS ++ "/me/reports";

/// Trailing segment of the per-lease activity sub-resource. `lease_id` is a path
/// param — `POST /v1/runners/me/leases/{lease_id}/activity` — so this can't be a
/// joined const like the others: the runner builds the full path off
/// `PATH_RUNNER_LEASES`, and the router matcher keys on this suffix segment.
pub const RUNNER_LEASE_ACTIVITY_SUFFIX = "activity";

/// Trailing segment of the per-lease renewal sub-resource —
/// `POST /v1/runners/me/leases/{lease_id}/renew`. Like the activity suffix this
/// stays a bare segment (the runner joins it onto `PATH_RUNNER_LEASES/{id}`) and
/// the router matcher keys on it. The runner calls this inside the renewal
/// window while actively executing, to push its kill deadline forward.
pub const RUNNER_LEASE_RENEW_SUFFIX = "renew";

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
/// `gate_blocked`/`dead_lettered` are `zombied`-side and never runner-reported.
pub const Outcome = enum { processed, agent_error };

/// Heartbeat reply status. `ok` is the only S0 value; `drain`/`stop` are
/// reserved for fleet failover so that workstream needn't recut the type.
pub const HeartbeatStatus = enum { ok, drain, stop };

/// `fleet.runners.status` lifecycle value — app-enforced (no SQL CHECK, per
/// RULE STS). Single-sourced here for register (insert) and the runnerBearer
/// lookup (active gate). Not a wire value.
pub const RUNNER_STATUS_ACTIVE = "active";

/// `fleet.runner_leases.status` lifecycle values — app-enforced (no SQL CHECK,
/// per RULE STS). `active` at lease issue, `reported` once the runner's report
/// finalizes, `expired` when reclaim re-leases a dead holder's event to another
/// runner. Single-sourced here (insert in the lease service, update in the
/// report + reclaim services); not a wire value.
pub const RUNNER_LEASE_STATUS_ACTIVE = "active";
pub const RUNNER_LEASE_STATUS_REPORTED = "reported";
pub const RUNNER_LEASE_STATUS_EXPIRED = "expired";

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
/// `zombied` stores only the token hash).
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

/// POST /v1/runners/me/reports (Bearer runner_token) — one batched write keyed
/// by `event_id`. `fencing_token` is echoed and recorded, and the control plane
/// verifies it at report: a reclaimed holder (token below the zombie's live
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
    /// coarse `outcome` above stays the binary processed/agent_error verdict.
    failure_reason: ?FailureClass = null,
    response_text: []const u8,
    /// Billing token count → `zombie_execution_telemetry.token_count`.
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
/// `zombied` lease/report logic.
pub const ReportResponse = struct {
    ok: bool,
};
