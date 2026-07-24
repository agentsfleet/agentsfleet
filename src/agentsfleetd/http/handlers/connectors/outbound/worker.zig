//! `connector:outbound` consumer — the generic, provider-routed answer-delivery
//! worker (§4). A boot-started background thread (spawned by
//! `serve_background.Threads`, like the sweepers — NOT a separate daemon) that
//! reads jobs off the `connector:outbound` stream and dispatches each by
//! `provider` to the matching connector poster. This is the ONLY place a
//! connector poster is imported — the core report path stays provider-agnostic
//! (Invariant 9). Bounded retry + backoff live here once, for every connector.
//!
//! Recovery: a stable consumer id + a pending-first read redeliver any job left
//! unacked by an agentsfleetd crash/restart mid-post; a job whose inline retries
//! are exhausted is acked (dropped, logged UZ-SLK-030) so it never hammers.

const std = @import("std");
const constants = @import("common");
const logging = @import("log");
const pg = @import("pg");
const queue_redis = @import("../../../../queue/redis_client.zig");
const connector_outbound = @import("../../../../queue/connector_outbound.zig");
const ec = @import("../../../../errors/error_registry.zig");
const bounded_fetch = @import("../bounded_fetch.zig");
const slack_post = @import("../slack/post.zig");

const log = logging.scoped(.connector_outbound);

/// Idle backoff between non-blocking stream claims. Answers arrive at
/// model-run cadence (seconds), so a ≤250 ms pickup lag is noise — and the
/// shared queue connection is borrowed per-command instead of parked in a
/// server-side BLOCK (the scaling doc's "no per-stream blocking loop"
/// invariant holds again). Shutdown joins within one interval.
const IDLE_POLL_MS: u64 = 250;
/// Inline delivery attempts before a retryable failure is dropped (acked). Kept
/// small — a durable stream + pending redelivery cover a crash; this covers a
/// transient 429 / brief 5xx without hammering.
const MAX_ATTEMPTS: u32 = 3;
const BACKOFF_BASE_MS: u64 = 200;

/// Run until `shutdown` is set. Spawned by `serve_background.Threads.start`.
/// `slack_api_base` is the Slack Web API root (default in prod; a FakeSlack
/// loopback when a test drives this worker directly).
pub fn run(
    pool: *pg.Pool,
    queue: *queue_redis.Client,
    alloc: std.mem.Allocator,
    shutdown: *std.atomic.Value(bool),
    slack_api_base: []const u8,
    sched: *bounded_fetch.Scheduler,
) void {
    // Idempotent — serve already ensured the group at boot, but a worker that
    // starts before that (or after a group loss) self-heals.
    connector_outbound.ensureGroup(queue) catch |err|
        log.warn("outbound_group_ensure_failed", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .err = @errorName(err) });

    var consumer_buf: [queue_redis.CONSUMER_ID_BUF_LEN]u8 = undefined;
    const consumer_id = queue_redis.stableConsumerId(&consumer_buf);
    log.debug("outbound_worker_started", .{ .consumer = consumer_id });

    while (!shutdown.load(.acquire)) {
        // Pending-first: redeliver anything this consumer was handed but never
        // acked (a restart mid-post). Drain one per loop; a hit `continue`s to
        // check for more before claiming a new read.
        if (readPendingSafe(queue, consumer_id)) |d| {
            var job = d;
            deliverAndAck(pool, queue, alloc, sched, &job, slack_api_base);
            continue;
        }
        // Non-blocking claim; a hit loops straight back (drain a hot stream
        // without pacing), an idle pass sleeps the backoff.
        if (readNextSafe(queue, consumer_id)) |d| {
            var job = d;
            deliverAndAck(pool, queue, alloc, sched, &job, slack_api_base);
            continue;
        }
        constants.sleepNanos(IDLE_POLL_MS * std.time.ns_per_ms);
    }
    log.debug("outbound_worker_shutdown", .{});
}

fn readPendingSafe(queue: *queue_redis.Client, consumer_id: []const u8) ?connector_outbound.Delivery {
    return connector_outbound.readPending(queue, consumer_id) catch |err| {
        log.warn("outbound_read_pending_failed", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .err = @errorName(err) });
        return null;
    };
}

fn readNextSafe(queue: *queue_redis.Client, consumer_id: []const u8) ?connector_outbound.Delivery {
    return connector_outbound.readNext(queue, consumer_id) catch |err| {
        log.warn("outbound_read_next_failed", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .err = @errorName(err) });
        return null;
    };
}

/// Deliver a job (with bounded retry) then XACK it. A delivered or permanently-
/// failed job is acked; a retry-exhausted job is also acked (dropped, logged) so
/// it cannot hammer. Only a crash BEFORE the ack leaves it pending for redelivery.
fn deliverAndAck(pool: *pg.Pool, queue: *queue_redis.Client, alloc: std.mem.Allocator, sched: *bounded_fetch.Scheduler, job: *connector_outbound.Delivery, slack_api_base: []const u8) void {
    defer job.deinit(alloc);
    const verdict = deliverWithRetry(pool, alloc, sched, job.*, slack_api_base);
    if (verdict == .retryable) {
        log.warn("outbound_delivery_exhausted", .{ .error_code = ec.ERR_SLACK_OUTBOUND_POST_FAILED, .provider = job.provider, .fleet_id = job.fleet_id });
    }
    connector_outbound.ack(queue, job.entry_id) catch |err|
        log.warn("outbound_ack_failed", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .entry_id = job.entry_id, .err = @errorName(err) });
}

/// Dispatch to the provider poster, retrying a `retryable` verdict up to
/// `MAX_ATTEMPTS` with exponential backoff. Returns the final verdict.
fn deliverWithRetry(pool: *pg.Pool, alloc: std.mem.Allocator, sched: *bounded_fetch.Scheduler, job: connector_outbound.Delivery, slack_api_base: []const u8) slack_post.Outcome {
    var attempt: u32 = 0;
    while (attempt < MAX_ATTEMPTS) : (attempt += 1) {
        switch (dispatch(pool, alloc, sched, job, slack_api_base)) {
            .delivered => return .delivered,
            .permanent => return .permanent,
            .retryable => if (attempt + 1 < MAX_ATTEMPTS) {
                constants.sleepNanos((BACKOFF_BASE_MS << @intCast(attempt)) * std.time.ns_per_ms);
            },
        }
    }
    return .retryable;
}

/// Route one job to its connector poster by `provider`. Adding Grafana/Jira/
/// Linear is one more arm here + a sibling `post.zig` — never a new worker.
fn dispatch(pool: *pg.Pool, alloc: std.mem.Allocator, sched: *bounded_fetch.Scheduler, job: connector_outbound.Delivery, slack_api_base: []const u8) slack_post.Outcome {
    if (std.mem.eql(u8, job.provider, constants.PROVIDER_SLACK)) {
        return slack_post.deliver(alloc, constants.globalIo(), sched, pool, slack_api_base, job.workspace_id, job.fleet_id, job.event_id, job.answer);
    }
    log.warn("outbound_unknown_provider", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .provider = job.provider });
    return .permanent; // unknown provider → drop (acked), don't redeliver forever
}
