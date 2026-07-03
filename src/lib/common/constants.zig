//! Single-source knobs the control plane and the runner daemon both key off
//! (RULE UFS). Deliberately datastore-free — the daemon build graph
//! (`build_runner.zig`) imports this without pulling `pg`/`redis`, so the
//! "runner holds zero datastore credentials" invariant stays structural.

/// Wall-clock helper (`clock.nowMillis`/`nowNanos`), re-exported through the
/// `common` module so every build graph that already imports `common` reaches
/// it as `@import("common").clock` — see `clock.zig`.
pub const clock = @import("clock.zig");

/// Process-wide blocking sync (`common.Mutex`/`Condition`) + their shared `Io`
/// accessor — Zig 0.16's replacement for `std.Thread.Mutex`. See `sync.zig`.
const sync = @import("sync.zig");
pub const Mutex = sync.Mutex;
pub const Condition = sync.Condition;
pub const WaitGroup = sync.WaitGroup;
pub const globalIo = sync.globalIo;
pub const sleepNanos = sync.sleepNanos;

/// Project-facing CSPRNG (`common.secureRandomBytes`) — Zig 0.16's replacement
/// for `std.crypto.random`/`std.posix.getrandom`. See `random.zig`.
pub const secureRandomBytes = @import("random.zig").secureRandomBytes;

/// Bounded, jittered exponential backoff for control-plane retries
/// (`common.backoff.ms(attempt)`; bounds `MAX_BACKOFF_MS`/`BASE_MS`/`JITTER_PCT`).
/// Single source for every retry sleep so an outage can't grow it unbounded.
/// See `backoff.zig`.
pub const backoff = @import("backoff.zig");

/// Shared env-var reads over the 0.16 `Environ.Map` both binaries thread from
/// `std.process.Init` (`common.env.owned`). See `env.zig`.
pub const env = @import("env.zig");

/// How long an issued lease/affinity claim stays valid before the slot becomes
/// reclaimable, and the increment each renewal adds. The control plane sets
/// `leased_until = now + this` and stamps the lease row's `lease_expires_at` to
/// the same value; the daemon treats it as the kill deadline. A live runner
/// extends it via the `/renew` verb (decoupling liveness from execution
/// duration); dead-runner detection is a separate later workstream (a lapse
/// scan over `last_seen_at`), not a function of shrinking this — so it stays
/// short as the silent-death backstop.
pub const LEASE_TTL_MS: i64 = 30_000;

/// The runner auto-renews a lease once fewer than this many ms remain before
/// `lease_expires_at`. Must be < `LEASE_TTL_MS` so a renewal leaves slack for a
/// transient failure to retry before the deadline (renew-fail is fail-safe:
/// unrenewed by the deadline → child killed + event reclaimed, never double-run).
pub const RENEWAL_WINDOW_MS: i64 = 10_000;

/// How often the runner's child-supervision read loop wakes to consider a
/// renewal while waiting on a quiet-but-alive child (e.g. a long model call that
/// emits no progress frames). Must be < `RENEWAL_WINDOW_MS` so at least one tick
/// lands inside the window before the deadline. The wake is also the synthetic
/// keepalive cadence — a tick on a live child attests liveness even with no
/// frames, so a legitimate long run renews and is never falsely reclaimed.
pub const RENEWAL_TICK_MS: i64 = 5_000;

/// Hard ceiling on a single lease's total wall-clock, measured from the lease
/// row's `created_at`. Renewal clamps to `min(now + LEASE_TTL_MS, created_at +
/// MAX_RUNTIME_MS)` and is refused once exceeded — a wedged-but-emitting agent
/// still terminates regardless of progress frames.
pub const MAX_RUNTIME_MS: i64 = 43_200_000;

/// Liveness lapse threshold: a runner whose `last_seen_at` is older than this is
/// derived `offline` by the fleet read. Reintroduced here with its first
/// consumer (the derived-liveness display) now that the detection model is
/// settled: an actively-renewing runner is `busy` (the live-lease check runs
/// BEFORE this threshold), so a long execution that stops heartbeating is never
/// falsely offline — exactly the concern that kept this undefined. Three lease
/// TTLs of silence is unambiguously dead for an idle runner (which heartbeats
/// every lease cycle). The lapse-reassignment scan reuses/refines this value.
pub const RUNNER_OFFLINE_AFTER_MS: i64 = LEASE_TTL_MS * 3;

/// Control-loop heartbeat cadence: how often the runner's main (control) thread
/// emits one host heartbeat, decoupled from worker execution now that the worker
/// pool owns lease polling. A busy pool no longer carries the heartbeat, so this
/// only has to keep an *idle* host live: it MUST stay below `RUNNER_OFFLINE_AFTER_MS`
/// (the comptime assertion below enforces it) so an idle host always heartbeats
/// before the fleet read would derive it offline. One stream per host regardless
/// of `RUNNER_WORKER_COUNT`.
pub const HEARTBEAT_INTERVAL_MS: i64 = 10_000;

comptime {
    if (HEARTBEAT_INTERVAL_MS >= RUNNER_OFFLINE_AFTER_MS)
        @compileError("HEARTBEAT_INTERVAL_MS must be < RUNNER_OFFLINE_AFTER_MS so an idle host heartbeats before it is derived offline");
}

/// Backoff hint handed to a runner when there is no work to lease. The lease
/// verb is always 200; this rides `retry_after_ms` (no 204).
pub const NO_WORK_RETRY_AFTER_MS: u32 = 1_000;

// ── Connectors (Slack-resident channel bot, M106) ───────────────────────────
// Provider + binding-kind identifiers shared across the OAuth connector
// (spec.zig aliases `PROVIDER_SLACK`), the inbound events ingress, and the
// generic `connector_installs`/`connector_channels` routing tables. The
// migrations (schema/029,030) reference these named constants rather than
// static-string CHECKs (RULE STS/UFS).

/// Connector provider id for Slack — the `provider` column value in
/// `connector_installs`/`connector_channels` and the `<provider>-app` /
/// `fleet:<provider>` vault-key stem.
pub const PROVIDER_SLACK = "slack";

/// Connector provider id for GitHub — the registry id, the `{provider}` route
/// segment, and the `github-app` / `fleet:github` vault-key stem.
pub const PROVIDER_GITHUB = "github";

/// Connector provider id for Zoho Desk — the registry id, the `{provider}` route
/// segment, and the `zoho-app` / `fleet:zoho` vault-key stem.
pub const PROVIDER_ZOHO = "zoho";

/// Connector provider id for Jira — the registry id, the `{provider}` route
/// segment, and the `jira-app` / `fleet:jira` vault-key stem.
pub const PROVIDER_JIRA = "jira";

/// Connector provider id for Linear — the registry id, the `{provider}` route
/// segment, and the `linear-app` / `fleet:linear` vault-key stem.
pub const PROVIDER_LINEAR = "linear";

/// Connector provider id for Datadog — the registry id, the `{provider}` route
/// segment, and the `fleet:datadog` vault-key stem.
pub const PROVIDER_DATADOG = "datadog";

/// Connector provider id for Grafana — the registry id, the `{provider}` route
/// segment, and the `fleet:grafana` vault-key stem.
pub const PROVIDER_GRAFANA = "grafana";

/// Connector provider id for Fly — the registry id, the `{provider}` route
/// segment, and the `fleet:fly` vault-key stem.
pub const PROVIDER_FLY = "fly";

/// OAuth 2.0 token endpoints for the refresh-token providers — the ONE spelling
/// shared by the connect flow (`connectors/<p>/spec.zig`) and the credential
/// broker's refresh-mint registry (`credentials/integration.zig`), so the code
/// exchange and the later refresh mint hit the same URL (RULE UFS).
pub const ZOHO_TOKEN_ENDPOINT: []const u8 = "https://accounts.zoho.com/oauth/v2/token";
pub const JIRA_TOKEN_ENDPOINT: []const u8 = "https://auth.atlassian.com/oauth/token";
pub const LINEAR_TOKEN_ENDPOINT: []const u8 = "https://api.linear.app/oauth/token";

/// `connector_channels.kind` for a per-channel resident fleet — the durable
/// fleet that owns one Slack channel's memory namespace.
pub const CONNECTOR_CHANNEL_KIND_RESIDENT = "resident";

/// Actor-attribution prefix for an inbound Slack mention event
/// (`slack:<slack_user_id>`), mirroring the webhook producer's `webhook:<src>`
/// shape. The signature-only ingress has no OIDC principal, so the actor is
/// free-form provenance, never an authorization subject.
pub const SLACK_ACTOR_PREFIX = "slack:";
