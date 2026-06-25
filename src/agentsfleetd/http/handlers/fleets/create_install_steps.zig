//! Synthetic install-progression emitter for POST /v1/workspaces/{ws}/fleets.
//!
//! There is no provisioning subsystem — a fleet is functionally ready the
//! instant its row + event stream exist. To give the dashboard a live,
//! terminal-native install sequence anyway, the create path spawns a detached
//! worker here that, AFTER the 201 has been sent:
//!
//!   1. sleeps briefly so the client's post-create SSE subscriber has time to
//!      attach to `fleet:{id}:activity` (the channel is ephemeral — anything
//!      published before the subscriber connects is lost, so we wait);
//!   2. publishes `install:creating` then `install:provisioning`;
//!   3. flips `core.fleets.status` from `installing` to `active`;
//!   4. publishes `install:ready` (emitted AFTER the flip so a late reconnect
//!      that reads `fleet.status` also sees `active` — no stuck spinner).
//!
//! The flip is the source of truth; the frames are a cosmetic live tail. A
//! dropped frame or a failed spawn leaves the row reconcilable from its status
//! column (a subscriber that connects late re-renders the correct step from
//! `fleet.status`), so every failure path here is best-effort + logged.
//!
//! Allocator: the worker owns heap copies of `fleet_id` + `workspace_id` (the
//! request arena dies with the 201). The pool + queue pointers are boot-owned
//! and outlive every request. Extracted from create.zig (RULE FLL).

const std = @import("std");
const constants = @import("common");
const logging = @import("log");
const pg = @import("pg");
const ec = @import("../../../errors/error_registry.zig");
const queue_redis = @import("../../../queue/redis_client.zig");
const activity_publisher = @import("../../../fleet_runtime/activity_publisher.zig");
const fleet_config = @import("../../../fleet_runtime/config.zig");

const log = logging.scoped(.fleet_install);

/// Wall delay before the first frame, giving the client's post-201 SSE
/// subscriber time to attach before anything is published. Short enough that
/// "installing" reads as a quick beat, long enough to win the connect race on
/// a normal round-trip. The subsequent steps add one more beat each.
const SUBSCRIBER_ATTACH_MS: u64 = 250;
const STEP_GAP_MS: u64 = 200;
/// Upper bound the pre-flip beat must stay under so "installing" reads as a
/// quick beat rather than a stall — asserted by the timing pin test.
const MAX_INSTALL_BEAT_MS: u64 = 1000;

/// The ordered install-step kinds the happy path walks. The terminal step
/// (`install:ready`) is emitted by the caller after the status flip, so it is
/// intentionally absent here — this is only the pre-flip pair.
const PRE_FLIP_STEPS = [_][]const u8{
    activity_publisher.KIND_INSTALL_CREATING,
    activity_publisher.KIND_INSTALL_PROVISIONING,
};

/// Heap-owned worker payload. Freed by the worker on exit (the spawn boundary
/// hands ownership across). A dedicated stable allocator (`c_allocator`) is
/// used so the lifetime is independent of any request arena.
const Job = struct {
    pool: *pg.Pool,
    queue: *queue_redis.Client,
    workspace_id: []const u8,
    fleet_id: []const u8,
    /// Optional drain handle — `finish()`ed exactly once on worker exit so a
    /// harness can `wait()` for in-flight workers before tearing down the pool.
    /// Null in production (no graceful pool teardown to race).
    wg: ?*constants.WaitGroup = null,
};

fn allocator() std.mem.Allocator {
    return std.heap.c_allocator;
}

/// Spawn the detached install-step worker. The only synchronous failure the
/// caller can observe is job allocation / thread-spawn — both non-fatal (the
/// row stays `installing` and reconciles later), so the caller logs + proceeds.
pub fn spawn(
    pool: *pg.Pool,
    queue: *queue_redis.Client,
    workspace_id: []const u8,
    fleet_id: []const u8,
    wg: ?*constants.WaitGroup,
) !void {
    const job = try prepareJob(pool, queue, workspace_id, fleet_id);
    errdefer freeJob(job);
    job.wg = wg;

    // Register BEFORE the spawn so a harness that drains immediately after this
    // returns observes the worker; the matching finish() is the worker's last act
    // (or the errdefer below if the spawn itself fails).
    if (wg) |w| w.start();
    errdefer if (wg) |w| w.finish();

    const thread = try std.Thread.spawn(.{}, worker, .{job});
    thread.detach();
    // The worker owns `job` now; the errdefers above do not fire because the
    // spawn succeeded and no error path remains.
}

/// Build a heap-owned `*Job` or bubble up the allocation error. Every cleanup
/// errdefer lives here, never past the spawn boundary.
fn prepareJob(
    pool: *pg.Pool,
    queue: *queue_redis.Client,
    workspace_id: []const u8,
    fleet_id: []const u8,
) !*Job {
    const alloc = allocator();
    const job = try alloc.create(Job);
    errdefer alloc.destroy(job);

    const ws_copy = try alloc.dupe(u8, workspace_id);
    errdefer alloc.free(ws_copy);
    const fleet_copy = try alloc.dupe(u8, fleet_id);
    errdefer alloc.free(fleet_copy);

    job.* = .{ .pool = pool, .queue = queue, .workspace_id = ws_copy, .fleet_id = fleet_copy };
    return job;
}

fn freeJob(job: *Job) void {
    const alloc = allocator();
    alloc.free(job.workspace_id);
    alloc.free(job.fleet_id);
    alloc.destroy(job);
}

/// Detached entrypoint: run the progression, then free the job unconditionally.
/// The drain handle (if any) is `finish()`ed last — after the job's pool work is
/// done — so a harness `wait()`ing on it never frees the pool under the worker.
fn worker(job: *Job) void {
    const wg = job.wg;
    defer if (wg) |w| w.finish();
    defer freeJob(job);
    runProgression(job);
}

/// The full install beat: wait for the subscriber, emit the pre-flip steps,
/// flip the status, then emit the ready step. Each sub-step is best-effort.
fn runProgression(job: *Job) void {
    var scratch = activity_publisher.Scratch.init(allocator());
    defer scratch.deinit();

    constants.sleepNanos(SUBSCRIBER_ATTACH_MS * std.time.ns_per_ms);
    for (PRE_FLIP_STEPS) |kind| {
        activity_publisher.publishInstallStep(job.queue, &scratch, job.fleet_id, kind);
        constants.sleepNanos(STEP_GAP_MS * std.time.ns_per_ms);
    }

    flipToActive(job) catch |err| {
        // The flip failed — emit `install:error` so the dashboard stops the
        // spinner with a retry instead of hanging; the row is reconcilable from
        // its (still `installing`) status column.
        log.warn(
            "install_flip_failed",
            .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .err = @errorName(err), .fleet_id = job.fleet_id },
        );
        activity_publisher.publishInstallStep(job.queue, &scratch, job.fleet_id, activity_publisher.KIND_INSTALL_ERROR);
        return;
    };

    // Ready is published AFTER the flip so a subscriber reconciling from
    // `fleet.status` at this point also reads `active`.
    activity_publisher.publishInstallStep(job.queue, &scratch, job.fleet_id, activity_publisher.KIND_INSTALL_READY);
}

/// Flip the fleet from `installing` to `active`. Scoped to both ids and guarded
/// on the current status so a concurrent operator action (e.g. a kill during
/// the install beat) is never clobbered — only an still-installing row flips.
fn flipToActive(job: *Job) !void {
    const conn = try job.pool.acquire();
    defer job.pool.release(conn);
    _ = try conn.exec(
        \\UPDATE core.fleets SET status = $1, updated_at = $2
        \\WHERE id = $3::uuid AND workspace_id = $4::uuid AND status = $5
    , .{
        fleet_config.FleetStatus.active.toSlice(),
        constants.clock.nowMillis(),
        job.fleet_id,
        job.workspace_id,
        fleet_config.FleetStatus.installing.toSlice(),
    });
}

// ── tests ──────────────────────────────────────────────────────────────────
// The DB flip + the detached spawn need a live Postgres + Redis, so the
// behavioural proof lives in the integration suite. The pure, deterministic
// invariants are pinned here: the step ordering + the cross-tier kind values
// the dashboard's reducer keys on.

test "PRE_FLIP_STEPS: creating then provisioning, ready/error emitted out-of-band" {
    try std.testing.expectEqual(@as(usize, 2), PRE_FLIP_STEPS.len);
    try std.testing.expectEqualStrings(activity_publisher.KIND_INSTALL_CREATING, PRE_FLIP_STEPS[0]);
    try std.testing.expectEqualStrings(activity_publisher.KIND_INSTALL_PROVISIONING, PRE_FLIP_STEPS[1]);
    // ready + error are not in the pre-flip list — ready follows the flip,
    // error replaces it on failure. A regression that pre-emits ready would
    // flip the dashboard out of install-mode before the row is active.
    for (PRE_FLIP_STEPS) |kind| {
        try std.testing.expect(!std.mem.eql(u8, kind, activity_publisher.KIND_INSTALL_READY));
        try std.testing.expect(!std.mem.eql(u8, kind, activity_publisher.KIND_INSTALL_ERROR));
    }
}

test "install kind values are the agreed cross-tier contract strings" {
    // These four strings are the contract the dashboard's FRAME_KIND.INSTALL_*
    // mirrors verbatim. Pin them so a rename on this side is caught here, not in
    // a silently-stuck install spinner in the browser.
    try std.testing.expectEqualStrings("install:creating", activity_publisher.KIND_INSTALL_CREATING);
    try std.testing.expectEqualStrings("install:provisioning", activity_publisher.KIND_INSTALL_PROVISIONING);
    try std.testing.expectEqualStrings("install:ready", activity_publisher.KIND_INSTALL_READY);
    try std.testing.expectEqualStrings("install:error", activity_publisher.KIND_INSTALL_ERROR);
}

test "install step timing is a short, bounded beat (no multi-second stall)" {
    // The whole pre-flip wait is attach + one gap per step; keep it sub-second
    // so "installing" reads as a quick beat, not a hang.
    const total_ms = SUBSCRIBER_ATTACH_MS + STEP_GAP_MS * PRE_FLIP_STEPS.len;
    try std.testing.expect(total_ms < MAX_INSTALL_BEAT_MS);
}
