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
const runner_events = @import("runner_events.zig");
const policy = @import("protocol_policy.zig");
const reports = @import("protocol_report.zig");
const memory = @import("protocol_memory.zig");
const credentials = @import("protocol_credentials.zig");

pub const LEASE_WIRE_VERSION_V1: u16 = 1;
pub const LEASE_WIRE_VERSION_CURRENT: u16 = 2;
pub const LEASE_REQUEST_CURRENT_JSON = "{\"wire_version\":2}";

/// Empty or malformed bodies are treated as version one by the handler. New
/// runners advertise the current version so newer enforcement may be issued.
pub const LeaseRequest = struct {
    wire_version: u16 = LEASE_WIRE_VERSION_V1,
};

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
/// GET + POST /v1/runners/me/memory/{fleet_id} — durable fleet-memory hydrate +
/// capture, keyed by the fleet. The runner names the fleet because it may hold
/// several concurrent leases; the server authorizes by verifying the runner holds
/// a live lease for that fleet (IDOR-safe — the client never reaches a fleet it
/// does not lease). The POST fences the write via `fencing_token` in the body,
/// like `/reports`. (`fleet_id` is our identifier end to end — the durable memory
/// rows key off it directly, with no legacy instance_id prefix.) This is the collection
/// prefix; the router appends the `{fleet_id}` segment. See
/// `docs/architecture/runner_fleet.md` §Memory continuity.
pub const PATH_RUNNER_MEMORY = PATH_RUNNERS ++ "/me/memory";
/// GET /v1/runners/me — read-only self status (`me` resolves from the token).
/// Distinct from the heartbeat: a pure read, it does NOT bump `last_seen_at`, so
/// an operator's `status` check can never mask a dead runner's liveness.
pub const PATH_RUNNER_SELF = PATH_RUNNERS ++ "/me";

/// GET /v1/runners/me/bundles/{content_hash} — runner-plane Fleet Bundle snapshot
/// download. The daemon proxies the immutable canonical tar from object storage
/// (the runner holds no datastore credentials). Collection prefix; the runner
/// appends the `{content_hash}` segment, mirroring `PATH_RUNNER_MEMORY`. The daemon
/// matcher keys on the `bundles` segment (`route_matchers_runner.zig`).
pub const PATH_RUNNER_BUNDLES = PATH_RUNNERS ++ "/me/bundles";

/// POST /v1/runners/me/credentials/mint — on-demand credential mint (M102 §3).
/// The runner forwards a sandboxed child's request here; the daemon's broker
/// mints a short-lived, workspace-scoped token. The workspace is derived from
/// the lease server-side (Invariant 2) — a caller-supplied workspace is ignored,
/// so the request carries only `lease_id`, the `integration` id, and an optional
/// `scope`. Static exact-match path (no path param); `me` resolves from the
/// `agt_r` token. Single-sourced (RULE UFS): the daemon handler matches on it and
/// the runner forwarder builds the URL from it.
pub const PATH_RUNNER_CREDENTIALS_MINT = PATH_RUNNERS ++ "/me/credentials/mint";

/// GET /v1/fleets/runners — platform-admin operator-plane read of the whole
/// fleet (paginated). The `/v1/fleet/...` namespace is the operator plane;
/// `/v1/runners` is enrollment + the runner self-plane. Distinct prefix so the
/// two never collide in the matcher.
pub const PATH_FLEET_RUNNERS = "/v1/fleets/runners";

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

// Renewal + report metering sub-protocol lives in `protocol_report.zig` (RULE
// FLL); re-exported here so `protocol.RenewRequest` (and siblings) are unchanged.
pub const RenewResponse = reports.RenewResponse;
pub const RenewRequest = reports.RenewRequest;

// Assigned-policy vocabulary + payloads live in `protocol_policy.zig` (RULE
// FLL); re-exported here so `protocol.SandboxTier` (and siblings) keep their
// names. The tier is control-plane-ASSIGNED (not self-reported telemetry) from
// the policy workstream on; the capability report stays unauthenticated
// self-assertion, so placement trust remains operator-assigned.
pub const SandboxTier = policy.SandboxTier;
pub const NetworkPolicy = policy.NetworkPolicy;
pub const FAIL_CLOSED_DEFAULT = policy.FAIL_CLOSED_DEFAULT;
pub const AssignedPolicy = policy.AssignedPolicy;
pub const CapabilityReport = policy.CapabilityReport;
pub const DEFAULT_WORKER_COUNT = policy.DEFAULT_WORKER_COUNT;
pub const MIN_WORKER_COUNT = policy.MIN_WORKER_COUNT;
pub const MAX_WORKER_COUNT = policy.MAX_WORKER_COUNT;
pub const MAX_REGISTRY_ENTRIES = policy.MAX_REGISTRY_ENTRIES;
pub const registryAllowlistValid = policy.registryAllowlistValid;
pub const capabilityReportBounded = policy.capabilityReportBounded;
pub const extraBindsValid = policy.extraBindsValid;
pub const ExtraBind = policy.ExtraBind;
pub const BindMode = policy.BindMode;
pub const MAX_EXTRA_BINDS = policy.MAX_EXTRA_BINDS;
pub const MAX_BIND_PATH_LEN = policy.MAX_BIND_PATH_LEN;
pub const MAX_BIND_NOTE_LEN = policy.MAX_BIND_NOTE_LEN;
pub const BASELINE_RO_PATHS = policy.BASELINE_RO_PATHS;
pub const SENSITIVE_PATHS = policy.SENSITIVE_PATHS;

/// How tenant secrets reach the runner. S0 ships `inline` only (secrets travel
/// in the lease over TLS, trusted fleet); `scoped`/`proxy` are the reserved
/// per-tenant / zero-trust modes a later workstream implements.
pub const SecretDelivery = enum { @"inline", scoped, proxy };

/// Terminal execution result the runner reports. Mirrors the
/// `core.fleet_events.status` values a runner can produce —
/// `gate_blocked`/`dead_lettered` are `agentsfleetd`-side and never runner-reported.
pub const Outcome = enum { processed, fleet_error };

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

/// Platform-admin mutation actions for `PATCH /v1/fleets/runners/{id}`. These
/// are wire enum values, so std.json accepts/serializes the tag names verbatim.
pub const RunnerAdminAction = enum { cordon, drain, revoke };

/// PATCH body: exactly one of `action` (admin-state transition) or
/// `assigned_policy` (policy re-assignment — reaches the host on its next
/// heartbeat, no host visit). Both-or-neither is a 400; the handler enforces it.
pub const RunnerAdminPatchRequest = struct {
    action: ?RunnerAdminAction = null,
    assigned_policy: ?AssignedPolicy = null,
};

pub const RunnerAdminPatchResponse = struct {
    id: []const u8,
    admin_state: AdminState,
    /// Present on the policy-update path: the assignment as stored.
    assigned_policy: ?AssignedPolicy = null,
};

pub const RunnerEventType = runner_events.RunnerEventType;
pub const PER_LEASE_EVENT_TYPES = runner_events.PER_LEASE_EVENT_TYPES;
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
/// every later call. The operator ASSIGNS the policy here; the host never
/// declares one.
pub const RegisterRequest = struct {
    host_id: []const u8,
    assigned_policy: AssignedPolicy,
    labels: []const []const u8,
};

/// register reply: the durable runner identity + its bearer token (returned once;
/// `agentsfleetd` stores only the token hash).
pub const RegisterResponse = struct {
    runner_id: []const u8,
    runner_token: []const u8,
    /// The assignment as stored (worker_count clamped into the shared bounds),
    /// echoed so the enrolling operator sees exactly what the host will apply.
    assigned_policy: AssignedPolicy,
};

/// POST /v1/runners/me/heartbeats request (Bearer runner_token). The capability
/// report rides the first heartbeat and any tick where the probe result
/// changed. Defaulted so an older runner's empty body parses to null — the row
/// then reads degraded with a no-capability-report reason, never a crash.
pub const HeartbeatRequest = struct {
    capability_report: ?CapabilityReport = null,
};

/// POST /v1/runners/me/heartbeats reply (`me` resolves from the token). Carries
/// the current assignment on every beat, so a dashboard change reaches the host
/// within one heartbeat with no host visit. `assigned_policy` is null only for
/// a row assigned before the policy columns existed — the runner then fails
/// closed and refuses to lease, and the row reads degraded.
pub const HeartbeatResponse = struct {
    status: HeartbeatStatus,
    assigned_policy: ?AssignedPolicy = null,
    degraded: bool = false,
    degraded_reason: ?[]const u8 = null,
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
    assigned_policy: ?AssignedPolicy = null,
    /// The host's stored capability report — what it can actually enforce.
    achievable: ?CapabilityReport = null,
    degraded: bool = false,
    degraded_reason: ?[]const u8 = null,
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
    /// The installed fleet's behaviour prose (the `SKILL.md` body after
    /// frontmatter extraction), so the sandboxed NullClaw turn runs the
    /// installed behaviour and not a generic chat. Soft reasoning input —
    /// hard tool/secret policy stays in `policy`. Additive + defaulted so a
    /// mixed-version fleet is safe: a new runner reading an older lease that
    /// omits the field gets `""`. An older runner negotiates the frozen
    /// version-one response and never receives newer fields.
    instructions: []const u8 = "",
    /// Content-addressed reference to the installed Fleet Bundle's canonical
    /// snapshot in object storage. Present only when the fleet was created from a
    /// bundle; the runner GETs `/v1/runners/me/bundles/{content_hash}` to
    /// materialize support files into the sandbox workspace. Additive + defaulted
    /// with the same rollout-safety as `instructions`: a new runner reading an
    /// older lease gets null and skips the download. An older runner receives
    /// the frozen version-one response.
    bundle: ?BundleManifest = null,
};

/// The downloadable half of a bundle-backed lease: the content hash addresses the
/// immutable canonical tar in object storage. The hash's presence on the lease is
/// the "has bundle" signal; a `404` from the download means the bundle is
/// skill-only (no support files were stored) and the runner proceeds with none.
pub const BundleManifest = struct {
    content_hash: []const u8,
};

/// POST /v1/runners/me/leases (Bearer runner_token, long-poll). Always 200:
/// `lease` is the work payload, or null with `retry_after_ms` set when there is
/// no work (a backoff hint — no 204).
pub const LeaseResponse = struct {
    lease: ?LeasePayload = null,
    retry_after_ms: ?u32 = null,
};

// The report family also lives in `protocol_report.zig` (RULE FLL).
pub const ReportTelemetry = reports.ReportTelemetry;
pub const ReportCheckpoint = reports.ReportCheckpoint;
pub const ReportRequest = reports.ReportRequest;
pub const ReportResponse = reports.ReportResponse;

// Durable fleet-memory wire sub-protocol lives in `protocol_memory.zig` (RULE
// FLL); re-exported here so `protocol.MemoryDelta` (and siblings) are unchanged.
pub const MAX_MEMORY_PUSH_BYTES = memory.MAX_MEMORY_PUSH_BYTES;
pub const MAX_MEMORY_ENTRIES_PER_AGENT = memory.MAX_MEMORY_ENTRIES_PER_AGENT;
pub const HYDRATE_WINDOW_BYTES = memory.HYDRATE_WINDOW_BYTES;
pub const MemoryDelta = memory.MemoryDelta;
pub const MemoryPushRequest = memory.MemoryPushRequest;
pub const MemoryHydrateResponse = memory.MemoryHydrateResponse;

// On-demand credential-mint wire sub-protocol lives in `protocol_credentials.zig`
// (RULE FLL); re-exported here so `protocol.MintCredentialRequest` is unchanged.
pub const MintCredentialRequest = credentials.MintCredentialRequest;
pub const MintCredentialResponse = credentials.MintCredentialResponse;

/// What the runner parent pipes down the child's stdin: the lease to execute,
/// plus the fleet's prior memory the parent already hydrated over the trusted
/// plane (`GET /v1/runners/me/memory/{fleet_id}`). The child seeds its
/// non-durable in-run store from `hydrated_memory` and never makes a network
/// call of its own — hydration rides the parent (which holds the `agt_r` token),
/// so no credential, URL, or DSN reaches the sandboxed fleet. The wrapper keeps
/// the lease shape unchanged while letting capture/hydrate flow parent-only.
pub const RunnerChildInput = struct {
    lease: LeasePayload,
    hydrated_memory: []const MemoryDelta,
};
