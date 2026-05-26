//! Single-source knobs the control plane and the runner daemon both key off
//! (RULE UFS). Deliberately datastore-free — the daemon build graph
//! (`build_runner.zig`) imports this without pulling `pg`/`redis`, so the
//! "runner holds zero datastore credentials" invariant stays structural.

/// How long an issued lease/affinity claim stays valid before the slot becomes
/// reclaimable. The control plane sets `leased_until = now + this` and stamps
/// the lease row's `lease_expires_at` to the same value; the daemon treats it
/// as the renewal deadline. There is no heartbeat-based renewal yet, so a live
/// runner on an event longer than this is reclaimed and its work redone
/// (fencing keeps that correct, just wasteful) — renewal is a follow-up.
pub const LEASE_TTL_MS: i64 = 30_000;

/// Backoff hint handed to a runner when there is no work to lease. The lease
/// verb is always 200; this rides `retry_after_ms` (no 204).
pub const NO_WORK_RETRY_AFTER_MS: u32 = 1_000;

/// Consumer-id fallback when an ephemeral id cannot be allocated; a fixed id is
/// acceptable because zombied is the single Redis consumer for the stream.
pub const RUNNER_CONSUMER_FALLBACK = "runner-local";
