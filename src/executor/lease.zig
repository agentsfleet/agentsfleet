//! Lease/heartbeat manager for orphan cleanup.
//!
//! Runs a background thread that periodically checks for sessions with
//! expired leases. When a worker disappears (crashes, network partition),
//! the executor cancels and cleans up orphaned sessions.

const std = @import("std");
const logging = @import("log");
const Session = @import("session.zig");
const SessionStore = @import("runtime/session_store.zig");
const executor_metrics = @import("executor_metrics.zig");

const log = logging.scoped(.executor_lease);

/// How often the lease manager scans for expired sessions.
///
/// 5s is intentionally coarse: isLeaseExpired() is pure arithmetic
/// (milliTimestamp delta), so scanning <100 sessions costs ~1µs.
/// With a 30s lease timeout, worst-case orphan lifetime is 35s.
/// Switch to a deadline-heap when concurrent sessions exceed ~500.
const REAP_INTERVAL_MS: u64 = 5_000;

pub const LeaseManager = struct {
    store: *SessionStore,
    running: std.atomic.Value(bool),

    pub fn init(store: *SessionStore) LeaseManager {
        return .{
            .store = store,
            .running = std.atomic.Value(bool).init(false),
        };
    }

    pub fn run(self: *LeaseManager) void {
        self.running.store(true, .release);
        log.info("manager_started", .{ .interval_ms = REAP_INTERVAL_MS });

        while (self.running.load(.acquire)) {
            std.Thread.sleep(REAP_INTERVAL_MS * std.time.ns_per_ms);
            if (!self.running.load(.acquire)) break;

            const reaped = self.store.reapExpired();
            if (reaped > 0) {
                log.warn("expired_reaped", .{ .count = reaped });
            }
        }
    }

    pub fn stop(self: *LeaseManager) void {
        self.running.store(false, .release);
    }
};

test "LeaseManager can be created and stopped" {
    const alloc = std.testing.allocator;
    var store = SessionStore.init(alloc);
    defer store.deinit();

    var manager = LeaseManager.init(&store);
    try std.testing.expect(!manager.running.load(.acquire));

    // Start in background, stop immediately.
    const thread = try std.Thread.spawn(.{}, LeaseManager.run, .{&manager});
    std.Thread.sleep(10 * std.time.ns_per_ms);
    manager.stop();
    thread.join();
    try std.testing.expect(!manager.running.load(.acquire));
}
