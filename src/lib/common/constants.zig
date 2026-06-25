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
