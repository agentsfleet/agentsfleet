//! Frozen /v1/runners control protocol — the request/response types and enums
//! `agentsfleetd` (the control plane) and the host-resident runner exchange over HTTPS.
//!
//! These shapes are the interface the parallel runner workstreams build against;
//! do not change a field without amending the keystone spec. Two conventions
//! hold throughout:
//!   * Identity comes from the Bearer token, never the URL or body. register is
//!     authed by an existing operator/provisioner credential — a Clerk JWT or a
//!     `agt_t` api_key, via bearer_or_api_key — and mints the runner_token;
//!     every later call carries that minted runner_token (`/v1/runners/me/...`,
//!     where `me` resolves from the token). No request carries a runner_id —
//!     there is nothing to reconcile.
//!   * Wire enum values are the enum tag names verbatim (std.json renders enums
//!     via @tagName), so the enum is the single source for each value (RULE UFS).
//!
//! The lease payload reuses the canonical execution types so the wire and the
//! runner never drift: the event is the normalized `EventEnvelope`, and the
//! resolved config + inline secrets travel as the runner's own
//! `ExecutionPolicy`. Leases are fenced — see `LeasePayload.fencing_token`.

const EventEnvelope = @import("event_envelope.zig");
const ExecutionPolicy = @import("execution_policy.zig").ExecutionPolicy;
const FailureClass = @import("execution_result.zig").FailureClass;
const runner_events = @import("runner_events.zig");

// ── Wire paths ──────────────────────────────────────────────────────────────
// Single-sourced (RULE UFS) so the router and the future TS client share them
// verbatim. Identity is the Bearer token, so the self-plane is `me` — no
// runner_id ever appears in a path (mirrors `/v1/tenants/me/...`).
pub const PATH_RUNNERS = "/v1/runners";

/// Runner-token prefix — the wire contract for the machine principal. Single-
/// sourced here (RULE UFS) because BOTH build graphs reference it: agentsfleetd mints
/// + validates it (`runner_bearer.zig`, `register.zig`) and the host daemon
/// validates the env-supplied token's prefix before the lease loop. The literal
/// must stay `agt_r` verbatim — runner_bearer carries the pin test.
pub const RUNNER_TOKEN_PREFIX = "agt_r";

pub const PATH_RUNNER_HEARTBEATS = PATH_RUNNERS ++ "/me/heartbeats";
pub const PATH_RUNNER_LEASES = PATH_RUNNERS ++ "/me/leases";
pub const PATH_RUNNER_REPORTS = PATH_RUNNERS ++ "/me/reports";
/// GET + POST /v1/runners/me/memory/{agent_id} — durable agent-memory hydrate +
/// capture, keyed by the agent. The runner names the agent because it may hold
/// several concurrent leases; the server authorizes by verifying the runner holds
/// a live lease for that agent (IDOR-safe — the client never reaches a agent it
/// does not lease). The POST fences the write via `fencing_token` in the body,
/// like `/reports`. (`agent_id` is our identifier end to end; its `zmb:`-prefixed
/// storage form is internal to `agentsfleetd`'s memory adapter.) This is the collection
/// prefix; the router appends the `{agent_id}` segment. See
/// `docs/architecture/runner_fleet.md` §Memory continuity.
pub const PATH_RUNNER_MEMORY = PATH_RUNNERS ++ "/me/memory";
/// GET /v1/runners/me — read-only self status (`me` resolves from the token).
/// Distinct from the heartbeat: a pure read, it does NOT bump `last_seen_at`, so
/// an operator's `status` check can never mask a dead runner's liveness.
pub const PATH_RUNNER_SELF = PATH_RUNNERS ++ "/me";

/// GET /v1/fleet/runners — platform-admin operator-plane read of the whole
/// fleet (paginated). The `/v1/fleet/...` namespace is the operator plane;
/// `/v1/runners` is enrollment + the runner self-plane. Distinct prefix so the
/// two never collide in the matcher.
pub const PATH_FLEET_RUNNERS = "/v1/fleet/runners";

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
/// `core.agent_events.status` values a runner can produce —
/// `gate_blocked`/`dead_lettered` are `agentsfleetd`-side and never runner-reported.
pub const Outcome = enum { processed, agent_error };

/// Heartbeat reply status. `ok` is the only S0 value; `drain`/`stop` are
/// reserved for fleet failover so that workstream needn't recut the type.
pub const HeartbeatStatus = enum { ok, drain, stop };

/// `fleet.runners.admin_state` — operator intent, a typed enum, app-enforced (no
/// SQL CHECK, per RULE STS). `active` admits the runner plane; cordoned/draining/
/// drained/revoked all reject it (→ 401 UZ-RUN-009). Renamed from `status`. The
/// enum is the single source for the operator PATCH; the string consts below are
/// derived from it (RULE UFS) for the SQL insert + the active gate. Not a wire value.
pub const AdminState = enum { active, cordoned, draining, drained, revoked };
/// The only `admin_state` that admits a runner-plane call — derived from the enum
/// (RULE UFS). Used by register (insert) and the runnerBearer lookup (active gate).
pub const ADMIN_STATE_ACTIVE = @tagName(AdminState.active);

/// Platform-admin mutation actions for `PATCH /v1/fleet/runners/{id}`. These
/// are wire enum values, so std.json accepts/serializes the tag names verbatim.
pub const RunnerAdminAction = enum { cordon, drain, revoke };

pub const RunnerAdminPatchRequest = struct {
    action: RunnerAdminAction,
};

pub const RunnerAdminPatchResponse = struct {
    id: []const u8,
    admin_state: AdminState,
};

pub const RunnerEventType = runner_events.RunnerEventType;
pub const RunnerEventItem = runner_events.RunnerEventItem;
pub const RunnerEventsResponse = runner_events.RunnerEventsResponse;

/// `fleet.runners.last_seen_at` sentinel for a runner minted but never seen.
/// register inserts this; the heartbeat moves it to `now`. The fleet read
/// derives `registered` from it, so a fresh runner is honestly "registered",
/// not a fake "online". Single-sourced (RULE UFS) — the minter and the liveness
/// derivation must agree on the sentinel.
pub const RUNNER_LAST_SEEN_NEVER: i64 = 0;

/// Derived runtime liveness of a runner — computed by the fleet read from
/// `last_seen_at` + the live-lease join, NEVER stored (storing it would drift;
/// see docs/architecture/runner_fleet.md "Runner state"). Serialized by tag
/// name; the dashboard's `RunnerLiveness` union mirrors these verbatim (UFS).
///   registered — minted, never connected (`last_seen_at == RUNNER_LAST_SEEN_NEVER`)
///   busy       — holds a live lease (actively renewing — takes precedence over offline)
///   online     — heartbeat fresh, no live lease
///   offline    — heartbeat stale beyond the lapse threshold
pub const RunnerLiveness = enum { registered, busy, online, offline };

/// `fleet.runner_leases.status` lifecycle values — app-enforced (no SQL CHECK,
/// per RULE STS). `active` at lease issue, `reported` once the runner's report
/// finalizes, `expired` when reclaim re-leases a dead holder's event to another
/// runner. Single-sourced here (insert in the lease service, update in the
/// report + reclaim services); not a wire value.
pub const RUNNER_LEASE_STATUS_ACTIVE = "active";
pub const RUNNER_LEASE_STATUS_REPORTED = "reported";
pub const RUNNER_LEASE_STATUS_EXPIRED = "expired";

/// POST /v1/runners — register. Auth: an existing credential —
/// `Bearer <Clerk JWT | agt_t api_key>` (via bearer_or_api_key), not an
/// enrollment token. The response's runner_token identifies the runner on
/// every later call.
pub const RegisterRequest = struct {
    host_id: []const u8,
    sandbox_tier: SandboxTier,
    labels: []const []const u8,
};

/// register reply: the durable runner identity + its bearer token (returned once;
/// `agentsfleetd` stores only the token hash).
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

/// GET /v1/runners/me reply (Bearer runner_token). The runner's own registration
/// row, read-only — `status` reads this instead of heartbeating so inspecting a
/// host never writes liveness. `last_seen_at` is epoch ms (0 if never seen).
pub const SelfResponse = struct {
    id: []const u8,
    status: []const u8,
    host_id: []const u8,
    sandbox_tier: []const u8,
    last_seen_at: i64,
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
    /// The installed agent's behaviour prose (the `SKILL.md` body after
    /// frontmatter extraction), so the sandboxed NullClaw turn runs the
    /// installed behaviour and not a generic chat. Soft reasoning input —
    /// hard tool/secret policy stays in `policy`. Additive + defaulted so a
    /// mixed-version fleet is safe: a new runner reading an older lease that
    /// omits the field gets `""` (rollout is runners-first — an older runner
    /// reading a newer lease rejects the unknown field and runs no work).
    instructions: []const u8 = "",
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

/// Session resume cursor written to `core.agent_sessions.context_json`.
pub const ReportCheckpoint = struct {
    last_event_id: []const u8,
    last_response: []const u8,
};

/// POST /v1/runners/me/reports (Bearer runner_token) — one batched write keyed
/// by `event_id`. `fencing_token` is echoed and recorded, and the control plane
/// verifies it at report: a reclaimed holder (token below the agent's live
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
    /// Billing token count → `agent_execution_telemetry.token_count`.
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

/// Upper bound on the total memory bytes (sum of key+content+category over every
/// delta) one push may carry. The runner caps what it surfaces; the control plane
/// rejects beyond this. Oversized memory is truncated + logged, never silently
/// dropped whole. Single-sourced (RULE UFS) — both build graphs key off it.
pub const MAX_MEMORY_PUSH_BYTES: usize = 256 * 1024; // 256 KiB

/// Hard ceiling on the durable memory entries one agent may accumulate across all
/// its runs. The per-push cap bounds a single push; this bounds the unbounded
/// growth a long-lived (or adversarial) agent would otherwise build up — `enforceCap`
/// evicts beyond it server-side, tier-ordered: coldest non-core rows first, `core`
/// rows only when no non-core row remains. A backstop, not the primary bound
/// (stable-key overwrite + `memory_forget` are the agent's own).
pub const MAX_MEMORY_ENTRIES_PER_AGENT: usize = 1000;

/// Byte budget for one hydration window. The `GET` Compactor is category-pinned:
/// every `core` entry that fits hydrates first (newest-first), then the newest
/// non-core entries fill the remainder of this budget; the dropped entries stay
/// durable in Postgres, unhydrated. Bounds the payload a run seeds into the child
/// regardless of how large the durable set has grown.
pub const HYDRATE_WINDOW_BYTES: usize = 256 * 1024; // 256 KiB

/// One durable agent-memory item on the wire — the unit of both capture (POST
/// body) and hydrate (GET response). Carries no scope: the agent is the
/// `{agent_id}` path segment, server-validated against the runner's live lease.
/// One shape for a memory item, shared agentsfleetd <-> runner (RULE UFS).
pub const MemoryDelta = struct {
    key: []const u8,
    content: []const u8,
    category: []const u8,
};

/// POST /v1/runners/me/memory/{agent_id} (Bearer runner_token) — capture the
/// run's memory for the path's agent. `lease_id` + `fencing_token` ride the body
/// exactly like `ReportRequest`: the control plane loads that lease, verifies the
/// runner owns it, cross-checks `lease.agent_id == {agent_id}`, and fences the
/// write — a reclaimed holder (token below the agent's live fencing seq) is
/// rejected UZ-RUN-005. The scope (`agent_id`) is server-derived; each delta is
/// upserted (`ON CONFLICT (key, agent_id) DO UPDATE`), so a retried push is idempotent.
pub const MemoryPushRequest = struct {
    lease_id: []const u8,
    fencing_token: u64,
    memory: []const MemoryDelta,
};

/// GET /v1/runners/me/memory/{agent_id} reply — a compacted hydration window of
/// the path agent's durable memory (category-pinned under `HYDRATE_WINDOW_BYTES`:
/// `core` entries first, then newest non-core; the dropped entries stay in
/// Postgres), which the runner parent seeds into the child's in-run store at run
/// start. The runner names the agent it holds in its `LeasePayload`; the server
/// returns memory only for a agent the runner holds a live lease for.
pub const MemoryHydrateResponse = struct {
    memory: []const MemoryDelta,
};

/// What the runner parent pipes down the child's stdin: the lease to execute,
/// plus the agent's prior memory the parent already hydrated over the trusted
/// plane (`GET /v1/runners/me/memory/{agent_id}`). The child seeds its
/// non-durable in-run store from `hydrated_memory` and never makes a network
/// call of its own — hydration rides the parent (which holds the `agt_r` token),
/// so no credential, URL, or DSN reaches the sandboxed agent. The wrapper keeps
/// the lease shape unchanged while letting capture/hydrate flow parent-only.
pub const RunnerChildInput = struct {
    lease: LeasePayload,
    hydrated_memory: []const MemoryDelta,
};
