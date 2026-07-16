//! Concurrency soak for the workspace-stream substrate.
//!
//! The workspace fan-in leans on two shared-substrate properties that only a
//! concurrency test can prove: (1) the hub's shared-consumer map + refcount stay
//! correct when many streams attach, detach, and tear down at once, and (2) the
//! reader's lock-released fan-out (push after snapshot, guarded by ref/unref)
//! never frees a subscription another thread is mid-push on.
//!
//! These run on the hub directly with NO Redis and NO Postgres — the churn is
//! the map edits, the refcount, and the fan-out push, so the test is fully
//! deterministic (no pub/sub delivery timing to flake on) and uses
//! `std.testing.allocator` so any leak or double-free fails the run. The
//! at-scale live-SSE + Redis-kill soak is a separate integration concern; this
//! isolates the substrate's thread-safety, which is where the fan-in's stability
//! actually rests.

const std = @import("std");
const common = @import("common");
const subscription_hub = @import("../../../events/subscription_hub.zig");
const Subscription = subscription_hub.Subscription;

const testing = std.testing;

// Kept modest so the soak stays fast under the shared CI machine, but large
// enough that a missing lock or a refcount slip shows up as a leak, a
// use-after-free, or a hang.
const WORKER_THREADS: usize = 16;
const CHURN_ROUNDS: usize = 400;
const CHANNELS_PER_CONSUMER: usize = 8;

// The name buffers are sized well above the bounded worker/channel indices, so
// bufPrint cannot overflow — a `@panic` names the impossible branch (zlint
// rejects `catch unreachable` for a caught error).
const NAME_BUF_PANIC = "soak: name buffer too small";

fn channelName(buf: []u8, worker: usize, ch: usize) []const u8 {
    return std.fmt.bufPrint(buf, "fleet:soak-{d}-{d}:activity", .{ worker, ch }) catch @panic(NAME_BUF_PANIC);
}

/// One worker: repeatedly stand up a shared consumer, attach a handful of
/// channels, detach them all, and release — the exact lifecycle a workspace
/// stream drives, minus the socket. A cold hub (no connection) makes every wire
/// send a no-op, so this measures the map + refcount paths in isolation.
fn churnWorker(hub: *subscription_hub, worker: usize, failures: *std.atomic.Value(u32)) void {
    var buf: [64]u8 = undefined;
    var round: usize = 0;
    while (round < CHURN_ROUNDS) : (round += 1) {
        var label_buf: [32]u8 = undefined;
        const label = std.fmt.bufPrint(&label_buf, "ws-soak-{d}", .{worker}) catch @panic(NAME_BUF_PANIC);
        const shared = hub.createSharedConsumer(label) catch {
            _ = failures.fetchAdd(1, .monotonic);
            return;
        };
        var ch: usize = 0;
        while (ch < CHANNELS_PER_CONSUMER) : (ch += 1) {
            hub.attachChannel(shared, channelName(&buf, worker, ch)) catch {
                _ = failures.fetchAdd(1, .monotonic);
            };
        }
        // A concurrent reader could be mid-push on any of these; detach + unref
        // must let the LAST reference free it, never the detach that raced.
        ch = 0;
        while (ch < CHANNELS_PER_CONSUMER) : (ch += 1) {
            hub.detachChannel(shared, channelName(&buf, worker, ch));
        }
        shared.unref();
    }
}

test "soak: concurrent shared-consumer churn leaves the hub empty, leak-free, and never wedged" {
    // testing.allocator fails the test on any leak or double-free — the whole
    // point of the soak. A hang (missing wake / lock inversion) shows up as the
    // join never returning, which the test runner's timeout catches.
    var hub = subscription_hub.init(testing.allocator, common.globalIo());
    defer hub.deinit();
    defer hub.stop();

    var failures: std.atomic.Value(u32) = .init(0);
    var threads: [WORKER_THREADS]std.Thread = undefined;
    for (&threads, 0..) |*t, i| {
        t.* = try std.Thread.spawn(.{}, churnWorker, .{ &hub, i, &failures });
    }
    for (&threads) |t| t.join();

    try testing.expectEqual(@as(u32, 0), failures.load(.monotonic));
    // Balanced churn: every attach was detached, so no channel may linger.
    try testing.expectEqual(@as(usize, 0), hub.channelCount());
}

/// The §6 refcount invariant, raced directly: a reader that has snapshotted a
/// subscription and is pushing into it must not be use-after-freed by a
/// concurrent unsubscribe. Simulates the reader's snapshot with an explicit
/// ref+push while another thread detaches and releases.
fn readerPushLoop(shared: *Subscription, stop: *std.atomic.Value(bool)) void {
    while (!stop.load(.acquire)) {
        // Mirror the hub reader: take a ref for the push window, push, drop it.
        shared.ref();
        shared.pushTagged("fleet:soak-race:activity", "{\"kind\":\"chunk\"}");
        shared.unref();
    }
}

test "soak: a subscription being pushed cannot be freed by a concurrent release (refcount)" {
    var hub = subscription_hub.init(testing.allocator, common.globalIo());
    defer hub.deinit();
    defer hub.stop();

    const shared = try hub.createSharedConsumer("ws-race");
    try hub.attachChannel(shared, "fleet:soak-race:activity");

    // Reader thread refs + pushes in a tight loop; it holds a live ref across
    // each push, so the owner's release below can never free under it.
    var stop: std.atomic.Value(bool) = .init(false);
    const reader = try std.Thread.spawn(.{}, readerPushLoop, .{ shared, &stop });

    // Let the reader interleave, then the owner detaches + releases its ref.
    common.sleepNanos(20 * std.time.ns_per_ms);
    hub.detachChannel(shared, "fleet:soak-race:activity");

    stop.store(true, .release);
    reader.join();
    // The reader dropped every ref it took; the owner's unref frees exactly once.
    shared.unref();
    try testing.expectEqual(@as(usize, 0), hub.channelCount());
}
