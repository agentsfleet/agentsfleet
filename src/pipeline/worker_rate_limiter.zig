const std = @import("std");
const rate_limit = @import("../reliability/rate_limit.zig");
const metrics = @import("../observability/metrics.zig");
const worker_runtime = @import("worker_runtime.zig");

pub const TenantRateLimiter = struct {
    alloc: std.mem.Allocator,
    buckets: std.StringHashMap(rate_limit.TokenBucket),
    capacity: u32,
    refill_per_sec: f64,

    pub fn init(alloc: std.mem.Allocator, capacity: u32, refill_per_sec: f64) TenantRateLimiter {
        return .{
            .alloc = alloc,
            .buckets = std.StringHashMap(rate_limit.TokenBucket).init(alloc),
            .capacity = capacity,
            .refill_per_sec = refill_per_sec,
        };
    }

    pub fn deinit(self: *TenantRateLimiter) void {
        var it = self.buckets.iterator();
        while (it.next()) |entry| self.alloc.free(entry.key_ptr.*);
        self.buckets.deinit();
    }

    pub fn acquire(self: *TenantRateLimiter, tenant_id: []const u8, provider: []const u8, cost: f64) !void {
        while (true) {
            const now_ms = std.time.milliTimestamp();
            const bucket = try self.getOrCreateBucket(tenant_id, provider, now_ms);
            if (bucket.allow(now_ms, cost)) return;

            const wait_ms = @max(bucket.waitMsUntil(now_ms, cost), 1);
            metrics.addRateLimitWaitMs(wait_ms);
            std.Thread.sleep(wait_ms * std.time.ns_per_ms);
        }
    }

    pub fn acquireCancelable(
        self: *TenantRateLimiter,
        tenant_id: []const u8,
        provider: []const u8,
        cost: f64,
        cancel_flag: *const std.atomic.Value(bool),
        deadline_ms: i64,
    ) worker_runtime.WorkerError!void {
        while (true) {
            try worker_runtime.ensureRunActive(cancel_flag, deadline_ms);

            const now_ms = std.time.milliTimestamp();
            const bucket = self.getOrCreateBucket(tenant_id, provider, now_ms) catch |err| switch (err) {
                error.OutOfMemory, error.NoSpaceLeft => return worker_runtime.WorkerError.InvalidPipelineProfile,
            };
            if (bucket.allow(now_ms, cost)) return;

            const wait_ms = @max(bucket.waitMsUntil(now_ms, cost), 1);
            metrics.addRateLimitWaitMs(wait_ms);
            try worker_runtime.sleepCooperative(wait_ms, cancel_flag, deadline_ms);
        }
    }

    pub fn getOrCreateBucket(
        self: *TenantRateLimiter,
        tenant_id: []const u8,
        provider: []const u8,
        now_ms: i64,
    ) !*rate_limit.TokenBucket {
        var key_buf: [256]u8 = undefined;
        var heap_key: ?[]u8 = null;
        const scoped_key = std.fmt.bufPrint(&key_buf, "{s}::{s}", .{ tenant_id, provider }) catch |err| blk: {
            if (err != error.NoSpaceLeft) return err;
            const allocated = try std.fmt.allocPrint(self.alloc, "{s}::{s}", .{ tenant_id, provider });
            heap_key = allocated;
            break :blk allocated;
        };
        defer if (heap_key) |k| self.alloc.free(k);

        if (self.buckets.getPtr(scoped_key)) |bucket| return bucket;

        const key_copy = try self.alloc.dupe(u8, scoped_key);
        errdefer self.alloc.free(key_copy);

        try self.buckets.put(key_copy, rate_limit.TokenBucket.init(self.capacity, self.refill_per_sec, now_ms));
        return self.buckets.getPtr(key_copy).?;
    }
};

test "integration: rate limiter scopes by tenant and provider" {
    var limiter = TenantRateLimiter.init(std.testing.allocator, 1, 1.0);
    defer limiter.deinit();

    const now_ms = std.time.milliTimestamp();
    const scout_bucket = try limiter.getOrCreateBucket("tenant-a", "agent_scout", now_ms);
    const scout_bucket_again = try limiter.getOrCreateBucket("tenant-a", "agent_scout", now_ms);
    const pr_bucket = try limiter.getOrCreateBucket("tenant-a", "github_pr_create", now_ms);

    try std.testing.expect(scout_bucket == scout_bucket_again);
    try std.testing.expect(scout_bucket != pr_bucket);

    try std.testing.expect(scout_bucket.allow(now_ms, 1.0));
    try std.testing.expect(!scout_bucket.allow(now_ms, 1.0));
    try std.testing.expect(pr_bucket.allow(now_ms, 1.0));
}

test "integration: rate limiter avoids false positives for prefix-similar tenants" {
    var limiter = TenantRateLimiter.init(std.testing.allocator, 1, 1.0);
    defer limiter.deinit();

    const now_ms = std.time.milliTimestamp();
    const a = try limiter.getOrCreateBucket("tenant-a", "provider-x", now_ms);
    const b = try limiter.getOrCreateBucket("tenant-a1", "provider-x", now_ms);
    try std.testing.expect(a != b);
}

test "integration: rate limiter cancelable path honors shutdown signal" {
    var limiter = TenantRateLimiter.init(std.testing.allocator, 1, 1.0);
    defer limiter.deinit();

    const now_ms = std.time.milliTimestamp();
    const bucket = try limiter.getOrCreateBucket("tenant-z", "provider-z", now_ms);
    try std.testing.expect(bucket.allow(now_ms, 1.0));

    var running = std.atomic.Value(bool).init(false);
    try std.testing.expectError(
        worker_runtime.WorkerError.ShutdownRequested,
        limiter.acquireCancelable("tenant-z", "provider-z", 1.0, &running, std.time.milliTimestamp() + 10_000),
    );
}

test "integration: rate limiter cancelable path honors deadline" {
    var limiter = TenantRateLimiter.init(std.testing.allocator, 1, 1.0);
    defer limiter.deinit();

    const now_ms = std.time.milliTimestamp();
    const bucket = try limiter.getOrCreateBucket("tenant-d", "provider-d", now_ms);
    try std.testing.expect(bucket.allow(now_ms, 1.0));

    var running = std.atomic.Value(bool).init(true);
    try std.testing.expectError(
        worker_runtime.WorkerError.RunDeadlineExceeded,
        limiter.acquireCancelable("tenant-d", "provider-d", 1.0, &running, std.time.milliTimestamp() - 1),
    );
}

test "integration: tenant limiter deinit releases scoped keys under churn" {
    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    defer {
        const leaked = gpa.deinit();
        std.testing.expectEqual(@as(std.heap.Check, .ok), leaked) catch unreachable;
    }

    var limiter = TenantRateLimiter.init(gpa.allocator(), 2, 5.0);
    defer limiter.deinit();

    var i: usize = 0;
    while (i < 1_000) : (i += 1) {
        var tenant_buf: [32]u8 = undefined;
        var provider_buf: [32]u8 = undefined;
        const tenant = try std.fmt.bufPrint(&tenant_buf, "tenant-{d}", .{i % 50});
        const provider = try std.fmt.bufPrint(&provider_buf, "provider-{d}", .{i % 20});
        _ = try limiter.getOrCreateBucket(tenant, provider, std.time.milliTimestamp());
    }
}

test "integration: tenant limiter long scoped keys use fallback path without leaks" {
    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    defer {
        const leaked = gpa.deinit();
        std.testing.expectEqual(@as(std.heap.Check, .ok), leaked) catch unreachable;
    }

    var limiter = TenantRateLimiter.init(gpa.allocator(), 2, 5.0);
    defer limiter.deinit();

    const tenant = "tenant-xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx";
    const provider = "provider-yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy";

    var i: usize = 0;
    while (i < 200) : (i += 1) {
        _ = try limiter.getOrCreateBucket(tenant, provider, std.time.milliTimestamp());
    }
}
