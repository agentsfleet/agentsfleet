//! Single-source knobs the control plane and the runner daemon both key off
//! (RULE UFS). Deliberately datastore-free — the daemon build graph
//! (`build_runner.zig`) imports this without pulling `pg`/`redis`, so the
//! "runner holds zero datastore credentials" invariant stays structural.

/// How long an issued lease/affinity claim stays valid before the slot becomes
/// reclaimable, and the increment each renewal adds. The control plane sets
/// `leased_until = now + this` and stamps the lease row's `lease_expires_at` to
/// the same value; the daemon treats it as the kill deadline. A live runner
/// extends it via the `/renew` verb (decoupling liveness from execution
/// duration); dead-runner detection comes from `HEARTBEAT_LAPSE_MS`, not from
/// shrinking this, so it stays short as the silent-death backstop.
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

/// A runner is treated as lapsed (its leases reassignable to other healthy
/// hosts) when its `fleet.runners.last_seen_at` is older than this. During a
/// long execution `last_seen_at` is bumped only by a successful `/renew`, which
/// fires at the first supervision tick inside the renewal window — so the
/// worst-case gap between bumps is `LEASE_TTL_MS - RENEWAL_WINDOW_MS +
/// RENEWAL_TICK_MS` (25 s: a renewal can slip one tick past the window opening,
/// not just `LEASE_TTL_MS - RENEWAL_WINDOW_MS`). This MUST exceed that gap, or
/// the deferred lapse-reassignment scan reclaims a healthy long-running lease
/// mid-cycle; it MUST stay under `LEASE_TTL_MS` so lapse detection still
/// front-runs the deadline backstop it exists to beat. The `comptime` block
/// pins both bounds so the reassignment work inherits a safe value.
/// `last_seen_at` is also bumped by the heartbeat verb between executions.
pub const HEARTBEAT_LAPSE_MS: i64 = 28_000;

comptime {
    // Worst-case gap between last_seen_at bumps on a live long run: the renewal
    // fires at the first tick under the window, so it can land one tick late.
    const max_renewal_gap_ms = LEASE_TTL_MS - RENEWAL_WINDOW_MS + RENEWAL_TICK_MS;
    if (HEARTBEAT_LAPSE_MS <= max_renewal_gap_ms)
        @compileError("HEARTBEAT_LAPSE_MS must exceed the worst-case renewal gap or the lapse scan falsely reclaims healthy leases");
    if (HEARTBEAT_LAPSE_MS >= LEASE_TTL_MS)
        @compileError("HEARTBEAT_LAPSE_MS must stay under LEASE_TTL_MS to front-run the deadline backstop");
}

/// Backoff hint handed to a runner when there is no work to lease. The lease
/// verb is always 200; this rides `retry_after_ms` (no 204).
pub const NO_WORK_RETRY_AFTER_MS: u32 = 1_000;

/// Consumer-id fallback when an ephemeral id cannot be allocated; a fixed id is
/// acceptable because zombied is the single Redis consumer for the stream.
pub const RUNNER_CONSUMER_FALLBACK = "runner-local";
