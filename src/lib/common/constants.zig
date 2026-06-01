//! Single-source knobs the control plane and the runner daemon both key off
//! (RULE UFS). Deliberately datastore-free — the daemon build graph
//! (`build_runner.zig`) imports this without pulling `pg`/`redis`, so the
//! "runner holds zero datastore credentials" invariant stays structural.

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

// NOTE: the runner-lapse threshold (formerly `HEARTBEAT_LAPSE_MS`) is
// intentionally NOT defined here. Its only consumer — the lapse-reassignment
// scan over `last_seen_at` — is a later workstream, and the correct value
// depends on that scan's design (notably whether actively-renewing leases are
// in scope). It is reintroduced with its consumer so the threshold is derived
// from the real detection model rather than guessed ahead of it.

/// Backoff hint handed to a runner when there is no work to lease. The lease
/// verb is always 200; this rides `retry_after_ms` (no 204).
pub const NO_WORK_RETRY_AFTER_MS: u32 = 1_000;

/// Consumer-id fallback when an ephemeral id cannot be allocated; a fixed id is
/// acceptable because zombied is the single Redis consumer for the stream.
pub const RUNNER_CONSUMER_FALLBACK = "runner-local";
