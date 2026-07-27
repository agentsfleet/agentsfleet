//! Unit tier for the per-request read lifecycle.
//!
//! The properties under test are the ones that make the scope worth having over
//! hand-placed calls: exactly one outcome per request, a pessimistic default so
//! an unclassified exit is visible, and stage durations that attribute the
//! request's own time rather than floating free of it.

const std = @import("std");
const testing = std.testing;

const scope_mod = @import("library_read_scope.zig");
const stages = @import("library_stages.zig");

/// Every test needs a real `std.Io` because the scope reads the boot clock.
/// Threaded is what the HTTP harness uses, so the clock behaves here as it does
/// in the integration tier.
fn withIo(comptime body: fn (io: std.Io) anyerror!void) !void {
    var threaded = std.Io.Threaded.init(testing.allocator, .{});
    defer threaded.deinit();
    try body(threaded.io());
}

fn unclassifiedExit(io: std.Io) !void {
    stages.resetForTest();
    defer stages.resetForTest();

    {
        var scope = scope_mod.begin(io, .tenant_models);
        defer scope.end();
        // No classify() call — this is the path an author forgot about.
    }

    const snap = stages.snapshot();
    const s = @intFromEnum(stages.Surface.tenant_models);
    // Visible as a fault, NOT as a success. A default of `ok` would make a
    // forgotten exit path indistinguishable from a served page.
    try testing.expectEqual(@as(u64, 1), snap.read_outcomes[s][@intFromEnum(stages.Outcome.internal_error)]);
    try testing.expectEqual(@as(u64, 0), snap.read_outcomes[s][@intFromEnum(stages.Outcome.ok)]);
}

test "an unclassified exit reports internal_error, never ok" {
    try withIo(unclassifiedExit);
}

fn endIsIdempotent(io: std.Io) !void {
    stages.resetForTest();
    defer stages.resetForTest();

    {
        var scope = scope_mod.begin(io, .global_models);
        defer scope.end();
        scope.succeed();
        // An explicit end BEFORE the defer: a handler that ends on its success
        // path and also carries the customary defer must still count once, or
        // every served page is reported twice.
        scope.end();
    }

    const snap = stages.snapshot();
    const s = @intFromEnum(stages.Surface.global_models);
    try testing.expectEqual(@as(u64, 1), snap.read_outcomes[s][@intFromEnum(stages.Outcome.ok)]);
}

test "end is idempotent — one request counts once" {
    try withIo(endIsIdempotent);
}

fn lastClassificationWins(io: std.Io) !void {
    stages.resetForTest();
    defer stages.resetForTest();

    {
        var scope = scope_mod.begin(io, .tenant_models);
        defer scope.end();
        scope.succeed();
        // The over-ceiling body is discovered AFTER the rows were built, so the
        // handler reclassifies late. If the first classification won, an
        // internal fault would be reported as a served page.
        scope.classify(.internal_error);
    }

    const snap = stages.snapshot();
    const s = @intFromEnum(stages.Surface.tenant_models);
    try testing.expectEqual(@as(u64, 1), snap.read_outcomes[s][@intFromEnum(stages.Outcome.internal_error)]);
    try testing.expectEqual(@as(u64, 0), snap.read_outcomes[s][@intFromEnum(stages.Outcome.ok)]);
}

test "the last classification wins, so a late failure is not reported as ok" {
    try withIo(lastClassificationWins);
}

fn stagesAreConsecutive(io: std.Io) !void {
    stages.resetForTest();
    defer stages.resetForTest();

    {
        var scope = scope_mod.begin(io, .fleet_summary);
        defer scope.end();
        scope.endStage(.authorize);
        scope.endStage(.sql);
        scope.endStageWith(.serialize, .{ .bytes = 2048, .count = 12 });
        scope.succeed();
    }

    const snap = stages.snapshot();
    const s = @intFromEnum(stages.Surface.fleet_summary);
    // Each stage recorded exactly one observation — the marker moved rather
    // than every stage re-timing from the request's start.
    try testing.expectEqual(@as(u64, 1), snap.stages[s][@intFromEnum(stages.Stage.authorize)].count);
    try testing.expectEqual(@as(u64, 1), snap.stages[s][@intFromEnum(stages.Stage.sql)].count);
    try testing.expectEqual(@as(u64, 1), snap.stages[s][@intFromEnum(stages.Stage.serialize)].count);
    // A stage the read never ran records nothing.
    try testing.expectEqual(@as(u64, 0), snap.stages[s][@intFromEnum(stages.Stage.cache_lookup)].count);

    try testing.expectEqual(@as(u64, 2048), snap.payload_bytes[s]);
    try testing.expectEqual(@as(u64, 12), snap.results[s]);
}

test "consecutive stages each record once and carry their own detail" {
    try withIo(stagesAreConsecutive);
}

fn poolWaitCarriesItsResult(io: std.Io) !void {
    stages.resetForTest();
    defer stages.resetForTest();

    {
        var scope = scope_mod.begin(io, .tenant_models);
        defer scope.end();
        scope.endStageWith(.pool_wait, .{ .pool_result = .timeout });
        scope.classify(.timeout);
    }

    const snap = stages.snapshot();
    try testing.expectEqual(@as(u64, 1), snap.pool_results[@intFromEnum(stages.PoolResult.timeout)]);
    const s = @intFromEnum(stages.Surface.tenant_models);
    try testing.expectEqual(@as(u64, 1), snap.read_outcomes[s][@intFromEnum(stages.Outcome.timeout)]);
}

test "a pool wait records its result and the read reports the timeout" {
    try withIo(poolWaitCarriesItsResult);
}

fn stageDurationsAreAttributed(io: std.Io) !void {
    stages.resetForTest();
    defer stages.resetForTest();

    var scope = scope_mod.begin(io, .global_models);
    defer scope.end();
    scope.endStage(.sql);
    scope.endStage(.map);
    scope.succeed();

    const snap = stages.snapshot();
    const s = @intFromEnum(stages.Surface.global_models);
    // Real elapsed time, not a placeholder: a scope that recorded zero for every
    // stage would satisfy every count assertion above while measuring nothing.
    // Two clock reads on a threaded Io are never the same instant.
    const total = snap.stages[s][@intFromEnum(stages.Stage.sql)].duration_ns +
        snap.stages[s][@intFromEnum(stages.Stage.map)].duration_ns;
    try testing.expect(total > 0);
}

test "recorded stage durations are real elapsed time" {
    try withIo(stageDurationsAreAttributed);
}
