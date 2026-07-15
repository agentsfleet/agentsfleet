const std = @import("std");

const model = @import("model.zig");
const support = @import("test_support.zig");

const WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-105000000001";
const FLEET_ID = "0195b4ba-8d3a-7f13-8abc-105000000002";
const SCHEDULE_ID = "0195b4ba-8d3a-7f13-8abc-105000000003";
const LEASE_ONE = "0195b4ba-8d3a-7f13-8abc-105000000004";
const LEASE_TWO = "0195b4ba-8d3a-7f13-8abc-105000000005";
const LEASE_THREE = "0195b4ba-8d3a-7f13-8abc-105000000006";

test "store: schedule row round-trips through create list claim finalize and delete" {
    const alloc = std.testing.allocator;
    var fixture = (try support.Fixture.open(WORKSPACE_ID, FLEET_ID)) orelse return error.SkipZigTest;
    defer fixture.deinit();

    var synced = try support.createAndFinalize(&fixture, alloc, SCHEDULE_ID, LEASE_ONE);
    defer synced.deinit(alloc);
    try std.testing.expectEqual(model.SyncStatus.synced, synced.sync_status);
    try std.testing.expectEqual(@as(i64, 1), synced.generation);
    try std.testing.expectEqual(@as(?[]const u8, null), synced.sync_token);

    var loaded = (try fixture.store.get(alloc, FLEET_ID, SCHEDULE_ID)).?;
    defer loaded.deinit(alloc);
    try std.testing.expectEqualStrings("0 9 * * *", loaded.cron);
    try std.testing.expectEqualStrings("Asia/Kolkata", loaded.timezone);

    const listed = try fixture.store.list(alloc, FLEET_ID);
    defer {
        for (listed) |*schedule| schedule.deinit(alloc);
        alloc.free(listed);
    }
    try std.testing.expectEqual(@as(usize, 1), listed.len);

    const claim_input: model.MutationInput = .{
        .schedule_id = SCHEDULE_ID,
        .fleet_id = FLEET_ID,
        .cron = "30 9 * * 1-5",
        .timezone = "UTC",
        .message = "weekday summary",
        .desired_status = .active,
        .lease_token = LEASE_TWO,
        .now_ms = 300,
        .lease_until_ms = 400,
    };
    var claimed = switch (try fixture.store.claimMutation(alloc, claim_input)) {
        .claimed => |schedule| schedule,
        else => return error.ExpectedClaim,
    };
    defer claimed.deinit(alloc);
    try std.testing.expectEqual(@as(i64, 2), claimed.generation);
    try std.testing.expectEqualStrings("30 9 * * 1-5", claimed.cron);

    var failed = (try fixture.store.finalizeFailure(
        alloc,
        SCHEDULE_ID,
        claimed.generation,
        LEASE_TWO,
        "provider unavailable",
        301,
    )).?;
    defer failed.deinit(alloc);
    try std.testing.expectEqual(model.SyncStatus.failed, failed.sync_status);
    try std.testing.expectEqualStrings("provider unavailable", failed.last_error.?);

    var deleting = switch (try fixture.store.claimMutation(alloc, .{
        .schedule_id = SCHEDULE_ID,
        .fleet_id = FLEET_ID,
        .cron = failed.cron,
        .timezone = failed.timezone,
        .message = failed.message,
        .desired_status = .deleting,
        .lease_token = LEASE_THREE,
        .now_ms = 500,
        .lease_until_ms = 600,
    })) {
        .claimed => |schedule| schedule,
        else => return error.ExpectedDeleteClaim,
    };
    defer deleting.deinit(alloc);
    try std.testing.expect(try fixture.store.deleteClaimed(SCHEDULE_ID, deleting.generation, LEASE_THREE));
    try std.testing.expectEqual(@as(?model.Schedule, null), try fixture.store.get(alloc, FLEET_ID, SCHEDULE_ID));
}

test "store: source identity conflict is distinct from the fleet cap" {
    const alloc = std.testing.allocator;
    var fixture = (try support.Fixture.open(
        "0195b4ba-8d3a-7f13-8abc-105000000011",
        "0195b4ba-8d3a-7f13-8abc-105000000012",
    )) orelse return error.SkipZigTest;
    defer fixture.deinit();

    var first = try support.createAndFinalize(&fixture, alloc, SCHEDULE_ID, LEASE_ONE);
    defer first.deinit(alloc);
    const outcome = try fixture.store.create(alloc, .{
        .fleet_id = fixture.fleet_id,
        .source = .trigger,
        .source_key = SCHEDULE_ID,
        .cron = "0 10 * * *",
        .message = "duplicate source",
    }, "0195b4ba-8d3a-7f13-8abc-105000000013", LEASE_TWO, 200, 300);
    try std.testing.expect(outcome == .source_conflict);
}

test "store: every owned-row allocation failure unwinds without leaks" {
    const alloc = std.testing.allocator;
    var fixture = (try support.Fixture.open(
        "0195b4ba-8d3a-7f13-8abc-105000000021",
        "0195b4ba-8d3a-7f13-8abc-105000000022",
    )) orelse return error.SkipZigTest;
    defer fixture.deinit();
    var synced = try support.createAndFinalize(&fixture, alloc, SCHEDULE_ID, LEASE_ONE);
    defer synced.deinit(alloc);

    for (0..6) |fail_index| {
        var failing = std.testing.FailingAllocator.init(alloc, .{ .fail_index = fail_index });
        try std.testing.expectError(error.OutOfMemory, fixture.store.get(
            failing.allocator(),
            fixture.fleet_id,
            SCHEDULE_ID,
        ));
    }
}
