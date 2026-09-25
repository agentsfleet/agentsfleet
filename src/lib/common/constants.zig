//! Single-source knobs the control plane and the runner daemon both key off
//! (RULE UFS). Deliberately datastore-free — the daemon build graph
//! (`build_runner.zig`) imports this without pulling `pg`/`redis`, so the
//! "runner holds zero datastore credentials" invariant stays structural.

/// Wall-clock helper (`clock.nowMillis`/`nowNanos`), re-exported through the
/// `common` module so every build graph that already imports `common` reaches
/// it as `@import("common").clock` — see `clock.zig`.
pub const clock = @import("clock.zig");

/// `Options` is reached by coercion from an anonymous literal at every
/// instantiation, so it stays unexported — a re-export nobody names is dead code
/// (RULE NDC).
/// Process-wide blocking sync (`common.Mutex`/`Condition`) + their shared `Io`
/// accessor — Zig 0.16's replacement for `std.Thread.Mutex`. See `sync.zig`.
const sync = @import("sync.zig");
pub const Mutex = sync.Mutex;
pub const RwLock = sync.RwLock;
pub const Condition = sync.Condition;
pub const WaitGroup = sync.WaitGroup;
pub const Event = sync.Event;
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

/// The one bounded retry loop for idempotent calls: the caller passes a
/// `Policy` (retryable errors, attempt cap, time budget) and the loop paces
/// with `backoff` (`common.retry.run`). See `retry.zig`.
pub const retry = @import("retry.zig");

/// Shared env-var reads over the 0.16 `Environ.Map` both binaries thread from
/// `std.process.Init` (`common.env.owned`). See `env.zig`.
pub const env = @import("env.zig");

/// Process-level current RSS reader (`common.rss.currentBytes`) — the
/// coarse memory oracle the RSS growth-probe soaks read; null where
/// unsupported so a probe skips. See `rss.zig`.
pub const rss = @import("rss.zig");

/// The runner auto-renews a lease once fewer than this many ms remain before
/// the deadline the server sent it.
///
/// Runner-local policy, and only that. The lease's length is the daemon's and
/// arrives on the renew reply, so this file does not spell it: the driver
/// compares against `self.deadline_ms` and never against a duration of its own.
/// What this value has to be smaller than is the remaining lease, which the
/// daemon guarantees by never issuing a deadline closer than one renewal
/// window — so all that is left to enforce here is the tick below.
///
/// Renew-fail is fail-safe: unrenewed by the deadline → child killed + event
/// reclaimed, never double-run.
pub const RENEWAL_WINDOW_MS: i64 = 10_000;

/// How often the runner's child-supervision read loop wakes to consider a
/// renewal while waiting on a quiet-but-alive child (e.g. a long model call that
/// emits no progress frames). Must be < `RENEWAL_WINDOW_MS` so at least one tick
/// lands inside the window before the deadline. The wake is also the synthetic
/// keepalive cadence — a tick on a live child attests liveness even with no
/// frames, so a legitimate long run renews and is never falsely reclaimed.
pub const RENEWAL_TICK_MS: i64 = 5_000;

comptime {
    if (RENEWAL_TICK_MS >= RENEWAL_WINDOW_MS)
        @compileError("RENEWAL_TICK_MS must be < RENEWAL_WINDOW_MS so at least one tick lands inside the renewal window");
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
const PROVIDER_SLACK = "slack";
