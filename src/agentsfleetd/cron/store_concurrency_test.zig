const std = @import("std");

const Store = @import("Store.zig");
const model = @import("model.zig");
const support = @import("test_support.zig");

const CONTENDERS: usize = 100;
const MUTATION_WORKSPACE = "0195b4ba-8d3a-7f13-8abc-105000000101";
const MUTATION_FLEET = "0195b4ba-8d3a-7f13-8abc-105000000102";
const MUTATION_SCHEDULE = "0195b4ba-8d3a-7f13-8abc-105000000103";
const INITIAL_LEASE = "0195b4ba-8d3a-7f13-8abc-105000000104";
const MUTATION_NOW_MS: i64 = 1_000;
const MUTATION_LEASE_UNTIL_MS: i64 = 2_000;

const Counts = struct {
    claimed: std.atomic.Value(u32) = .init(0),
    busy: std.atomic.Value(u32) = .init(0),
    capped: std.atomic.Value(u32) = .init(0),
    errors: std.atomic.Value(u32) = .init(0),
};

const ClaimWorker = struct {
    store: Store,
    token: []const u8,
    ready: *std.atomic.Value(u32),
    gate: *std.atomic.Value(bool),
    counts: *Counts,

    fn run(self: ClaimWorker) void {
        _ = self.ready.fetchAdd(1, .acq_rel);
        while (!self.gate.load(.acquire)) std.atomic.spinLoopHint();
        const outcome = self.store.claimMutation(std.heap.page_allocator, .{
            .schedule_id = MUTATION_SCHEDULE,
            .fleet_id = MUTATION_FLEET,
            .cron = "0 10 * * *",
            .timezone = "UTC",
            .message = "contended update",
            .desired_status = .active,
            .lease_token = self.token,
            .now_ms = MUTATION_NOW_MS,
            .lease_until_ms = MUTATION_LEASE_UNTIL_MS,
        }) catch {
            _ = self.counts.errors.fetchAdd(1, .monotonic);
            return;
        };
        switch (outcome) {
            .claimed => |value| {
                var schedule = value;
                schedule.deinit(std.heap.page_allocator);
                _ = self.counts.claimed.fetchAdd(1, .monotonic);
            },
            .busy => _ = self.counts.busy.fetchAdd(1, .monotonic),
            .not_found => _ = self.counts.errors.fetchAdd(1, .monotonic),
        }
    }
};

test "store concurrency: 100 simultaneous mutations have one lease winner" {
    const alloc = std.testing.allocator;
    var fixture = (try support.Fixture.open(MUTATION_WORKSPACE, MUTATION_FLEET)) orelse return error.SkipZigTest;
    defer fixture.deinit();
    var initial = try support.createAndFinalize(&fixture, alloc, MUTATION_SCHEDULE, INITIAL_LEASE);
    defer initial.deinit(alloc);

    var token_bufs: [CONTENDERS][36]u8 = undefined;
    var workers: [CONTENDERS]ClaimWorker = undefined;
    var threads: [CONTENDERS]std.Thread = undefined;
    var ready = std.atomic.Value(u32).init(0);
    var gate = std.atomic.Value(bool).init(false);
    var counts: Counts = .{};
    var spawned: usize = 0;
    errdefer {
        gate.store(true, .release);
        for (threads[0..spawned]) |*thread| thread.join();
    }
    for (&workers, &token_bufs, 0..) |*worker, *token_buf, index| {
        worker.* = .{
            .store = fixture.store,
            .token = try support.indexedUuid(token_buf, 200_000 + index),
            .ready = &ready,
            .gate = &gate,
            .counts = &counts,
        };
        threads[index] = try std.Thread.spawn(.{}, ClaimWorker.run, .{worker.*});
        spawned += 1;
    }
    while (ready.load(.acquire) != CONTENDERS) std.atomic.spinLoopHint();
    gate.store(true, .release);
    for (&threads) |*thread| thread.join();

    try std.testing.expectEqual(@as(u32, 1), counts.claimed.load(.acquire));
    try std.testing.expectEqual(@as(u32, CONTENDERS - 1), counts.busy.load(.acquire));
    try std.testing.expectEqual(@as(u32, 0), counts.errors.load(.acquire));
}

const CreateWorker = struct {
    store: Store,
    fleet_id: []const u8,
    schedule_id: []const u8,
    gate: *std.atomic.Value(bool),
    counts: *Counts,

    fn run(self: CreateWorker) void {
        while (!self.gate.load(.acquire)) std.atomic.spinLoopHint();
        const outcome = self.store.create(std.heap.page_allocator, .{
            .fleet_id = self.fleet_id,
            .source = .api,
            .source_key = self.schedule_id,
            .cron = "0 9 * * *",
            .message = "cap contender",
        }, self.schedule_id, INITIAL_LEASE, 100, 200) catch {
            _ = self.counts.errors.fetchAdd(1, .monotonic);
            return;
        };
        switch (outcome) {
            .created => |value| {
                var schedule = value;
                schedule.deinit(std.heap.page_allocator);
                _ = self.counts.claimed.fetchAdd(1, .monotonic);
            },
            .cap_reached => _ = self.counts.capped.fetchAdd(1, .monotonic),
            else => _ = self.counts.errors.fetchAdd(1, .monotonic),
        }
    }
};

test "store concurrency: racing creates never exceed the 32 schedule Fleet cap" {
    var fixture = (try support.Fixture.open(
        "0195b4ba-8d3a-7f13-8abc-105000000111",
        "0195b4ba-8d3a-7f13-8abc-105000000112",
    )) orelse return error.SkipZigTest;
    defer fixture.deinit();

    for (0..model.MAX_SCHEDULES_PER_FLEET - 1) |index| {
        var id_buf: [36]u8 = undefined;
        const schedule_id = try support.indexedUuid(&id_buf, 300_000 + index);
        var created = switch (try fixture.store.create(std.testing.allocator, .{
            .fleet_id = fixture.fleet_id,
            .source = .api,
            .source_key = schedule_id,
            .cron = "0 9 * * *",
            .message = "cap seed",
        }, schedule_id, INITIAL_LEASE, 100, 200)) {
            .created => |schedule| schedule,
            else => return error.CapSeedFailed,
        };
        created.deinit(std.testing.allocator);
    }

    var id_bufs: [CONTENDERS][36]u8 = undefined;
    var workers: [CONTENDERS]CreateWorker = undefined;
    var threads: [CONTENDERS]std.Thread = undefined;
    var gate = std.atomic.Value(bool).init(false);
    var counts: Counts = .{};
    var spawned: usize = 0;
    errdefer {
        gate.store(true, .release);
        for (threads[0..spawned]) |*thread| thread.join();
    }
    for (&workers, &id_bufs, 0..) |*worker, *id_buf, index| {
        const schedule_id = try support.indexedUuid(id_buf, 400_000 + index);
        worker.* = .{
            .store = fixture.store,
            .fleet_id = fixture.fleet_id,
            .schedule_id = schedule_id,
            .gate = &gate,
            .counts = &counts,
        };
        threads[index] = try std.Thread.spawn(.{}, CreateWorker.run, .{worker.*});
        spawned += 1;
    }
    gate.store(true, .release);
    for (&threads) |*thread| thread.join();

    try std.testing.expectEqual(@as(u32, 1), counts.claimed.load(.acquire));
    try std.testing.expectEqual(@as(u32, CONTENDERS - 1), counts.capped.load(.acquire));
    try std.testing.expectEqual(@as(u32, 0), counts.errors.load(.acquire));

    const rows = try fixture.store.list(std.testing.allocator, fixture.fleet_id);
    defer {
        for (rows) |*schedule| schedule.deinit(std.testing.allocator);
        std.testing.allocator.free(rows);
    }
    try std.testing.expectEqual(model.MAX_SCHEDULES_PER_FLEET, rows.len);
}
