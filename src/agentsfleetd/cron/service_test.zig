const std = @import("std");
const pg = @import("pg");
const common = @import("common");

const QStashClient = @import("QStashClient.zig");
const Service = @import("Service.zig");
const support = @import("test_support.zig");

const POOL_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-105000000301";
const POOL_FLEET_ID = "0195b4ba-8d3a-7f13-8abc-105000000302";
const RECOVER_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-105000000311";
const RECOVER_FLEET_ID = "0195b4ba-8d3a-7f13-8abc-105000000312";
const OOM_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-105000000321";
const OOM_FLEET_ID = "0195b4ba-8d3a-7f13-8abc-105000000322";
const CONCURRENCY_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-105000000331";
const CONCURRENCY_FLEET_ID = "0195b4ba-8d3a-7f13-8abc-105000000332";
const CONCURRENCY_SCHEDULE_ID = "0195b4ba-8d3a-7f13-8abc-105000000333";
const LEASE_TOKEN = "0195b4ba-8d3a-7f13-8abc-105000000304";
const TOKEN = "qstash-service-test-token";
const DESTINATION = "https://api.agentsfleet.net/v1/ingress/qstash/schedules";
const CONTENDERS: usize = 100;

const Fake = struct {
    pool: ?*pg.Pool = null,
    status: u16 = 200,
    failure: ?anyerror = null,
    probe_pool: bool = false,
    pool_probe_ok: bool = false,
    calls: std.atomic.Value(u32) = .init(0),
    deletes: std.atomic.Value(u32) = .init(0),
    entered: std.atomic.Value(bool) = .init(false),
    block: std.atomic.Value(bool) = .init(false),

    fn exchange(self: *Fake) QStashClient.Exchange {
        return .{ .ptr = self, .callFn = call };
    }

    fn call(ptr: *anyopaque, alloc: std.mem.Allocator, request: QStashClient.Request) anyerror!QStashClient.Response {
        const self: *Fake = @ptrCast(@alignCast(ptr));
        _ = self.calls.fetchAdd(1, .monotonic);
        if (self.probe_pool) {
            const pool = self.pool orelse return error.MissingPool;
            const conn = pool.acquire() catch return error.PoolStillHeld;
            pool.release(conn);
            self.pool_probe_ok = true;
        }
        self.entered.store(true, .release);
        while (self.block.load(.acquire)) std.atomic.spinLoopHint();
        if (self.failure) |failure| return failure;
        if (request.method == .DELETE) {
            _ = self.deletes.fetchAdd(1, .monotonic);
            return .{ .status = self.status, .body = try alloc.dupe(u8, "") };
        }
        const Body = struct { schedule_id: []const u8, generation: i64 };
        var parsed = try std.json.parseFromSlice(Body, alloc, request.body, .{});
        defer parsed.deinit();
        const response = try std.json.Stringify.valueAlloc(alloc, .{ .scheduleId = parsed.value.schedule_id }, .{});
        return .{ .status = self.status, .body = response };
    }
};

fn client(fake: *Fake) QStashClient {
    return QStashClient.init(fake.exchange(), "https://qstash.test", DESTINATION);
}

test "service: provider I/O runs after releasing the only database connection" {
    const alloc = std.testing.allocator;
    var fixture = (try support.Fixture.openSingle(POOL_WORKSPACE_ID, POOL_FLEET_ID)) orelse return error.SkipZigTest;
    defer fixture.deinit();
    var fake: Fake = .{ .pool = fixture.pool, .probe_pool = true };
    const service = Service.init(fixture.store, client(&fake), TOKEN);
    var outcome = try service.create(alloc, .{
        .fleet_id = POOL_FLEET_ID,
        .source = .api,
        .source_key = "api:pool-probe",
        .cron = "0 9 * * *",
        .timezone = "Asia/Kolkata",
        .message = "summarize",
    });
    defer outcome.deinit(alloc);
    try std.testing.expect(fake.pool_probe_ok);
    try std.testing.expectEqual(@as(u32, 1), fake.calls.load(.acquire));
    switch (outcome) {
        .schedule => |schedule| try std.testing.expectEqual(.synced, schedule.sync_status),
        else => return error.UnexpectedOutcome,
    }
    const schedule_id = switch (outcome) {
        .schedule => |schedule| try alloc.dupe(u8, schedule.schedule_id),
        else => unreachable,
    };
    defer alloc.free(schedule_id);
    var paused = try service.update(alloc, .{
        .fleet_id = POOL_FLEET_ID,
        .schedule_id = schedule_id,
        .cron = "0 10 * * *",
        .timezone = "Asia/Kolkata",
        .message = "pause me",
        .desired_status = .paused,
    });
    defer paused.deinit(alloc);
    switch (paused) {
        .schedule => |schedule| try std.testing.expectEqual(.paused, schedule.desired_status),
        else => return error.UnexpectedOutcome,
    }
    var resumed = try service.update(alloc, .{
        .fleet_id = POOL_FLEET_ID,
        .schedule_id = schedule_id,
        .cron = "0 10 * * *",
        .timezone = "Asia/Kolkata",
        .message = "resume me",
        .desired_status = .active,
    });
    defer resumed.deinit(alloc);
    switch (resumed) {
        .schedule => |schedule| try std.testing.expectEqual(.active, schedule.desired_status),
        else => return error.UnexpectedOutcome,
    }
    var removed = try service.remove(alloc, POOL_FLEET_ID, schedule_id);
    defer removed.deinit(alloc);
    try std.testing.expect(removed == .deleted);
    try std.testing.expect((try fixture.store.get(alloc, POOL_FLEET_ID, schedule_id)) == null);
    try std.testing.expectEqual(@as(u32, 4), fake.calls.load(.acquire));
}

test "service: provider failure is durable and explicit sync recovers the newest generation" {
    const alloc = std.testing.allocator;
    var fixture = (try support.Fixture.open(RECOVER_WORKSPACE_ID, RECOVER_FLEET_ID)) orelse return error.SkipZigTest;
    defer fixture.deinit();
    var fake: Fake = .{ .status = 503 };
    const service = Service.init(fixture.store, client(&fake), TOKEN);
    var failed = try service.create(alloc, .{
        .fleet_id = RECOVER_FLEET_ID,
        .source = .api,
        .source_key = "api:recover",
        .cron = "0 9 * * *",
        .timezone = "UTC",
        .message = "recover me",
    });
    const schedule_id = switch (failed) {
        .provider_failed => |failure| try alloc.dupe(u8, failure.schedule.schedule_id),
        else => return error.UnexpectedOutcome,
    };
    defer alloc.free(schedule_id);
    defer failed.deinit(alloc);
    fake.status = 200;
    var recovered = try service.sync(alloc, RECOVER_FLEET_ID, schedule_id);
    defer recovered.deinit(alloc);
    switch (recovered) {
        .schedule => |schedule| {
            try std.testing.expectEqual(.synced, schedule.sync_status);
            try std.testing.expectEqual(@as(i64, 2), schedule.generation);
            try std.testing.expect(schedule.last_error == null);
        },
        else => return error.UnexpectedOutcome,
    }
}

test "service: explicit sync claims current row state" {
    const alloc = std.testing.allocator;
    var fixture = (try support.Fixture.open(
        "0195b4ba-8d3a-7f13-8abc-105000000341",
        "0195b4ba-8d3a-7f13-8abc-105000000342",
    )) orelse return error.SkipZigTest;
    defer fixture.deinit();
    const schedule_id = "0195b4ba-8d3a-7f13-8abc-105000000343";
    var seeded = try support.createAndFinalize(&fixture, alloc, schedule_id, LEASE_TOKEN);
    defer seeded.deinit(alloc);
    const conn = try fixture.pool.acquire();
    defer fixture.pool.release(conn);
    _ = try conn.exec(
        \\UPDATE core.fleet_schedules
        \\SET desired_status = 'paused', sync_status = 'failed', updated_at = $2
        \\WHERE uid = $1::uuid
    , .{ schedule_id, common.clock.nowMillis() });
    var fake: Fake = .{};
    const service = Service.init(fixture.store, client(&fake), TOKEN);
    var synced = try service.sync(alloc, fixture.fleet_id, schedule_id);
    defer synced.deinit(alloc);
    switch (synced) {
        .schedule => |schedule| try std.testing.expectEqual(.paused, schedule.desired_status),
        else => return error.UnexpectedOutcome,
    }
    try std.testing.expectEqual(@as(u32, 1), fake.deletes.load(.acquire));
}

test "service: partial update preserves current row fields" {
    const alloc = std.testing.allocator;
    var fixture = (try support.Fixture.open(
        "0195b4ba-8d3a-7f13-8abc-105000000351",
        "0195b4ba-8d3a-7f13-8abc-105000000352",
    )) orelse return error.SkipZigTest;
    defer fixture.deinit();
    const schedule_id = "0195b4ba-8d3a-7f13-8abc-105000000353";
    var seeded = try support.createAndFinalize(&fixture, alloc, schedule_id, LEASE_TOKEN);
    defer seeded.deinit(alloc);
    const conn = try fixture.pool.acquire();
    defer fixture.pool.release(conn);
    _ = try conn.exec(
        \\UPDATE core.fleet_schedules
        \\SET cron_expression = '0 12 * * *', updated_at = $2
        \\WHERE uid = $1::uuid
    , .{ schedule_id, common.clock.nowMillis() });
    var fake: Fake = .{};
    const service = Service.init(fixture.store, client(&fake), TOKEN);
    var patched = try service.update(alloc, .{
        .fleet_id = fixture.fleet_id,
        .schedule_id = schedule_id,
        .message = "patched only the message",
    });
    defer patched.deinit(alloc);
    switch (patched) {
        .schedule => |schedule| {
            try std.testing.expectEqualStrings("0 12 * * *", schedule.cron);
            try std.testing.expectEqualStrings("patched only the message", schedule.message);
        },
        else => return error.UnexpectedOutcome,
    }
}
test "service: source desired update preserves current schedule fields" {
    const alloc = std.testing.allocator;
    var fixture = (try support.Fixture.open(
        "0195b4ba-8d3a-7f13-8abc-105000000361",
        "0195b4ba-8d3a-7f13-8abc-105000000362",
    )) orelse return error.SkipZigTest;
    defer fixture.deinit();
    const schedule_id = "0195b4ba-8d3a-7f13-8abc-105000000363";
    var seeded = try support.createAndFinalize(&fixture, alloc, schedule_id, LEASE_TOKEN);
    defer seeded.deinit(alloc);
    const conn = try fixture.pool.acquire();
    defer fixture.pool.release(conn);
    _ = try conn.exec(
        \\UPDATE core.fleet_schedules
        \\SET cron_expression = '15 7 * * *', message = 'current trigger', updated_at = $2
        \\WHERE uid = $1::uuid
    , .{ schedule_id, common.clock.nowMillis() });
    var fake: Fake = .{};
    const service = Service.init(fixture.store, client(&fake), TOKEN);
    var paused = try service.setSourceDesired(alloc, fixture.fleet_id, schedule_id, .paused);
    defer paused.deinit(alloc);
    switch (paused) {
        .schedule => |schedule| {
            try std.testing.expectEqual(.paused, schedule.desired_status);
            try std.testing.expectEqualStrings("15 7 * * *", schedule.cron);
            try std.testing.expectEqualStrings("current trigger", schedule.message);
        },
        else => return error.UnexpectedOutcome,
    }
    try std.testing.expectEqual(@as(u32, 1), fake.deletes.load(.acquire));
}

test "service: provider out of memory clears the lease into durable failed state" {
    const alloc = std.testing.allocator;
    var fixture = (try support.Fixture.open(OOM_WORKSPACE_ID, OOM_FLEET_ID)) orelse return error.SkipZigTest;
    defer fixture.deinit();
    var fake: Fake = .{ .failure = error.OutOfMemory };
    const service = Service.init(fixture.store, client(&fake), TOKEN);
    try std.testing.expectError(error.OutOfMemory, service.create(alloc, .{
        .fleet_id = OOM_FLEET_ID,
        .source = .api,
        .source_key = "api:oom",
        .cron = "0 9 * * *",
        .timezone = "UTC",
        .message = "persist failure",
    }));
    const rows = try fixture.store.list(alloc, OOM_FLEET_ID);
    defer {
        for (rows) |*row| row.deinit(alloc);
        alloc.free(rows);
    }
    try std.testing.expectEqual(@as(usize, 1), rows.len);
    try std.testing.expectEqual(.failed, rows[0].sync_status);
    try std.testing.expect(rows[0].sync_token == null);
    try std.testing.expect(rows[0].last_error != null);
}

const Counts = struct {
    synced: std.atomic.Value(u32) = .init(0),
    busy: std.atomic.Value(u32) = .init(0),
    errors: std.atomic.Value(u32) = .init(0),
};

const Worker = struct {
    service: Service,
    fleet_id: []const u8,
    schedule_id: []const u8,
    ready: *std.atomic.Value(u32),
    gate: *std.atomic.Value(bool),
    counts: *Counts,

    fn run(self: Worker) void {
        _ = self.ready.fetchAdd(1, .acq_rel);
        while (!self.gate.load(.acquire)) std.atomic.spinLoopHint();
        var outcome = self.service.sync(std.heap.page_allocator, self.fleet_id, self.schedule_id) catch {
            _ = self.counts.errors.fetchAdd(1, .monotonic);
            return;
        };
        defer outcome.deinit(std.heap.page_allocator);
        switch (outcome) {
            .schedule => _ = self.counts.synced.fetchAdd(1, .monotonic),
            .busy => _ = self.counts.busy.fetchAdd(1, .monotonic),
            else => _ = self.counts.errors.fetchAdd(1, .monotonic),
        }
    }
};

test "service concurrency: 100 simultaneous syncs make one provider call" {
    const alloc = std.testing.allocator;
    var fixture = (try support.Fixture.openContended(CONCURRENCY_WORKSPACE_ID, CONCURRENCY_FLEET_ID)) orelse return error.SkipZigTest;
    defer fixture.deinit();
    var seeded = try support.createAndFinalize(&fixture, alloc, CONCURRENCY_SCHEDULE_ID, LEASE_TOKEN);
    defer seeded.deinit(alloc);
    var fake: Fake = .{};
    fake.block.store(true, .release);
    const service = Service.init(fixture.store, client(&fake), TOKEN);
    var ready = std.atomic.Value(u32).init(0);
    var gate = std.atomic.Value(bool).init(false);
    var counts: Counts = .{};
    var workers: [CONTENDERS]Worker = undefined;
    var threads: [CONTENDERS]std.Thread = undefined;
    var spawned: usize = 0;
    errdefer {
        gate.store(true, .release);
        fake.block.store(false, .release);
        for (threads[0..spawned]) |*thread| thread.join();
    }
    for (&workers, 0..) |*worker, index| {
        worker.* = .{
            .service = service,
            .fleet_id = CONCURRENCY_FLEET_ID,
            .schedule_id = CONCURRENCY_SCHEDULE_ID,
            .ready = &ready,
            .gate = &gate,
            .counts = &counts,
        };
        threads[index] = try std.Thread.spawn(.{}, Worker.run, .{worker.*});
        spawned += 1;
    }
    while (ready.load(.acquire) != CONTENDERS) std.atomic.spinLoopHint();
    gate.store(true, .release);
    const deadline = common.clock.nowMillis() + 10_000;
    while (counts.busy.load(.acquire) + counts.errors.load(.acquire) < CONTENDERS - 1 and
        common.clock.nowMillis() < deadline) std.atomic.spinLoopHint();
    fake.block.store(false, .release);
    for (&threads) |*thread| thread.join();
    spawned = 0;
    try std.testing.expectEqual(@as(u32, 1), fake.calls.load(.acquire));
    try std.testing.expectEqual(@as(u32, 1), counts.synced.load(.acquire));
    try std.testing.expectEqual(@as(u32, CONTENDERS - 1), counts.busy.load(.acquire));
    try std.testing.expectEqual(@as(u32, 0), counts.errors.load(.acquire));
}
